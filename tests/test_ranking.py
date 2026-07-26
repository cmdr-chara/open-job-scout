import tomllib
from importlib.resources import files

from open_job_scout.models import Job
from open_job_scout.ranking import contains_term, filter_job, rank_job, required_years


def config() -> dict:
    resource = files("open_job_scout").joinpath("default_config.toml")
    with resource.open("rb") as handle:
        return tomllib.load(handle)


def test_ranking_is_explainable() -> None:
    job = Job(
        title="Junior Python Backend Engineer",
        company="Example",
        source_url="https://example.test/job",
        remote=True,
        description="Build APIs with Python, FastAPI, PostgreSQL and Docker.",
    )
    ranked = rank_job(job, config())
    assert ranked.score > 40
    assert any(reason.startswith("skills:") for reason in ranked.reasons)


def test_excessive_experience_is_filtered() -> None:
    job = Job(
        title="Backend Engineer",
        company="Example",
        source_url="https://example.test/job",
        description="Requires 5+ years of experience.",
    )
    accepted, reason = filter_job(job, config())
    assert accepted is False
    assert reason == "requires 5 years of experience"
    assert required_years(job.description) == 5


def test_preferred_experience_is_not_treated_as_required() -> None:
    text = "Five years is preferred; 2 years of experience are required."
    assert required_years(text) == 2


def test_italian_experience_requirement_is_detected() -> None:
    assert required_years("Sono richiesti 4 anni di esperienza professionale.") == 4


def test_decimal_experience_does_not_become_five_years() -> None:
    assert required_years("At least 3.5 years of experience.") == 3.5


def test_keyword_matching_uses_word_boundaries() -> None:
    assert contains_term("Build services in Go.", "go") is True
    assert contains_term("Build a Django service.", "go") is False


def test_degree_policy_can_filter_or_ignore() -> None:
    settings = config()
    settings["profile"]["has_degree"] = False
    job = Job(
        title="Backend Engineer",
        company="Example",
        source_url="https://example.test/job",
        description="A bachelor's degree is required.",
    )

    settings["profile"]["degree_policy"] = "filter"
    assert filter_job(job, settings) == (False, "degree required")

    settings["profile"]["degree_policy"] = "ignore"
    assert filter_job(job, settings) == (True, None)
