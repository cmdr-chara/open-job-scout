from __future__ import annotations

import http.client
import ipaddress
import json
import math
import os
import re
import urllib.error
import urllib.parse
import urllib.request
from collections import deque
from collections.abc import Mapping
from dataclasses import dataclass, field
from typing import Any, Protocol

from .models import Job

API_ROOT = "https://api.firecrawl.dev/v2"
USER_AGENT = "OpenJobScout/0.2 (+https://github.com/cmdr-chara/open-job-scout)"
MAX_RESPONSE_BYTES = 10_000_000
MAX_DESCRIPTION_CHARS = 100_000
MAX_SCRAPE_ID_CHARS = 128
DEFAULT_EXCLUDE_DOMAINS = (
    "linkedin.com",
    "indeed.com",
    "glassdoor.com",
    "ziprecruiter.com",
    "greenhouse.io",
    "lever.co",
    "ashbyhq.com",
    "recruitee.com",
)

_JOB_SCHEMA: dict[str, Any] = {
    "type": "object",
    "properties": {
        "page_type": {"type": "string", "enum": ["job", "careers", "other"]},
        "requires_interaction": {"type": "boolean"},
        "job": {
            "type": ["object", "null"],
            "properties": {
                "title": {"type": ["string", "null"]},
                "company": {"type": ["string", "null"]},
                "location": {"type": ["string", "null"]},
                "remote": {"type": ["boolean", "null"]},
                "work_mode": {
                    "type": ["string", "null"],
                    "enum": ["remote", "hybrid", "onsite", "unknown", None],
                },
                "employment_type": {"type": ["string", "null"]},
                "salary_min": {
                    "type": ["number", "null"],
                    "description": "Employer-published annual minimum only; null otherwise.",
                },
                "salary_max": {
                    "type": ["number", "null"],
                    "description": "Employer-published annual maximum only; null otherwise.",
                },
                "currency": {"type": ["string", "null"]},
                "posted_at": {"type": ["string", "null"]},
                "canonical_url": {"type": ["string", "null"]},
                "description": {"type": ["string", "null"]},
            },
        },
        "job_links": {
            "type": "array",
            "maxItems": 50,
            "items": {
                "type": "object",
                "properties": {
                    "title": {"type": ["string", "null"]},
                    "url": {"type": "string"},
                },
                "required": ["url"],
            },
        },
    },
    "required": ["page_type", "requires_interaction", "job_links"],
}

_EXTRACTION_PROMPT = """
Classify this public page as a single job posting, a careers/jobs index, or other.
Use only facts visible on the page. Never infer missing company, salary, location,
remote status, employment type, dates, or URLs. Salary fields are annual compensation
only: include them only when the employer explicitly publishes annual values on the
page; never estimate or annualize hourly, monthly, daily, or otherwise ambiguous
compensation. For a single currently open job, return the normalized job fields and
preserve the employer's description text without navigation/cookie/footer boilerplate.
For a careers index, return public job-posting links visible on the page. Set
requires_interaction only when ordinary public navigation or a load-more control is
needed to reveal listings. Do not log in, fill an application, solve or bypass a
CAPTCHA, or bypass any access control.
""".strip()

_INTERACT_PROMPT = """
This is an explicitly allowed public careers page. Reveal job links only through normal
public navigation or load-more controls. Do not log in, enter personal data, fill an
application, solve or bypass a CAPTCHA, or bypass any access control. Return only JSON
with this shape: {"job_links":[{"title":"optional title","url":"https://..."}]}.
If public job links cannot be revealed without a challenge or authentication, return
{"job_links":[]}.
""".strip()


@dataclass(frozen=True, slots=True)
class FirecrawlSettings:
    enabled: bool = False
    search_enabled: bool = True
    search_limit_per_term: int = 8
    max_scrapes: int = 16
    career_urls: tuple[str, ...] = ()
    interact_urls: tuple[str, ...] = ()
    include_domains: tuple[str, ...] = ()
    exclude_domains: tuple[str, ...] = DEFAULT_EXCLUDE_DOMAINS
    timeout_seconds: int = 45
    zero_data_retention: bool = True


