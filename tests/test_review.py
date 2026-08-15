from pathlib import Path

from open_job_scout.database import find_job, list_job_events, save_jobs
from open_job_scout.models import Job
from open_job_scout.review import run_review_session


def test_review_session_can_note_open_and_mark(tmp_path: Path) -> None:
    database = tmp_path / "jobs.sqlite3"
    job = Job(
        title="Backend Engineer",
        company="Example",
        source_url="https://example.test/jobs/1",
        canonical_url="https://careers.example/jobs/1",
        score=80,
    )
    save_jobs([job], database)
    row = find_job(database, job.fingerprint[:10])

    answers = iter(["n", "Check team size", "o", "r"])
    output: list[str] = []
    opened: list[str] = []

    decisions = run_review_session(
        [row],
        database,
        open_job=lambda item: opened.append(str(item["canonical_url"])) or str(
            item["canonical_url"]
        ),
        input_func=lambda prompt: next(answers),
        output=output.append,
    )

    assert decisions == 1
    stored = find_job(database, job.fingerprint[:10])
    assert stored["status"] == "reviewed"
    assert "Check team size" in stored["notes"]
    assert opened == ["https://careers.example/jobs/1"]
    events = list_job_events(database, job.fingerprint[:10])
    assert {event["event_type"] for event in events} >= {"note", "status"}
    assert any("Marked" in line for line in output)


def test_review_session_skip_does_not_change_status(tmp_path: Path) -> None:
    database = tmp_path / "jobs.sqlite3"
    job = Job(
        title="Backend Engineer",
        company="Example",
        source_url="https://example.test/jobs/1",
    )
    save_jobs([job], database)
    row = find_job(database, job.fingerprint[:10])

    decisions = run_review_session(
        [row],
        database,
        open_job=lambda item: str(item["source_url"]),
        input_func=lambda prompt: "s",
        output=lambda text: None,
    )

    assert decisions == 0
    assert find_job(database, job.fingerprint[:10])["status"] == "new"
