from __future__ import annotations

import argparse
import json
import subprocess
import tempfile
from contextlib import closing
from pathlib import Path

from open_job_scout.database import (
    SCHEMA_VERSION,
    connect,
    find_job,
    list_job_events,
    mark_job,
    save_jobs,
    set_next_action,
)
from open_job_scout.models import Job
from open_job_scout.tracker import tracker_insights

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_CONFIG = ROOT / "examples" / "config.example.toml"
VECTORS = json.loads(
    (ROOT / "tests" / "fixtures" / "runtime_contract_vectors.json").read_text(
        encoding="utf-8"
    )
)


def _run(binary: Path, database: Path, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            str(binary),
            "--database",
            str(database),
            "--config",
            str(DEFAULT_CONFIG),
            *args,
        ],
        check=True,
        capture_output=True,
        text=True,
    )


def _show(binary: Path, database: Path, fingerprint: str) -> dict[str, object]:
    result = _run(binary, database, "show", fingerprint[:10], "--json")
    return json.loads(result.stdout)


def _sample_job(suffix: str) -> Job:
    return Job(
        title="Junior Backend Engineer",
        company="Parity Labs",
        source_url=f"https://example.test/source/{suffix}",
        location="Italy",
        remote=True,
        work_mode="remote",
        employment_type="fulltime",
        description="Python backend role. Fully remote within Italy.",
        source="contract",
        canonical_url=f"https://example.test/jobs/{suffix}",
        score=73.5,
        reasons=["runtime parity fixture"],
        verification_status="verified",
        verification_source="contract",
    )


def _check_rust_created_schema(binary: Path, directory: Path) -> None:
    database = directory / "rust-created.db"
    _run(binary, database, "list")
    with closing(connect(database)) as connection:
        version = connection.execute("PRAGMA user_version").fetchone()[0]
        tables = {
            row[0]
            for row in connection.execute(
                "SELECT name FROM sqlite_master WHERE type='table'"
            ).fetchall()
        }
    assert version == SCHEMA_VERSION
    assert {"jobs", "job_events"}.issubset(tables)

    job = _sample_job("rust-created")
    save_jobs([job], database)
    shown = _show(binary, database, job.fingerprint)
    assert shown["fingerprint"] == job.fingerprint
    assert shown["status"] == "new"


def _check_python_created_round_trip(binary: Path, directory: Path) -> None:
    database = directory / "python-created.db"
    job = _sample_job("python-created")
    save_jobs([job], database)

    shown = _show(binary, database, job.fingerprint)
    assert shown["fingerprint"] == job.fingerprint
    assert shown["work_mode"] == "remote"

    _run(
        binary,
        database,
        "mark",
        job.fingerprint[:10],
        "applied",
        "--note",
        "rust round-trip",
    )
    python_row = find_job(database, job.fingerprint[:10])
    assert python_row["status"] == "applied"
    assert python_row["status_manually_set"] == 1
    assert any(
        event["event_type"] == "status" and event["new_value"] == "applied"
        for event in list_job_events(database, job.fingerprint[:10])
    )

    mark_job(database, job.fingerprint[:10], "interview", "python round-trip")
    shown = _show(binary, database, job.fingerprint)
    assert shown["status"] == "interview"

    history = json.loads(
        _run(binary, database, "history", job.fingerprint[:10], "--json").stdout
    )
    transitions = {
        (event["old_value"], event["new_value"])
        for event in history
        if event["event_type"] == "status"
    }
    assert ("new", "applied") in transitions
    assert ("applied", "interview") in transitions


def _check_work_mode_parity(binary: Path, directory: Path) -> None:
    database = directory / "work-modes.db"
    jobs: list[tuple[Job, str]] = []
    for index, vector in enumerate(VECTORS["work_modes"]):
        job = Job(
            title=vector["title"],
            company=f"Mode Contract {index}",
            source_url=f"https://example.test/mode/{index}",
            location=vector["location"],
            description=vector["description"],
            remote=vector["remote"],
            work_mode=vector["work_mode"],
            employment_type="fulltime",
            source="contract",
        )
        jobs.append((job, vector["expected"]))

    save_jobs((job for job, _ in jobs), database)
    _run(binary, database, "rerank")
    for job, expected in jobs:
        shown = _show(binary, database, job.fingerprint)
        assert shown["work_mode"] == expected, (
            f"Rust work-mode drift for {job.fingerprint[:10]}: "
            f"expected {expected}, got {shown['work_mode']}"
        )


def _check_follow_up_parity(binary: Path, directory: Path) -> None:
    database = directory / "follow-ups.db"
    job = _sample_job("follow-up")
    save_jobs([job], database)
    set_next_action(database, job.fingerprint[:10], "2099-01-02", "check recruiter reply")
    shown = _show(binary, database, job.fingerprint)
    assert shown["next_action_at"] == "2099-01-02"
    assert shown["next_action_note"] == "check recruiter reply"
    due = json.loads(_run(binary, database, "due", "--days", "27000", "--json").stdout)
    assert due and due[0]["fingerprint"] == job.fingerprint
    _run(binary, database, "follow-up", job.fingerprint[:10], "--clear")
    assert find_job(database, job.fingerprint[:10])["next_action_at"] is None


def _check_stats_parity(binary: Path, directory: Path) -> None:
    database = directory / "stats.db"
    job = _sample_job("stats")
    save_jobs([job], database)
    mark_job(database, job.fingerprint[:10], "rejected", "parity rejection")
    expected = tracker_insights(database)["funnel"]
    actual = json.loads(_run(binary, database, "stats", "--json").stdout)["insights"]["funnel"]
    for key in (
        "reviewed_or_beyond",
        "applied_or_beyond",
        "interview_or_beyond",
        "offers",
        "review_to_application_pct",
    ):
        assert actual[key] == expected[key], (
            f"Rust stats drift for {key}: expected {expected[key]}, got {actual[key]}"
        )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error(f"Rust binary does not exist: {binary}")

    with tempfile.TemporaryDirectory(prefix="openjobscout-parity-") as temp:
        directory = Path(temp)
        _check_rust_created_schema(binary, directory)
        _check_python_created_round_trip(binary, directory)
        _check_work_mode_parity(binary, directory)
        _check_follow_up_parity(binary, directory)
        _check_stats_parity(binary, directory)

    print(
        "Python/Rust runtime parity OK: schema, tracker round-trip, history, "
        "work mode, follow-up, stats"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