@dataclass(slots=True)
class FirecrawlBatch:
    jobs: list[Job] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)
    searches: int = 0
    scrapes: int = 0
    interactions: int = 0
    enabled: bool = False
    # A source is complete only after an empty search or a successful scrape.
    # Counters alone cannot distinguish a successful search followed by failed
    # scrapes from a search that legitimately found no public targets.
    successful: bool = False


class FirecrawlTransport(Protocol):
    def search(self, query: str, settings: FirecrawlSettings) -> list[dict[str, Any]]: ...

    def scrape(self, url: str, settings: FirecrawlSettings) -> dict[str, Any]: ...

    def interact(self, scrape_id: str, settings: FirecrawlSettings) -> list[dict[str, Any]]: ...

    def stop_interaction(self, scrape_id: str) -> None: ...


class FirecrawlClient:
    def __init__(self, api_key: str, timeout_seconds: int) -> None:
        if not api_key.strip():
            raise ValueError("Firecrawl API key must not be blank")
        self._api_key = api_key.strip()
        self._timeout = timeout_seconds

    def _request(
        self,
        method: str,
        path: str,
        payload: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        body = None if payload is None else json.dumps(payload).encode("utf-8")
        request = urllib.request.Request(
            f"{API_ROOT}{path}",
            data=body,
            method=method,
            headers={
                "Authorization": f"Bearer {self._api_key}",
                "Content-Type": "application/json",
                "User-Agent": USER_AGENT,
            },
        )
        try:
            with urllib.request.urlopen(request, timeout=self._timeout) as response:
                raw = response.read(MAX_RESPONSE_BYTES + 1)
                if len(raw) > MAX_RESPONSE_BYTES:
                    raise RuntimeError("Firecrawl response exceeded the allowed size")
        except urllib.error.HTTPError as exc:
            raise RuntimeError(f"Firecrawl returned HTTP {exc.code}") from exc
        except (urllib.error.URLError, http.client.HTTPException, OSError) as exc:
            raise RuntimeError(f"Firecrawl request failed: {type(exc).__name__}") from exc
        try:
            decoded = json.loads(raw.decode("utf-8"))
        except (UnicodeError, json.JSONDecodeError) as exc:
            raise RuntimeError("Firecrawl returned invalid JSON") from exc
        if not isinstance(decoded, dict):
            raise RuntimeError("Firecrawl returned a non-object response")
        if decoded.get("success") is False:
            message = str(decoded.get("error") or "request failed")
            raise RuntimeError(f"Firecrawl request failed: {message[:200]}")
        return decoded

    def search(self, query: str, settings: FirecrawlSettings) -> list[dict[str, Any]]:
        payload: dict[str, Any] = {
            "query": query,
            "limit": settings.search_limit_per_term,
            "sources": ["web"],
            "safe": True,
            "timeout": settings.timeout_seconds * 1000,
            "ignoreInvalidURLs": True,
        }
        if settings.include_domains:
            payload["includeDomains"] = list(settings.include_domains)
        elif settings.exclude_domains:
            payload["excludeDomains"] = list(settings.exclude_domains)
        response = self._request("POST", "/search", payload)
        data = response.get("data")
        web = data.get("web") if isinstance(data, Mapping) else None
        return [item for item in web or [] if isinstance(item, dict)]

    def scrape(self, url: str, settings: FirecrawlSettings) -> dict[str, Any]:
        response = self._request(
            "POST",
            "/scrape",
            {
                "url": url,
                "formats": [
                    {
                        "type": "json",
                        "prompt": _EXTRACTION_PROMPT,
                        "schema": _JOB_SCHEMA,
                    }
                ],
                "onlyMainContent": True,
                "removeBase64Images": True,
                "blockAds": True,
                "zeroDataRetention": settings.zero_data_retention,
                "timeout": settings.timeout_seconds * 1000,
            },
        )
        data = response.get("data")
        return data if isinstance(data, dict) else {}

    def interact(self, scrape_id: str, settings: FirecrawlSettings) -> list[dict[str, Any]]:
        response = self._request(
            "POST",
            f"/scrape/{urllib.parse.quote(scrape_id, safe='')}/interact",
            {"prompt": _INTERACT_PROMPT, "timeout": settings.timeout_seconds},
        )
        output = response.get("output")
        if not isinstance(output, str):
            return []
        parsed = _parse_json_text(output)
        links = parsed.get("job_links") if isinstance(parsed, dict) else None
        return [item for item in links or [] if isinstance(item, dict)]

    def stop_interaction(self, scrape_id: str) -> None:
        try:
            self._request(
                "DELETE",
                f"/scrape/{urllib.parse.quote(scrape_id, safe='')}/interact",
            )
        except RuntimeError:
            return


def settings_from_config(config: Mapping[str, Any]) -> FirecrawlSettings:
    raw = config.get("firecrawl", {})
    if raw is None:
        raw = {}
    if not isinstance(raw, Mapping):
        raise ValueError("Config section [firecrawl] must be a table.")

    def boolean(key: str, default: bool) -> bool:
        value = raw.get(key, default)
        if not isinstance(value, bool):
            raise ValueError(f"Config value [firecrawl].{key} must be true or false.")
        return value

    def integer(key: str, default: int, minimum: int, maximum: int) -> int:
        value = raw.get(key, default)
        if isinstance(value, bool) or not isinstance(value, int) or not minimum <= value <= maximum:
            raise ValueError(
                f"Config value [firecrawl].{key} must be an integer between "
                f"{minimum} and {maximum}."
            )
        return value

    def strings(key: str, default: tuple[str, ...] = ()) -> tuple[str, ...]:
        value = raw.get(key, list(default))
        if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
            raise ValueError(f"Config value [firecrawl].{key} must be a list of strings.")
        cleaned = tuple(item.strip() for item in value if item.strip())
        if len(cleaned) != len(value):
            raise ValueError(f"Config value [firecrawl].{key} contains a blank item.")
        return cleaned

    career_urls = strings("career_urls")
    interact_urls = strings("interact_urls")
    include_domains = strings("include_domains")
    exclude_domains = strings("exclude_domains", DEFAULT_EXCLUDE_DOMAINS)
    for key, values in (("career_urls", career_urls), ("interact_urls", interact_urls)):
        for value in values:
            if not _public_http_url(value):
                raise ValueError(
                    f"Config value [firecrawl].{key} contains an unsafe or invalid "
                    "public HTTP(S) URL."
                )
    for key, values in (("include_domains", include_domains), ("exclude_domains", exclude_domains)):
        for value in values:
            if not _valid_domain(value):
                raise ValueError(
                    f"Config value [firecrawl].{key} contains invalid hostname {value!r}."
                )
    if include_domains and exclude_domains and exclude_domains != DEFAULT_EXCLUDE_DOMAINS:
        raise ValueError(
            "Config values [firecrawl].include_domains and custom exclude_domains "
            "are mutually exclusive."
        )

    return FirecrawlSettings(
        enabled=boolean("enabled", False),
        search_enabled=boolean("search_enabled", True),
        search_limit_per_term=integer("search_limit_per_term", 8, 1, 50),
        max_scrapes=integer("max_scrapes", 16, 1, 100),
        career_urls=career_urls,
        interact_urls=interact_urls,
        include_domains=include_domains,
        exclude_domains=exclude_domains,
        timeout_seconds=integer("timeout_seconds", 45, 5, 300),
        zero_data_retention=boolean("zero_data_retention", True),
    )


def discover_firecrawl(
    config: Mapping[str, Any],
    *,
    api_key: str | None = None,
    client: FirecrawlTransport | None = None,
) -> FirecrawlBatch:
    settings = settings_from_config(config)
    batch = FirecrawlBatch(enabled=settings.enabled)
    if not settings.enabled:
        return batch

    if client is None:
        selected_key = api_key or os.environ.get("FIRECRAWL_API_KEY", "")
        if not selected_key.strip():
            raise RuntimeError(
                "Firecrawl is enabled but FIRECRAWL_API_KEY is not set. "
                "Disable [firecrawl].enabled or provide the environment variable."
            )
        client = FirecrawlClient(selected_key, settings.timeout_seconds)

    search = config.get("search")
    if not isinstance(search, Mapping):
        raise ValueError("Config section [search] is missing or invalid.")
    terms = search.get("terms", [])
    if not isinstance(terms, list) or not all(isinstance(term, str) for term in terms):
        raise ValueError("Config value [search].terms must be a list of strings.")
    location = str(search.get("location") or "").strip()

    queue: deque[str] = deque()
    queued: set[str] = set()
    scraped: set[str] = set()
    interact_keys = {_url_key(url) for url in settings.interact_urls}

    def enqueue(url: object) -> None:
        if not isinstance(url, str) or not _public_http_url(url):
            return
        key = _url_key(url)
        if key in queued or key in scraped:
            return
        queued.add(key)
        queue.append(url.strip())

    for url in settings.career_urls:
        enqueue(url)
    for url in settings.interact_urls:
        enqueue(url)

    if settings.search_enabled:
        for term in terms:
            query = " ".join(part for part in (term.strip(), location, "jobs careers") if part)
            if not query:
                continue
            try:
                results = client.search(query, settings)
                batch.searches += 1
            except RuntimeError as exc:
                batch.warnings.append(f"search {term!r}: {exc}")
                continue
            valid_targets = 0
            for result in results:
                result_url = result.get("url")
                if isinstance(result_url, str) and _public_http_url(result_url):
                    valid_targets += 1
                    enqueue(result_url)
            if valid_targets == 0:
                batch.successful = True

    scrape_attempts = 0
    while queue and scrape_attempts < settings.max_scrapes:
        url = queue.popleft()
        key = _url_key(url)
        queued.discard(key)
        if key in scraped:
            continue
        scraped.add(key)
        scrape_attempts += 1
        try:
            data = client.scrape(url, settings)
            batch.scrapes += 1
            batch.successful = True
        except RuntimeError as exc:
            batch.warnings.append(f"scrape {url}: {exc}")
            continue

        extracted = data.get("json")
        if not isinstance(extracted, Mapping):
            batch.warnings.append(f"scrape {url}: no structured job data returned")
            continue

        job = _job_from_extracted(extracted.get("job"), url)
        if extracted.get("page_type") == "job" and job is not None:
            batch.jobs.append(job)

        links = extracted.get("job_links")
        if isinstance(links, list):
            for link in links:
                if isinstance(link, Mapping):
                    enqueue(link.get("url"))

        if extracted.get("requires_interaction") is not True:
            continue
        if key not in interact_keys:
            batch.warnings.append(
                f"interaction required for {url}; add the exact URL to "
                "[firecrawl].interact_urls to opt in"
            )
            continue
        scrape_id = _scrape_id(data)
        if not scrape_id:
            batch.warnings.append(
                f"interaction requested for {url}, but no valid scrape ID was returned"
            )
            continue
        try:
            links = client.interact(scrape_id, settings)
            batch.interactions += 1
            for link in links:
                enqueue(link.get("url"))
        except RuntimeError as exc:
            batch.warnings.append(f"interact {url}: {exc}")
        finally:
            client.stop_interaction(scrape_id)

    return batch


def _job_from_extracted(value: object, source_url: str) -> Job | None:
    if not isinstance(value, Mapping):
        return None
    title = _text(value.get("title"))
    company = _text(value.get("company"))
    if not title or not company or not _public_http_url(source_url):
        return None
    canonical = _text(value.get("canonical_url"))
    if not _public_http_url(canonical):
        canonical = source_url
    work_mode = _work_mode(value.get("work_mode"))
    remote_value = value.get("remote")
    remote = remote_value if isinstance(remote_value, bool) else None
    if remote is None and work_mode == "remote":
        remote = True
    elif remote is None and work_mode == "onsite":
        remote = False
    salary_min = _number(value.get("salary_min"))
    salary_max = _number(value.get("salary_max"))
    description = _text(value.get("description"), collapse=False)[:MAX_DESCRIPTION_CHARS]
    return Job(
        title=title,
        company=company,
        source_url=source_url,
        canonical_url=canonical,
        original_canonical_url=canonical,
        location=_text(value.get("location")) or None,
        remote=remote,
        work_mode=work_mode,
        employment_type=_text(value.get("employment_type")) or None,
        salary_min=salary_min,
        salary_max=salary_max,
        currency=_text(value.get("currency")) or None,
        salary_source="firecrawl" if salary_min is not None or salary_max is not None else None,
        description=description,
        posted_at=_text(value.get("posted_at")) or None,
        source="firecrawl",
    )


def _work_mode(value: object) -> str:
    mode = _text(value).lower().replace("-", "")
    return mode if mode in {"remote", "hybrid", "onsite"} else "unknown"


def _number(value: object) -> float | None:
    try:
        number = float(value)
    except (TypeError, ValueError):
        return None
    return number if math.isfinite(number) and number >= 0 else None


def _text(value: object, *, collapse: bool = True) -> str:
    if not isinstance(value, str):
        return ""
    text = value.strip()
    return re.sub(r"\s+", " ", text) if collapse else text


def _public_http_url(value: object) -> bool:
    if not isinstance(value, str):
        return False
    try:
        parts = urllib.parse.urlsplit(value.strip())
    except ValueError:
        return False
    if parts.scheme.lower() not in {"http", "https"} or not parts.hostname:
        return False
    if parts.username or parts.password:
        return False
    host = parts.hostname.rstrip(".").lower()
    if host == "localhost" or host.endswith((".local", ".localhost", ".internal")):
        return False
    try:
        address = ipaddress.ip_address(host)
    except ValueError:
        # Python's ipaddress module intentionally accepts only canonical
        # textual addresses. URL clients and resolvers may still interpret
        # legacy numeric IPv4 forms such as 127.1, 2130706433, or
        # 0x7f000001 as localhost. Reject those only when they did not parse
        # as a canonical IP, while preserving public literal IPv4 addresses.
        return not _looks_like_numeric_host(host)
    return address.is_global


def _looks_like_numeric_host(host: str) -> bool:
    return bool(host) and host[0] in "0123456789" and all(
        character.isdigit() or character in ".xabcdef" for character in host
    )


def _valid_domain(value: str) -> bool:
    value = value.strip()
    if (
        not value
        or any(character.isspace() for character in value)
        or "//" in value
        or "/" in value
        or ":" in value
    ):
        return False
    return _public_http_url(f"https://{value}/")


def _url_key(value: str) -> str:
    try:
        parts = urllib.parse.urlsplit(value.strip())
    except ValueError:
        return value.strip()
    return urllib.parse.urlunsplit(
        (parts.scheme.lower(), parts.netloc.lower(), parts.path.rstrip("/"), parts.query, "")
    )


def _scrape_id(data: Mapping[str, Any]) -> str | None:
    metadata = data.get("metadata")
    if not isinstance(metadata, Mapping):
        return None
    value = metadata.get("scrapeId") or metadata.get("scrape_id")
    if not isinstance(value, str) or not value or len(value) > MAX_SCRAPE_ID_CHARS:
        return None
    return value if re.fullmatch(r"[A-Za-z0-9_-]+", value) else None


def _parse_json_text(text: str) -> object:
    cleaned = text.strip()
    if cleaned.startswith("```"):
        cleaned = re.sub(r"^```(?:json)?\s*", "", cleaned, flags=re.IGNORECASE)
        cleaned = re.sub(r"\s*```$", "", cleaned)
    try:
        return json.loads(cleaned)
    except json.JSONDecodeError:
        return {}
