import hashlib
import sqlite3
from pathlib import Path

from open_job_scout.database import (
    SCHEMA,
    connect,
    find_job,
    get_jobs_by_fingerprints,
    list_jobs,
    mark_job,
    save_jobs,
)
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


def test_fingerprint_does_not_change_when_canonical_url_changes() -> None:
    job = sample_job()
    initial = job.fingerprint
    job.canonical_url = "https://careers.example/jobs/1"
    assert job.fingerprint == initial


def test_fetch_current_run_by_fingerprint(tmp_path: Path) -> None:
    database = tmp_path / "jobs.sqlite3"
    first = sample_job()
    second = Job(
        title="Platform Engineer",
        company="Another",
        source_url="https://example.test/jobs/2",
        score=20,
    )
    save_jobs([first, second], database)

    rows = get_jobs_by_fingerprints(database, [second.fingerprint])
    assert [row["title"] for row in rows] == ["Platform Engineer"]
    assert get_jobs_by_fingerprints(database, []) == []


def test_legacy_canonical_fingerprint_is_migrated(tmp_path: Path) -> None:
    database = tmp_path / "jobs.sqlite3"
    job = sample_job()
    canonical = "https://careers.example/jobs/1"
    legacy_identity = "|".join((job.company.lower(), job.title.lower(), canonical.lower()))
    legacy = hashlib.sha256(legacy_identity.encode()).hexdigest()
    now = "2026-07-26T12:00:00+00:00"

    connection = sqlite3.connect(database)
    connection.executescript(SCHEMA)
    connection.execute(
        """
        INSERT INTO jobs (
            fingerprint, title, company, source_url, canonical_url, score,
            first_seen_at, last_seen_at, status, notes
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            legacy,
            job.title,
            job.company,
            job.source_url,
            canonical,
            72,
            now,
            now,
            "applied",
            "Keep this note",
        ),
    )
    connection.commit()
    connection.close()

    migrated = connect(database)
    migrated.close()
    stored = find_job(database, job.fingerprint[:10])
    assert stored["status"] == "applied"
    assert stored["notes"] == "Keep this note"


def test_v01_direct_url_row_merges_on_first_refresh(tmp_path: Path) -> None:
    database = tmp_path / "jobs.sqlite3"
    direct = "https://careers.example/jobs/1"
    legacy = Job(
        title="Junior Backend Engineer",
        company="Example",
        source_url=direct,
        canonical_url=direct,
    )
    save_jobs([legacy], database)
    mark_job(database, legacy.fingerprint[:10], "applied", "Preserve this application")

    refreshed = Job(
        title=legacy.title,
        company=legacy.company,
        source_url="https://board.example/jobs/abc",
        canonical_url=direct,
        original_canonical_url=direct,
        score=88,
    )
    save_jobs([refreshed], database)

    rows = list_jobs(database)
    assert len(rows) == 1
    assert rows[0]["fingerprint"] == refreshed.fingerprint
    assert rows[0]["source_url"] == refreshed.source_url
    assert rows[0]["status"] == "applied"
    assert rows[0]["notes"] == "Preserve this application"


def test_same_role_with_different_direct_urls_stays_separate(tmp_path: Path) -> None:
    database = tmp_path / "jobs.sqlite3"
    first = Job(
        title="Backend Engineer",
        company="Example",
        source_url="https://board.example/jobs/1",
        canonical_url="https://careers.example/jobs/1",
    )
    second = Job(
        title=first.title,
        company=first.company,
        source_url="https://board.example/jobs/2",
        canonical_url="https://careers.example/jobs/2",
    )

    save_jobs([first, second], database)

    assert len(list_jobs(database)) == 2
