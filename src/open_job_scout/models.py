from __future__ import annotations

import hashlib
import re
from dataclasses import dataclass, field


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
    verification_source: str | None = None

    @property
    def fingerprint(self) -> str:
        return job_fingerprint(self.company, self.title, self.source_url)
