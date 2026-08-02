from __future__ import annotations

import shutil
import tomllib
from collections.abc import Mapping
from importlib.resources import files
from pathlib import Path
from typing import Any

APP_DIR = Path.home() / ".openjobscout"
DEFAULT_CONFIG = APP_DIR / "config.toml"


def expand_path(value: str) -> Path:
    return Path(value).expanduser().resolve()


def _section(config: Mapping[str, Any], name: str) -> Mapping[str, Any]:
    value = config.get(name)
    if not isinstance(value, Mapping):
        raise ValueError(f"Config section [{name}] is missing or invalid.")
    return value


def _string_list(
    section: Mapping[str, Any],
    section_name: str,
    key: str,
    *,
    allow_empty: bool = False,
    allow_blank: bool = False,
) -> list[str]:
    value = section.get(key)
    if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
        raise ValueError(f"Config value [{section_name}].{key} must be a list of strings.")
    if not allow_empty and not value:
        raise ValueError(f"Config value [{section_name}].{key} must not be empty.")
    if not allow_blank and any(not item.strip() for item in value):
        raise ValueError(f"Config value [{section_name}].{key} contains a blank item.")
    return value


def _integer(section: Mapping[str, Any], section_name: str, key: str, *, minimum: int) -> int:
    value = section.get(key)
    if isinstance(value, bool) or not isinstance(value, int) or value < minimum:
        raise ValueError(f"Config value [{section_name}].{key} must be an integer >= {minimum}.")
    return value


def validate_config(config: dict[str, Any]) -> dict[str, Any]:
    search = _section(config, "search")
    _string_list(search, "search", "terms")
    _string_list(search, "search", "sites")
    if not isinstance(search.get("location"), str):
        raise ValueError("Config value [search].location must be a string.")
    if not isinstance(search.get("country_indeed", "USA"), str):
        raise ValueError("Config value [search].country_indeed must be a string.")
    _integer(search, "search", "results_per_term", minimum=1)
    _integer(search, "search", "max_age_days", minimum=0)

    filters = _section(config, "filters")
    if not isinstance(filters.get("require_remote"), bool):
        raise ValueError("Config value [filters].require_remote must be true or false.")
    _string_list(
        filters,
        "filters",
        "allowed_employment_types",
        allow_empty=True,
        allow_blank=True,
    )
    _string_list(filters, "filters", "blocked_title_terms", allow_empty=True)
    _string_list(filters, "filters", "blocked_description_terms", allow_empty=True)
    _integer(filters, "filters", "max_required_years", minimum=0)

    ranking = _section(config, "ranking")
    for key in (
        "preferred_title_terms",
        "preferred_skills",
        "junior_signals",
        "concern_signals",
    ):
        _string_list(ranking, "ranking", key, allow_empty=True)

    salary = config.get("salary", {})
    if not isinstance(salary, Mapping):
        raise ValueError("Config section [salary] must be a table.")
    for key in (
        "minimum_annual",
        "preferred_annual",
        "unknown_penalty",
        "preferred_bonus",
    ):
        value = salary.get(key, 0)
        if isinstance(value, bool) or not isinstance(value, (int, float)) or value < 0:
            raise ValueError(f"Config value [salary].{key} must be a number >= 0.")
    unknown_policy = salary.get("unknown_policy", "allow")
    if unknown_policy not in {"allow", "filter"}:
        raise ValueError("Config value [salary].unknown_policy must be allow or filter.")
    if (
        salary.get("preferred_annual", 0)
        and salary.get("minimum_annual", 0) > salary["preferred_annual"]
    ):
        raise ValueError("Config value [salary].minimum_annual must not exceed preferred_annual.")

    storage = _section(config, "storage")
    for key in ("database", "report_directory"):
        value = storage.get(key)
        if not isinstance(value, str) or not value.strip():
            raise ValueError(f"Config value [storage].{key} must be a non-empty path string.")

    if "stale_after_days" in storage:
        _integer(storage, "storage", "stale_after_days", minimum=1)
    profile = config.get("profile", {})
    if not isinstance(profile, Mapping):
        raise ValueError("Config section [profile] must be a table.")
    if "has_degree" in profile and not isinstance(profile["has_degree"], bool):
        raise ValueError("Config value [profile].has_degree must be true or false.")
    policy = profile.get("degree_policy", "ignore")
    if not isinstance(policy, str) or policy not in {"ignore", "penalize", "filter"}:
        raise ValueError(
            "Config value [profile].degree_policy must be ignore, penalize, or filter."
        )
    penalty = profile.get("degree_penalty", 15)
    if isinstance(penalty, bool) or not isinstance(penalty, (int, float)) or penalty < 0:
        raise ValueError("Config value [profile].degree_penalty must be a number >= 0.")
    return config


def load_config(path: Path | None = None) -> dict[str, Any]:
    selected = (path or DEFAULT_CONFIG).expanduser().resolve()
    if not selected.exists():
        raise FileNotFoundError(
            f"Config not found: {selected}. Run `jobscout init` or pass --config."
        )
    with selected.open("rb") as handle:
        return validate_config(tomllib.load(handle))


def initialize_config(destination: Path | None = None, *, force: bool = False) -> Path:
    target = (destination or DEFAULT_CONFIG).expanduser().resolve()
    if target.exists() and not force:
        raise FileExistsError(f"Config already exists: {target}")
    target.parent.mkdir(parents=True, exist_ok=True)
    resource = files("open_job_scout").joinpath("default_config.toml")
    with resource.open("rb") as source, target.open("wb") as output:
        shutil.copyfileobj(source, output)
    return target
