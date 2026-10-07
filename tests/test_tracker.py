import sqlite3
from datetime import UTC, datetime, timedelta
from pathlib import Path

from open_job_scout.database import mark_job, save_jobs, set_next_action
from open_job_scout.models import Job
from open_job_scout.tracker import due_actions, query_jobs, tracker_insights, tracker_summary


def sample_jobs() -> list[Job]:
    return [
        Job(
            title="Junior Python Engineer",
            company="Acme",
            source_url="https://example.test/1",
            location="Remote - Italy",
            description="Build Python APIs for a product team.",
            source="linkedin",
            work_mode="remote",
            score=92,
        ),
        Job(
            title="Data Analyst",
            company="Beta",
            source_url="https://example.test/2",
            location="Remote - EU",
            description="SQL and analytics.",
            source="linkedin",
            work_mode="remote",
            score=64,
        ),
        Job(
            title="Backend Engineer",
            company="Gamma",
            source_url="https://example.test/3",
            location="Milan",
            description="Backend services.",
            source="google",
            work_mode="hybrid",
            score=81,
        ),
    ]


def test_query_jobs_combines_queue_filters(tmp_path: Path) -> None:
    database = tmp_path / "jobs.sqlite3"
    jobs = sample_jobs()
    save_jobs(jobs, database)
    mark_job(database, jobs[1].fingerprint[:10], "applied")

    rows = query_jobs(
        database,
        status="new",
        work_mode="remote",
        source="LINKEDIN",
        min_score=70,
        query="python",
        limit=None,
    )

    assert [row["title"] for row in rows] == ["Junior Python Engineer"]


def test_query_treats_like_wildcards_as_literal_text(tmp_path: Path) -> None:
    database = tmp_path / "jobs.sqlite3"
    jobs = [
        Job(
            title="100% Remote Engineer",
            company="Acme",
            source_url="https://example.test/exact",
            score=80,
        ),
        Job(
            title="1000 Remote Engineer",
            company="Beta",
            source_url="https://example.test/other",
            score=70,
        ),
    ]
    save_jobs(jobs, database)

    rows = query_jobs(database, query="100%", limit=None)

    assert [row["title"] for row in rows] == ["100% Remote Engineer"]


def test_tracker_summary_reports_pipeline_and_top_new(tmp_path: Path) -> None:
    database = tmp_path / "jobs.sqlite3"
    jobs = sample_jobs()
    jobs[0].salary_max = 55_000
    save_jobs(jobs, database)
    mark_job(database, jobs[1].fingerprint[:10], "applied")

    summary = tracker_summary(database)

    assert summary["total"] == 3
    assert summary["average_score"] == 79.0
    assert summary["salary_published"] == 1
    assert summary["statuses"] == {"applied": 1, "new": 2}
    assert summary["work_modes"] == {"remote": 2, "hybrid": 1}
    assert summary["sources"] == {"linkedin": 2, "google": 1}
    assert summary["top_new"][0]["title"] == "Junior Python Engineer"


def test_tracker_insights_counts_jobs_seen_and_posted_today(tmp_path: Path) -> None:
    database = tmp_path / "jobs.sqlite3"
    jobs = sample_jobs()[:2]
    save_jobs(jobs, database)
    now = datetime(2026, 8, 20, 12, 0, tzinfo=UTC)
    old = now - timedelta(days=8)
    with sqlite3.connect(database) as connection:
        connection.execute(
            "UPDATE jobs SET last_seen_at=?, posted_at=? WHERE fingerprint=?",
            (now.isoformat(), now.date().isoformat(), jobs[0].fingerprint),
        )
        connection.execute(
            "UPDATE jobs SET last_seen_at=?, posted_at=? WHERE fingerprint=?",
            (old.isoformat(), old.date().isoformat(), jobs[1].fingerprint),
        )

    insights = tracker_insights(database, now=now)

    assert insights["freshness"] == {"seen_last_7_days": 1, "posted_last_7_days": 1}


