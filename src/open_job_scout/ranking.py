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

HYBRID_SIGNALS = (
    "hybrid",
    "ibrido",
    "partly remote",
    "days per week in the office",
    "giorni a settimana in ufficio",
)
ONSITE_SIGNALS = (
    "on-site",
    "onsite",
    "office-based",
    "in office",
    "in sede",
    "in ufficio",
    "not remote",
    "no remote",
    "non remoto",
    "no smart working",
)
REMOTE_SIGNALS = (
    "fully remote",
    "full remote",
    "remote-first",
    "remote within",
    "remote from",
    "work from home",
    "da remoto",
)


def classify_work_mode(job: Job) -> str:
    """Classify conservatively: explicit hybrid/on-site language beats scraper flags."""
    text = normalize_text(f"{job.title} {job.location or ''} {job.description}")
    if any(signal in text for signal in HYBRID_SIGNALS):
        return "hybrid"
    if any(signal in text for signal in ONSITE_SIGNALS):
        return "onsite"
    if job.work_mode in {"remote", "hybrid", "onsite"}:
        return job.work_mode
    if job.remote is True or any(signal in text for signal in REMOTE_SIGNALS):
        return "remote"
    if job.remote is False:
        return "onsite"
    return "unknown"


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
    job.work_mode = classify_work_mode(job)
    if filters.get("require_remote") and job.work_mode != "remote":
        return False, f"fully remote work not confirmed ({job.work_mode})"
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

    salary = config.get("salary", {})
    minimum = float(salary.get("minimum_annual", 0))
    known_high = job.salary_max if job.salary_max is not None else job.salary_min
    if known_high is not None and known_high < minimum:
        return False, f"published salary below {minimum:g}"
    if known_high is None and salary.get("unknown_policy", "allow") == "filter":
        return False, "salary not published"
    return True, None


def rank_job(job: Job, config: dict[str, Any]) -> Job:
    ranking = config["ranking"]
    text = normalize_text(f"{job.title} {job.description}")
    title = normalize_text(job.title)
    job.work_mode = classify_work_mode(job)
    skills = [value for value in ranking.get("preferred_skills", []) if contains_term(text, value)]
    title_hits = [
        value for value in ranking.get("preferred_title_terms", []) if contains_term(title, value)
    ]
    junior = [value for value in ranking.get("junior_signals", []) if contains_term(text, value)]
    concerns = [value for value in ranking.get("concern_signals", []) if contains_term(text, value)]

    score = len(skills) * 5 + len(title_hits) * 12 + len(junior) * 7
    if job.work_mode == "remote":
        score += 8
    if job.verification_status == "verified":
        score += 5
    score -= len(concerns) * 8
    if job.verification_status == "closed":
        concerns.append("listing closed")
        score -= 100
    elif job.verification_status == "unreachable":
        concerns.append("listing could not be verified")
        score -= 15

    salary = config.get("salary", {})
    known_salary = job.salary_max if job.salary_max is not None else job.salary_min
    preferred_salary = float(salary.get("preferred_annual", 0))
    if known_salary is not None and preferred_salary > 0 and known_salary >= preferred_salary:
        score += float(salary.get("preferred_bonus", 10))
    elif known_salary is None:
        score -= float(salary.get("unknown_penalty", 0))

    if job.work_mode == "hybrid":
        concerns.append("hybrid work")
    elif job.work_mode == "onsite":
        concerns.append("on-site work")
    elif job.work_mode == "unknown":
        concerns.append("work mode unconfirmed")

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
    if job.work_mode == "remote":
        job.reasons.append("fully remote")
    if known_salary is not None:
        currency = job.currency or ""
        job.reasons.append(f"published salary: {known_salary:g} {currency}".strip())
    elif salary.get("unknown_penalty", 0):
        concerns.append("salary not published")
    job.concerns = concerns
    return job
