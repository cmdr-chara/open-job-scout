from __future__ import annotations

import re
from datetime import date, datetime
from typing import Any

from .models import Job, normalize_text


def age_days(value: str | None) -> int | None:
    if not value:
        return None
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00")).date()
    except ValueError:
        try:
            parsed = date.fromisoformat(value[:10])
        except ValueError:
            return None
    return (date.today() - parsed).days


def required_years(text: str) -> int | None:
    matches = re.findall(
        r"\b(\d{1,2})\+?\s*(?:years?|anni)(?:\s+of\s+(?:professional\s+)?)?(?:experience|esperienza)?",
        text,
        flags=re.IGNORECASE,
    )
    return max((int(value) for value in matches), default=None)


def filter_job(job: Job, config: dict[str, Any]) -> tuple[bool, str | None]:
    filters = config["filters"]
    title = normalize_text(job.title)
    body = normalize_text(job.description)
    if filters.get("require_remote") and job.remote is not True:
        return False, "remote work not confirmed"
    if any(term.lower() in title for term in filters.get("blocked_title_terms", [])):
        return False, "blocked seniority or title"
    if any(term.lower() in body for term in filters.get("blocked_description_terms", [])):
        return False, "blocked condition in description"
    years = required_years(body)
    if years is not None and years > int(filters.get("max_required_years", 99)):
        return False, f"requires {years} years of experience"
    employment = normalize_text(job.employment_type)
    allowed = {normalize_text(value) for value in filters.get("allowed_employment_types", [])}
    if allowed and employment not in allowed:
        return False, f"employment type not allowed: {employment}"
    days = age_days(job.posted_at)
    if days is not None and days > int(config["search"].get("max_age_days", 30)):
        return False, f"listing is {days} days old"
    return True, None


def rank_job(job: Job, config: dict[str, Any]) -> Job:
    ranking = config["ranking"]
    text = normalize_text(f"{job.title} {job.description}")
    title = normalize_text(job.title)
    skills = [value for value in ranking.get("preferred_skills", []) if value.lower() in text]
    title_hits = [
        value for value in ranking.get("preferred_title_terms", []) if value.lower() in title
    ]
    junior = [value for value in ranking.get("junior_signals", []) if value.lower() in text]
    concerns = [value for value in ranking.get("concern_signals", []) if value.lower() in text]

    score = len(skills) * 5 + len(title_hits) * 12 + len(junior) * 7
    if job.remote is True:
        score += 8
    if job.verification_status == "verified":
        score += 5
    score -= len(concerns) * 8

    profile = config.get("profile", {})
    degree_required = bool(
        re.search(
            r"(?:degree|laurea).{0,80}(?:required|mandatory|obbligatori|richiest)",
            text,
        )
    )
    if not profile.get("has_degree", True) and degree_required:
        concerns.append("degree required")
        score -= float(profile.get("degree_penalty", 15))

    job.score = max(0.0, min(100.0, round(score, 1)))
    job.reasons = []
    if title_hits:
        job.reasons.append(f"title: {', '.join(title_hits)}")
    if skills:
        job.reasons.append(f"skills: {', '.join(skills)}")
    if junior:
        job.reasons.append(f"early-career signals: {', '.join(junior)}")
    if job.remote is True:
        job.reasons.append("remote declared")
    job.concerns = concerns
    return job
