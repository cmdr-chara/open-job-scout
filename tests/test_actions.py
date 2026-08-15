from pathlib import Path

from open_job_scout.actions import add_note
from open_job_scout.database import find_job, list_job_events, save_jobs
from open_job_scout.models import Job


def test_add_note_preserves_status_ownership_and_timestamp(tmp_path: Path) -> None:
    database = tmp_path / "jobs.sqlite3"
    job = Job(
        title="Backend Engineer",
        company="Example",
        source_url="https://example.test/jobs/1",
        score=75,
    )
    save_jobs([job], database)
    before = find_job(database, job.fingerprint[:10])

    assert add_note(database, job.fingerprint[:10], "Follow up Friday") is True

    after = find_job(database, job.fingerprint[:10])
    assert after["status"] == before["status"]
    assert after["status_updated_at"] == before["status_updated_at"]
    assert after["status_manually_set"] == before["status_manually_set"]
    assert after["notes"] == "Follow up Friday"
    events = list_job_events(database, job.fingerprint[:10])
    assert events[0]["event_type"] == "note"
    assert events[0]["note"] == "Follow up Friday"


def test_duplicate_latest_note_is_not_appended(tmp_path: Path) -> None:
    database = tmp_path / "jobs.sqlite3"
    job = Job(
        title="Backend Engineer",
        company="Example",
        source_url="https://example.test/jobs/1",
    )
    save_jobs([job], database)

    assert add_note(database, job.fingerprint[:10], "Same note") is True
    assert add_note(database, job.fingerprint[:10], "Same note") is False

    note_events = [
        event
        for event in list_job_events(database, job.fingerprint[:10])
        if event["event_type"] == "note"
    ]
    assert len(note_events) == 1
