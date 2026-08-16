from __future__ import annotations

import csv
import math
import re
import sys
from collections.abc import Iterable
from html.parser import HTMLParser
from pathlib import Path
from typing import Any
from urllib.parse import urlsplit

from .firecrawl import discover_firecrawl, settings_from_config
from .models import Job, job_dedup_key

_MISSING_VALUES = {"", "<na>", "na", "nan", "nat", "none", "null"}
_MAX_DESCRIPTION_CHARS = 1_000_000


class _DescriptionParser(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self._ignored_depth = 0
        self.parts: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        if tag in {"script", "style", "noscript", "svg"}:
            self._ignored_depth += 1

    def handle_endtag(self, tag: str) -> None:
        if tag in {"script", "style", "noscript", "svg"} and self._ignored_depth:
            self._ignored_depth -= 1

    def handle_data(self, data: str) -> None:
        if not self._ignored_depth:
            self.parts.append(data)


def _plain_description(value: object) -> str:
    if value is None or (isinstance(value, float) and math.isnan(value)):
        return ""
    parser = _DescriptionParser()
    parser.feed(str(value)[:_MAX_DESCRIPTION_CHARS])
    return re.sub(r"\s+", " ", " ".join(parser.parts)).strip()


def _clean_http_url(value: object) -> str | None:
    """Return a usable HTTP(S) URL, or None for missing/scraped placeholder values."""
    if value is None or (isinstance(value, float) and math.isnan(value)):
        return None
    text = str(value).strip()
    if text.lower() in _MISSING_VALUES or any(character.isspace() for character in text):
        return None
    parts = urlsplit(text)
    if parts.scheme.lower() not in {"http", "https"} or not parts.hostname:
        return None
    return text


def _clean_text(value: object) -> str:
    if value is None or (isinstance(value, float) and math.isnan(value)):
        return ""
    text = re.sub(r"\s+", " ", str(value)).strip()
    return "" if text.lower() in _MISSING_VALUES else text


def _first_url(*values: object) -> str | None:
    return next((url for value in values if (url := _clean_http_url(value))), None)


def _float(value: object) -> float | None:
    try:
        if value in (None, ""):
            return None
        parsed = float(value)
        return parsed if math.isfinite(parsed) else None
    except (TypeError, ValueError):
        return None


def _first_float(*values: object) -> float | None:
    return next((number for value in values if (number := _float(value)) is not None), None)


def _bool(value: object) -> bool | None:
    if isinstance(value, bool):
        return value
    lowered = str(value or "").strip().lower()
    if lowered in {"true", "1", "yes", "remote"}:
        return True
    if lowered in {"false", "0", "no", "onsite", "on-site"}:
        return False
    return None


def row_to_job(row: dict[str, Any]) -> Job:
    low = {str(key).lower(): value for key, value in row.items()}
    # JobSpy's job_url is the source listing. Keep it even when a direct employer
    # URL is available: the two links answer different questions for the user.
    source_url = _first_url(
        low.get("job_url"),
        low.get("source_url"),
        low.get("job_url_direct"),
        low.get("canonical_url"),
    )
    canonical_url = _first_url(low.get("job_url_direct"), low.get("canonical_url"))
    return Job(
        title=_clean_text(low.get("title")),
        company=_clean_text(low.get("company")),
        source_url=source_url or "",
        canonical_url=canonical_url,
        original_canonical_url=canonical_url,
        location=_clean_text(low.get("location")) or None,
        remote=_bool(low.get("is_remote") if "is_remote" in low else low.get("remote")),
        employment_type=(
            _clean_text(low.get("job_type")) or _clean_text(low.get("employment_type")) or None
        ),
        salary_min=_first_float(low.get("min_amount"), low.get("salary_min")),
        salary_max=_first_float(low.get("max_amount"), low.get("salary_max")),
        currency=_clean_text(low.get("currency")) or None,
        salary_source=_clean_text(low.get("salary_source")) or None,
        description=_plain_description(low.get("description")),
        posted_at=(
            _clean_text(low.get("date_posted")) or _clean_text(low.get("posted_at")) or None
        ),
        source=_clean_text(low.get("site")) or _clean_text(low.get("source")) or "import",
    )


def import_csv(path: Path) -> list[Job]:
    with path.open("r", encoding="utf-8-sig", newline="") as handle:
        return [row_to_job(row) for row in csv.DictReader(handle)]


def discover(config: dict[str, Any]) -> list[Job]:
    settings = config["search"]
    sites = [str(site).lower() for site in settings["sites"]]
    if "indeed" in sites:
        raise RuntimeError(
            "The Indeed source is temporarily disabled because JobSpy's adapter "
            "does not verify TLS certificates. Remove `indeed` from [search].sites."
        )
    try:
        from jobspy import scrape_jobs
    except ImportError as exc:
        raise RuntimeError(
            "JobSpy is not installed. Reinstall OpenJobScout or run `uv sync`."
        ) from exc

    collected: list[Job] = []
    failures: list[str] = []
    warnings: list[str] = []
    completed = 0
    for term in settings["terms"]:
        for site in sites:
            print(f"Searching {site}: {term}", file=sys.stderr, flush=True)
            try:
                frame = scrape_jobs(
                    site_name=[site],
                    search_term=term,
                    google_search_term=term,
                    location=settings["location"],
                    results_wanted=int(settings["results_per_term"]),
                    hours_old=int(settings["max_age_days"]) * 24,
                    country_indeed=settings.get("country_indeed", "USA"),
                    linkedin_fetch_description=True,
                    # Avoid JobSpy's HTML-to-Markdown dependency. OpenJobScout strips
                    # the returned HTML with the standard-library parser above.
                    description_format="html",
                    verbose=1,
                    enforce_annual_salary=True,
                )
                collected.extend(row_to_job(row) for row in frame.to_dict(orient="records"))
                completed += 1
            except Exception as exc:  # One source must not discard successful sources.
                failures.append(f"{site}/{term}: {type(exc).__name__}: {exc}")

    firecrawl_settings = settings_from_config(config)
    if firecrawl_settings.enabled:
        print("Searching Firecrawl corporate sources", file=sys.stderr, flush=True)
        try:
            batch = discover_firecrawl(config)
            collected.extend(batch.jobs)
            warnings.extend(f"firecrawl: {warning}" for warning in batch.warnings)
            if batch.successful:
                completed += 1
            elif batch.searches or batch.scrapes or batch.warnings:
                failures.append(
                    "firecrawl: no search or scrape completed successfully"
                )
            print(
                "Firecrawl: "
                f"searches={batch.searches}, scrapes={batch.scrapes}, "
                f"interactions={batch.interactions}, jobs={len(batch.jobs)}",
                file=sys.stderr,
            )
        except (RuntimeError, ValueError) as exc:
            failures.append(f"firecrawl: {type(exc).__name__}: {exc}")

    if failures or warnings:
        print("Some discovery sources reported warnings or failures:", file=sys.stderr)
        for warning in warnings:
            print(f"- {warning}", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
    if not completed and failures:
        raise RuntimeError(f"All {len(failures)} configured searches failed.")
    return collected


def deduplicate(jobs: Iterable[Job]) -> list[Job]:
    unique: dict[str, Job] = {}
    for job in jobs:
        if job.title and job.company and job.source_url:
            key = job_dedup_key(job)
            current = unique.get(key)
            if current is None or (
                bool(job.canonical_url),
                len(job.description),
            ) > (bool(current.canonical_url), len(current.description)):
                unique[key] = job
    return list(unique.values())
