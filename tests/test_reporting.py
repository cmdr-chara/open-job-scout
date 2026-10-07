from pathlib import Path

from open_job_scout.database import find_job, mark_job, save_jobs, set_next_action
from open_job_scout.models import Job
from open_job_scout.reporting import write_markdown


def test_report_formats_salary_links_and_notes(tmp_path: Path) -> None:
    database = tmp_path / "jobs.sqlite3"
    job = Job(
        title="Backend Engineer",
        company="Example",
        source_url="https://board.example/jobs/1",
        canonical_url="https://careers.example/jobs/1",
        salary_min=40_000,
        salary_max=55_000,
        currency="EUR",
    )
    save_jobs([job], database)
    mark_job(database, job.fingerprint[:10], "applied", "Applied on the official page")
    set_next_action(database, job.fingerprint[:10], "2026-10-14", "Ask about interview timing")
    row = find_job(database, job.fingerprint[:10])

    output = write_markdown([row], tmp_path / "report.md")
    content = output.read_text(encoding="utf-8")

    assert "40,000-55,000 EUR" in content
    assert "<https://careers.example/jobs/1>" in content
    assert "Source URL: <https://board.example/jobs/1>" in content
    assert "Notes: Applied on the official page" in content
    assert "Next action: 2026-10-14 — Ask about interview timing" in content


def test_report_escapes_untrusted_markdown_fields(tmp_path: Path) -> None:
    database = tmp_path / "jobs.sqlite3"
    job = Job(
        title="![track](https://attacker.invalid/pixel)",
        company="Example",
        source_url="https://board.example/jobs/1",
    )
    save_jobs([job], database)
    row = find_job(database, job.fingerprint[:10])

    output = write_markdown([row], tmp_path / "report.md")
    content = output.read_text(encoding="utf-8")

    assert r"\!\[track\]\(https://attacker.invalid/pixel\)" in content
    assert "## 1. ![track]" not in content


def test_report_rejects_local_and_credential_links(tmp_path: Path) -> None:
    database = tmp_path / "jobs.sqlite3"
    job = Job(
        title="Engineer",
        company="Example",
        source_url="http://127.0.0.1/private",
        canonical_url="https://user:pass@example.com/job",
    )
    save_jobs([job], database)
    row = find_job(database, job.fingerprint[:10])

    content = write_markdown([row], tmp_path / "report.md").read_text(encoding="utf-8")

    assert "<http://127.0.0.1/private>" not in content
    assert "<https://user:pass@example.com/job>" not in content
