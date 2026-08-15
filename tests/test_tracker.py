from pathlib import Path

from open_job_scout.database import mark_job, save_jobs
from open_job_scout.models import Job
from open_job_scout.tracker import query_jobs, tracker_summary


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
