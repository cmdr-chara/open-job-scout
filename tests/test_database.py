from pathlib import Path

from open_job_scout.database import find_job, list_jobs, mark_job, save_jobs
from open_job_scout.models import Job


def sample_job() -> Job:
    return Job(
        title="Junior Backend Engineer",
        company="Example",
        source_url="https://example.test/jobs/1",
        score=72,
    )


def test_status_survives_refresh(tmp_path: Path) -> None:
    database = tmp_path / "jobs.sqlite3"
    job = sample_job()
    save_jobs([job], database)
    mark_job(database, job.fingerprint[:10], "applied", "Applied on official site")
    job.score = 80
    save_jobs([job], database)

    stored = find_job(database, job.fingerprint[:10])
    assert stored["status"] == "applied"
    assert stored["notes"] == "Applied on official site"
    assert stored["score"] == 80


def test_list_can_filter_status(tmp_path: Path) -> None:
    database = tmp_path / "jobs.sqlite3"
    job = sample_job()
    save_jobs([job], database)
    mark_job(database, job.fingerprint[:10], "reviewed")
    assert len(list_jobs(database, status="reviewed")) == 1
    assert list_jobs(database, status="new") == []


def test_closed_verification_closes_unreviewed_job(tmp_path: Path) -> None:
    database = tmp_path / "jobs.sqlite3"
    job = sample_job()
    job.verification_status = "closed"
    save_jobs([job], database)
    assert find_job(database, job.fingerprint[:10])["status"] == "closed"


def test_closed_refresh_does_not_overwrite_applied_status(tmp_path: Path) -> None:
    database = tmp_path / "jobs.sqlite3"
    job = sample_job()
    save_jobs([job], database)
    mark_job(database, job.fingerprint[:10], "applied")
    job.verification_status = "closed"
    save_jobs([job], database)
    assert find_job(database, job.fingerprint[:10])["status"] == "applied"
