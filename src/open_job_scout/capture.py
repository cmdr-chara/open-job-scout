"""Capture a public job page into a local Job record.

The capture path is deliberately conservative: it reads public HTML and JSON-LD,
keeps a normalized source URL and page description, and never submits forms or
follows application flows.
"""

from __future__ import annotations

import json
import math
import re
from html.parser import HTMLParser
from typing import Any
from urllib.parse import urljoin, urlsplit

from .models import Job, normalize_job_url
from .verification import is_safe_public_url, resolve_url

MAX_CAPTURE_DESCRIPTION = 100_000


class _PageParser(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.title_parts: list[str] = []
        self.visible_parts: list[str] = []
        self.meta: dict[str, str] = {}
        self.canonical: str | None = None
        self._title_depth = 0
        self._ignored_depth = 0
        self._jsonld_depth = 0
        self._jsonld_parts: list[str] = []
        self.jsonld_blocks: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        attributes = {key.lower(): value or "" for key, value in attrs}
        tag = tag.lower()
        if tag == "title":
            self._title_depth += 1
        if tag in {"script", "style", "noscript", "svg"}:
            self._ignored_depth += 1
        if tag == "script" and attributes.get("type", "").lower() == "application/ld+json":
            self._jsonld_depth += 1
            self._jsonld_parts.clear()
        if tag == "meta":
            key = (attributes.get("property") or attributes.get("name") or "").lower()
            content = attributes.get("content", "").strip()
            if key and content:
                self.meta[key] = content
        if tag == "link" and attributes.get("rel", "").lower() == "canonical":
            value = attributes.get("href", "").strip()
            if value:
                self.canonical = value

    def handle_endtag(self, tag: str) -> None:
        tag = tag.lower()
        if tag == "title" and self._title_depth:
            self._title_depth -= 1
        if tag == "script" and self._jsonld_depth:
            self._jsonld_depth -= 1
            if self._jsonld_parts:
                self.jsonld_blocks.append("".join(self._jsonld_parts).strip())
                self._jsonld_parts.clear()
        if tag in {"script", "style", "noscript", "svg"} and self._ignored_depth:
            self._ignored_depth -= 1

    def handle_data(self, data: str) -> None:
        if self._jsonld_depth:
            self._jsonld_parts.append(data)
        if self._title_depth:
            self.title_parts.append(data)
        if not self._ignored_depth and data.strip():
            self.visible_parts.append(data)


def _text(value: object, *, collapse: bool = True) -> str:
    if not isinstance(value, str):
        return ""
    value = re.sub(r"<[^>]+>", " ", value)
    return re.sub(r"\s+", " ", value).strip() if collapse else value.strip()


def _jsonld_records(parser: _PageParser) -> list[dict[str, Any]]:
    records: list[dict[str, Any]] = []
    for block in parser.jsonld_blocks:
        try:
            value = json.loads(block)
        except json.JSONDecodeError:
            continue
        roots = value if isinstance(value, list) else [value]
        for item in roots:
            if not isinstance(item, dict):
                continue
            graph = item.get("@graph")
            candidates = graph if isinstance(graph, list) else [item]
            records.extend(candidate for candidate in candidates if isinstance(candidate, dict))
    return records


def _job_posting(records: list[dict[str, Any]]) -> dict[str, Any]:
    for record in records:
        kind = record.get("@type", "")
        kinds = kind if isinstance(kind, list) else [kind]
        if any(str(value).lower() == "jobposting" for value in kinds):
            return record
    return {}


def _nested_name(value: object) -> str:
    if isinstance(value, dict):
        return _text(value.get("name"))
    return _text(value)


def _scalar_text(value: object) -> str:
    if isinstance(value, list):
        return ", ".join(_text(item) for item in value if _text(item))
    return _text(value)


def _location(value: object) -> str | None:
    values = value if isinstance(value, list) else [value]
    locations: list[str] = []
    for item in values:
        if not isinstance(item, dict):
            continue
        address = item.get("address", item)
        if isinstance(address, dict):
            parts = [
                _text(address.get(key))
                for key in ("streetAddress", "addressLocality", "addressRegion", "addressCountry")
            ]
            label = ", ".join(part for part in parts if part)
        else:
            label = _text(address)
        if label and label not in locations:
            locations.append(label)
    return "; ".join(locations) or None


def _salary(value: object) -> tuple[float | None, float | None, str | None]:
    if not isinstance(value, dict):
        return None, None, None
    currency = _text(value.get("currency")) or None
    amount = value.get("value")
    if not isinstance(amount, dict):
        amount = amount if amount is not None else value

    def number(key: str) -> float | None:
        if isinstance(amount, dict):
            candidate = amount.get(key)
        elif key == "value":
            candidate = amount
        else:
            candidate = None
        try:
            result = float(candidate)
        except (TypeError, ValueError):
            return None
        return result if result >= 0 and math.isfinite(result) else None
    minimum = number("minValue")
    maximum = number("maxValue")
    if minimum is None and maximum is None:
        maximum = number("value")
        minimum = maximum
    return minimum, maximum, currency


def _company_from_host(url: str) -> str:
    host = (urlsplit(url).hostname or "").removeprefix("www.")
    label = host.split(".")[0].replace("-", " ").strip()
    return label.title() or "Captured employer"


def capture_job(
    url: str,
    *,
    title: str | None = None,
    company: str | None = None,
    location: str | None = None,
) -> Job:
    """Capture one public posting without submitting or interacting with it."""
    url = url.strip()
    if not is_safe_public_url(url):
        raise ValueError("Capture requires a public HTTP(S) URL.")
    source_url = normalize_job_url(url) or url
    verification, final_url, body = resolve_url(url)
    if verification == "unreachable":
        raise RuntimeError("The public job page could not be reached.")

    parser = _PageParser()
    parser.feed(body)
    records = _jsonld_records(parser)
    posting = _job_posting(records)
    metadata = parser.meta
    resolved_url = final_url if is_safe_public_url(final_url) else url
    if parser.canonical:
        canonical_url = urljoin(resolved_url, parser.canonical)
        if is_safe_public_url(canonical_url):
            resolved_url = canonical_url
    parsed_title = (
        _text(title)
        or _text(posting.get("title"))
        or metadata.get("og:title", "")
        or _text(" ".join(parser.title_parts))
    )
    parsed_company = (
        _text(company)
        or _nested_name(posting.get("hiringOrganization"))
        or metadata.get("og:site_name", "")
        or _company_from_host(resolved_url)
    )
    if not parsed_title:
        raise ValueError("Could not find a job title; pass --title to provide one.")

    salary_min, salary_max, currency = _salary(posting.get("baseSalary"))
    work_mode = (
        "remote"
        if str(posting.get("jobLocationType", "")).lower() == "telecommute"
        else "unknown"
    )
    remote = True if work_mode == "remote" else None
    description = _text(posting.get("description"), collapse=False)
    if not description:
        description = _text(" ".join(parser.visible_parts))
    return Job(
        title=parsed_title,
        company=parsed_company,
        source_url=source_url,
        canonical_url=resolved_url,
        original_canonical_url=resolved_url,
        location=_text(location) or _location(posting.get("jobLocation")),
        remote=remote,
        work_mode=work_mode,
        employment_type=_scalar_text(posting.get("employmentType")) or None,
        salary_min=salary_min,
        salary_max=salary_max,
        currency=currency,
        salary_source="capture" if salary_min is not None or salary_max is not None else None,
        description=description[:MAX_CAPTURE_DESCRIPTION],
        posted_at=_text(posting.get("datePosted")) or None,
        source="capture",
        verification_status="closed" if verification == "closed" else "verified",
    )
