from __future__ import annotations

import hashlib
import re
from dataclasses import dataclass, field
from urllib.parse import parse_qsl, urlencode, urlsplit, urlunsplit


def normalize_text(value: str | None) -> str:
    return re.sub(r"\s+", " ", (value or "").strip().lower())


def job_fingerprint(company: str, title: str, source_url: str) -> str:
    """Return an identity that is stable when verification updates metadata."""
    identity = "|".join(
        (
            normalize_text(company),
            normalize_text(title),
            normalize_text(source_url),
        )
    )
    return hashlib.sha256(identity.encode("utf-8")).hexdigest()


_TRACKING_QUERY_KEYS = {
    "ref",
    "source",
    "trk",
    "trackingid",
    "utm_campaign",
    "utm_content",
    "utm_medium",
    "utm_source",
    "utm_term",
}


def normalize_job_url(value: str | None) -> str:
    """Normalize harmless URL differences without changing the posting identity."""
    if not value:
        return ""
    parts = urlsplit(value.strip())
    path = re.sub(r"/application/?$", "", parts.path.rstrip("/"), flags=re.IGNORECASE)
    query = urlencode(
        sorted(
            (key, item)
            for key, item in parse_qsl(parts.query, keep_blank_values=True)
            if key.lower() not in _TRACKING_QUERY_KEYS
        )
    )
    return urlunsplit((parts.scheme.lower(), parts.netloc.lower(), path, query, ""))


def job_dedup_key(job: Job) -> str:
    """Prefer a direct posting identity; otherwise cluster remote mirrors by role."""
    direct = normalize_job_url(job.canonical_url)
    if direct:
        return f"url:{direct}"
    base = f"{normalize_text(job.company)}|{normalize_text(job.title)}"
    if job.remote is True:
        return f"remote:{base}"
    return f"source:{normalize_job_url(job.source_url)}"


@dataclass(slots=True)
class Job:
    title: str
    company: str
    source_url: str
    location: str | None = None
    remote: bool | None = None
    employment_type: str | None = None
    salary_min: float | None = None
    salary_max: float | None = None
    currency: str | None = None
    salary_source: str | None = None
    description: str = ""
    posted_at: str | None = None
    source: str = "import"
    canonical_url: str | None = None
    original_canonical_url: str | None = None
    score: float = 0.0
    reasons: list[str] = field(default_factory=list)
    concerns: list[str] = field(default_factory=list)
    verification_status: str = "unverified"
    work_mode: str = "unknown"
    replacement_url: str | None = None
    replacement_title: str | None = None
    verification_source: str | None = None

    @property
    def fingerprint(self) -> str:
        return job_fingerprint(self.company, self.title, self.source_url)
