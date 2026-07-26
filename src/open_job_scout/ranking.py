from __future__ import annotations

import re
from datetime import date, datetime
from typing import Any

from .models import Job, normalize_text

EXPERIENCE_PATTERN = re.compile(
    r"\b(\d{1,2}(?:\.\d)?)\+?\s*(?:years?|ann[oi])"
    r"(?:\s+(?:of|di)\s+(?:professional(?:e|i)?\s+)?)?"
    r"(?:experience|esperienza)?",
    flags=re.IGNORECASE,
)
REQUIRED_SIGNALS = (
    "required",
    "requires",
    "requirement",
    "mandatory",
    "must have",
    "minimum",
    "at least",
    "richiest",
    "obbligatori",
    "necessari",
    "almeno",
)
PREFERENCE_SIGNALS = (
    "preferred",
    "nice to have",
    "a plus",
    "bonus",
    "desirable",
    "preferibil",
    "gradit",
)


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


def required_years(text: str) -> float | None:
    requirements: list[float] = []
    for clause in re.split(r";|\n|[•▪]|(?<!\d)\.(?!\d)", text):
        matches = EXPERIENCE_PATTERN.findall(clause)
        if not matches:
            continue
        lowered = normalize_text(clause)
        preferred = any(signal in lowered for signal in PREFERENCE_SIGNALS)
        explicit_requirement = any(signal in lowered for signal in REQUIRED_SIGNALS)
        if preferred and not explicit_requirement:
            continue
        requirements.extend(float(value) for value in matches)
    return max(requirements, default=None)


def contains_term(text: str, term: str) -> bool:
    normalized_text = normalize_text(text)
    normalized_term = normalize_text(term)
    if not normalized_term:
        return False
    pattern = re.escape(normalized_term).replace(r"\ ", r"\s+")
    return re.search(rf"(?<!\w){pattern}(?!\w)", normalized_text) is not None


def degree_required(text: str) -> bool:
    degree = r"(?:bachelor'?s?|master'?s?|university degree|degree|laurea)"
    required = (
        r"(?:required|mandatory|must have|requirement|"
        r"richiest[oaie]?|obbligatori[oaie]?|necessari[oaie]?)"
    )
    return bool(
        re.search(
            rf"(?:{degree}.{{0,80}}{required}|{required}.{{0,80}}{degree})",
            normalize_text(text),
        )
    )


def filter_job(job: Job, config: dict[str, Any]) -> tuple[bool, str | None]:
    filters = config["filters"]
    title = normalize_text(job.title)
    body = normalize_text(job.description)
    if filters.get("require_remote") and job.remote is not True:
        return False, "remote work not confirmed"
    if any(contains_term(title, term) for term in filters.get("blocked_title_terms", [])):
        return False, "blocked seniority or title"
    if any(contains_term(body, term) for term in filters.get("blocked_description_terms", [])):
        return False, "blocked condition in description"
    years = required_years(body)
    if years is not None and years > int(filters.get("max_required_years", 99)):
        return False, f"requires {years:g} years of experience"
    profile = config.get("profile", {})
    if (
        not profile.get("has_degree", True)
        and profile.get("degree_policy", "ignore") == "filter"
        and degree_required(body)
    ):
        return False, "degree required"
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
    skills = [value for value in ranking.get("preferred_skills", []) if contains_term(text, value)]
    title_hits = [
        value for value in ranking.get("preferred_title_terms", []) if contains_term(title, value)
    ]
    junior = [value for value in ranking.get("junior_signals", []) if contains_term(text, value)]
    concerns = [value for value in ranking.get("concern_signals", []) if contains_term(text, value)]

    score = len(skills) * 5 + len(title_hits) * 12 + len(junior) * 7
    if job.remote is True:
        score += 8
    if job.verification_status == "verified":
        score += 5
    score -= len(concerns) * 8

    profile = config.get("profile", {})
    if (
        not profile.get("has_degree", True)
        and profile.get("degree_policy", "ignore") == "penalize"
        and degree_required(text)
    ):
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
