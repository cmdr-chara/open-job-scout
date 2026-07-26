import tomllib
from importlib.resources import files

from open_job_scout.models import Job
from open_job_scout.ranking import filter_job, rank_job, required_years


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