def test_tracker_insights_excludes_future_dates_from_freshness(tmp_path: Path) -> None:
    database = tmp_path / "jobs.sqlite3"
    job = sample_jobs()[0]
    save_jobs([job], database)
    now = datetime(2026, 8, 20, 12, 0, tzinfo=UTC)
    future = now + timedelta(days=2)
    with sqlite3.connect(database) as connection:
        connection.execute(
            "UPDATE jobs SET last_seen_at=?, posted_at=? WHERE fingerprint=?",
            (future.isoformat(), future.date().isoformat(), job.fingerprint),
        )

    insights = tracker_insights(database, now=now)

    assert insights["freshness"] == {"seen_last_7_days": 0, "posted_last_7_days": 0}


def test_tracker_insights_reports_funnel_and_follow_up_actions(tmp_path: Path) -> None:
    database = tmp_path / "jobs.sqlite3"
    jobs = sample_jobs()
    save_jobs(jobs, database)
    mark_job(database, jobs[0].fingerprint[:10], "reviewed")
    mark_job(database, jobs[0].fingerprint[:10], "applied")
    mark_job(database, jobs[1].fingerprint[:10], "interview")

    insights = tracker_insights(database, now=datetime(2026, 8, 20, tzinfo=UTC))

    assert insights["funnel"] == {
        "reviewed_or_beyond": 2,
        "applied_or_beyond": 2,
        "interview_or_beyond": 1,
        "offers": 0,
        "review_to_application_pct": 100.0,
        "application_to_interview_pct": 50.0,
        "interview_to_offer_pct": 0.0,
        "recorded_status_transitions": 3,
    }
    assert insights["action_items"] == [
        {"type": "review", "count": 1, "message": "Review 1 new job(s)"},
        {
            "type": "verify",
            "count": 3,
            "message": "Recheck 3 job(s) with uncertain verification",
        },
    ]


def test_tracker_insights_empty_database_is_json_safe(tmp_path: Path) -> None:
    insights = tracker_insights(
        tmp_path / "empty.sqlite3", now=datetime(2026, 8, 20, tzinfo=UTC)
    )

    assert insights["total"] == 0
    assert insights["average_score"] == 0.0
    assert insights["freshness"] == {"seen_last_7_days": 0, "posted_last_7_days": 0}
    assert insights["action_items"] == []
    assert all(value == 0.0 for key, value in insights["funnel"].items() if key.endswith("pct"))


def test_follow_up_dates_are_persistent_due_and_actionable(tmp_path: Path) -> None:
    database = tmp_path / "jobs.sqlite3"
    jobs = sample_jobs()[:2]
    save_jobs(jobs, database)
    set_next_action(database, jobs[0].fingerprint[:10], "2026-08-18", "send recruiter email")
    set_next_action(database, jobs[1].fingerprint[:10], "2026-08-25", "check response")

    due = due_actions(database, through=datetime(2026, 8, 20, tzinfo=UTC).date())
    assert [row["title"] for row in due] == [jobs[0].title]
    insights = tracker_insights(database, now=datetime(2026, 8, 20, tzinfo=UTC))
    assert insights["follow_ups"] == {
        "overdue": 1,
        "due_today": 0,
        "due_next_7_days": 1,
    }
    assert insights["action_items"][-1] == {
        "type": "follow_up_overdue",
        "count": 1,
        "message": "Follow up on 1 overdue action(s)",
    }


def test_follow_up_can_be_cleared_without_losing_status(tmp_path: Path) -> None:
    database = tmp_path / "jobs.sqlite3"
    job = sample_jobs()[0]
    save_jobs([job], database)
    mark_job(database, job.fingerprint[:10], "applied")
    set_next_action(database, job.fingerprint[:10], "2026-08-20", "follow up")
    assert set_next_action(database, job.fingerprint[:10], None) is True
    row = query_jobs(database, status="applied", limit=None)[0]
    assert row["next_action_at"] is None
    assert row["status"] == "applied"


def test_rescheduling_follow_up_keeps_existing_note(tmp_path: Path) -> None:
    database = tmp_path / "jobs.sqlite3"
    job = sample_jobs()[0]
    save_jobs([job], database)
    set_next_action(database, job.fingerprint[:10], "2026-08-20", "email recruiter")

    assert set_next_action(database, job.fingerprint[:10], "2026-08-25") is True
    row = query_jobs(database, limit=None)[0]
    assert row["next_action_at"] == "2026-08-25"
    assert row["next_action_note"] == "email recruiter"
