import csv
import json
from pathlib import Path

from open_job_scout.exporting import write_export


def sample_row() -> dict:
    return {
        "fingerprint": "abc123",
        "title": "Junior Engineer",
        "company": "Example",
        "remote": 1,
        "work_mode": "remote",
        "score": 88.5,
        "status": "new",
        "reasons": '["python", "fully remote"]',
        "concerns": "[]",
    }


def test_json_export_normalizes_structured_fields(tmp_path: Path) -> None:
    output = tmp_path / "jobs.json"

    write_export([sample_row()], output, "json")

    payload = json.loads(output.read_text(encoding="utf-8"))
    assert payload[0]["remote"] is True
    assert payload[0]["reasons"] == ["python", "fully remote"]
    assert payload[0]["concerns"] == []


def test_csv_export_keeps_json_lists_portable(tmp_path: Path) -> None:
    output = tmp_path / "jobs.csv"

    write_export([sample_row()], output, "csv")

    with output.open(encoding="utf-8", newline="") as handle:
        row = next(csv.DictReader(handle))
    assert row["title"] == "Junior Engineer"
    assert json.loads(row["reasons"]) == ["python", "fully remote"]
