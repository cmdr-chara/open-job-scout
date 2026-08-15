import argparse
import tomllib
from importlib.resources import files
from pathlib import Path

import pytest

from open_job_scout.cli import _collect_and_save, build_parser
from open_job_scout.database import save_jobs
from open_job_scout.models import Job


def config_for(tmp_path: Path) -> dict:
    resource = files("open_job_scout").joinpath("default_config.toml")
    with resource.open("rb") as handle:
        settings = tomllib.load(handle)
    settings["storage"] = {
        "database": str(tmp_path / "jobs.sqlite3"),
        "report_directory": str(tmp_path / "reports"),
    }
    return settings


def test_automatic_report_contains_only_current_collection(tmp_path: Path) -> None:
    settings = config_for(tmp_path)
    database = Path(settings["storage"]["database"])
    old = Job(
        title="Old High Score",
        company="Archive",
        source_url="https://example.test/old",
        score=99,
    )
    save_jobs([old], database)
    current = Job(
        title="Current Role",
        company="Example",
        source_url="https://example.test/current",
    )

    result = _collect_and_save(
        [current],
        argparse.Namespace(config=None, no_verify=True),
        settings,
    )

    assert result == 0
    reports = list((tmp_path / "reports").glob("jobs_*.md"))
    assert len(reports) == 1
    content = reports[0].read_text(encoding="utf-8")
    assert "Current Role" in content
    assert "Old High Score" not in content


def test_limits_must_be_positive() -> None:
    with pytest.raises(SystemExit):
        build_parser().parse_args(["list", "--limit", "0"])


def test_queue_filters_and_export_options_parse() -> None:
    list_args = build_parser().parse_args(
        [
            "list",
            "--status",
            "new",
            "--work-mode",
            "remote",
            "--source",
            "linkedin",
            "--min-score",
            "70",
            "--query",
            "python",
            "--sort",
            "newest",
        ]
    )
    assert list_args.work_mode == "remote"
    assert list_args.source == "linkedin"
    assert list_args.min_score == 70
    assert list_args.query == "python"
    assert list_args.sort == "newest"

    export_args = build_parser().parse_args(["export", "--format", "json"])
    assert export_args.format == "json"
    assert export_args.limit is None

    stats_args = build_parser().parse_args(["stats"])
    assert stats_args.command == "stats"


def test_friendly_review_commands_parse() -> None:
    show = build_parser().parse_args(["show", "abc123", "--json", "--full"])
    assert show.id == "abc123"
    assert show.json is True
    assert show.full is True

    open_args = build_parser().parse_args(["open", "abc123", "--source"])
    assert open_args.id == "abc123"
    assert open_args.source is True

    note = build_parser().parse_args(["note", "abc123", "Follow up Friday"])
    assert note.id == "abc123"
    assert note.text == "Follow up Friday"

    next_args = build_parser().parse_args(
        ["next", "--work-mode", "remote", "--min-score", "70", "--open"]
    )
    assert next_args.work_mode == "remote"
    assert next_args.min_score == 70
    assert next_args.open is True

    review = build_parser().parse_args(
        ["review", "--work-mode", "remote", "--min-score", "60", "--limit", "5"]
    )
    assert review.work_mode == "remote"
    assert review.min_score == 60
    assert review.limit == 5

    alias = build_parser().parse_args(["view", "abc123"])
    assert alias.id == "abc123"
    assert callable(alias.handler)


def test_history_recheck_and_doctor_options_parse() -> None:
    history = build_parser().parse_args(["history", "abc123", "--json", "--limit", "10"])
    assert history.id == "abc123"
    assert history.json is True
    assert history.limit == 10

    recheck = build_parser().parse_args(
        ["recheck", "abc123", "def456", "--workers", "3"]
    )
    assert recheck.ids == ["abc123", "def456"]
    assert recheck.workers == 3
    assert recheck.limit == 50

    doctor = build_parser().parse_args(["doctor", "--json"])
    assert doctor.json is True


def test_min_score_must_be_in_range() -> None:
    with pytest.raises(SystemExit):
        build_parser().parse_args(["list", "--min-score", "101"])
