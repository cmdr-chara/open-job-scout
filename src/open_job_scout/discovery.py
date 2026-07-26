from __future__ import annotations

import csv
import sys
from collections.abc import Iterable
from pathlib import Path
from typing import Any

from .models import Job


def _float(value: object) -> float | None:
    try:
        if value in (None, ""):
            return None
        return float(value)
    except (TypeError, ValueError):
        return None


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
    url = str(
        low.get("job_url_direct")
        or low.get("canonical_url")
        or low.get("job_url")
        or low.get("source_url")
        or ""
    )
    return Job(
        title=str(low.get("title") or "").strip(),
        company=str(low.get("company") or "").strip(),
        source_url=url,
        canonical_url=str(low.get("job_url_direct") or "") or None,
        location=str(low.get("location") or "").strip() or None,
        remote=_bool(low.get("is_remote") if "is_remote" in low else low.get("remote")),
        employment_type=str(low.get("job_type") or low.get("employment_type") or "").strip()
        or None,
        salary_min=_float(low.get("min_amount") or low.get("salary_min")),
        salary_max=_float(low.get("max_amount") or low.get("salary_max")),
        currency=str(low.get("currency") or "").strip() or None,
        salary_source=str(low.get("salary_source") or "").strip() or None,
        description=str(low.get("description") or ""),
        posted_at=str(low.get("date_posted") or low.get("posted_at") or "").strip() or None,
        source=str(low.get("site") or low.get("source") or "import"),
    )


def import_csv(path: Path) -> list[Job]:
    with path.open("r", encoding="utf-8-sig", newline="") as handle:
        return [row_to_job(row) for row in csv.DictReader(handle)]


def discover(config: dict[str, Any]) -> list[Job]:
    try:
        from jobspy import scrape_jobs
    except ImportError as exc:
        raise RuntimeError(
            "JobSpy is not installed. Reinstall OpenJobScout or run `uv sync`."
        ) from exc

    settings = config["search"]
    collected: list[Job] = []
    failures: list[str] = []
    for term in settings["terms"]:
        print(f"Searching: {term}", file=sys.stderr, flush=True)
        try:
            frame = scrape_jobs(
                site_name=settings["sites"],
                search_term=term,
                google_search_term=term,
                location=settings["location"],
                results_wanted=int(settings["results_per_term"]),
                hours_old=int(settings["max_age_days"]) * 24,
                country_indeed=settings.get("country_indeed", "USA"),
                linkedin_fetch_description=True,
                description_format="markdown",
                verbose=1,
                enforce_annual_salary=True,
            )
            collected.extend(row_to_job(row) for row in frame.to_dict(orient="records"))
        except Exception as exc:  # A failed source must not discard completed searches.
            failures.append(f"{term}: {type(exc).__name__}: {exc}")
    if failures:
        print("Some searches failed:", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
    return collected


def deduplicate(jobs: Iterable[Job]) -> list[Job]:
    unique: dict[str, Job] = {}
    for job in jobs:
        if job.title and job.company and job.source_url:
            current = unique.get(job.fingerprint)
            if current is None or len(job.description) > len(current.description):
                unique[job.fingerprint] = job
    return list(unique.values())
