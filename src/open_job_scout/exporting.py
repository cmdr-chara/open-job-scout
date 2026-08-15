from __future__ import annotations

import csv
import json
from collections.abc import Iterable, Mapping
from pathlib import Path

EXPORT_FIELDS = (
    "fingerprint",
    "title",
    "company",
    "location",
    "remote",
    "work_mode",
    "employment_type",
    "salary_min",
    "salary_max",
    "currency",
    "salary_source",
    "description",
    "posted_at",
    "source",
    "source_url",
    "canonical_url",
    "score",
    "status",
    "verification_status",
    "verification_source",
    "replacement_url",
    "replacement_title",
    "first_seen_at",
    "last_seen_at",
    "status_updated_at",
    "notes",
    "reasons",
    "concerns",
)


def _value(row: Mapping, field: str) -> object:
    try:
        return row[field]
    except (KeyError, IndexError):
        return None


def _decode_json_list(value: object) -> list[str]:
    if isinstance(value, list):
        return [str(item) for item in value]
    if not isinstance(value, str):
        return []
    try:
        decoded = json.loads(value)
    except json.JSONDecodeError:
        return [value] if value else []
    return [str(item) for item in decoded] if isinstance(decoded, list) else []


def export_record(row: Mapping) -> dict[str, object]:
    record = {field: _value(row, field) for field in EXPORT_FIELDS}
    if record["remote"] in {0, 1}:
        record["remote"] = bool(record["remote"])
    record["reasons"] = _decode_json_list(record["reasons"])
    record["concerns"] = _decode_json_list(record["concerns"])
    return record


def write_export(rows: Iterable[Mapping], output: Path, format: str) -> Path:
    records = [export_record(row) for row in rows]
    output.parent.mkdir(parents=True, exist_ok=True)
    if format == "json":
        output.write_text(
            json.dumps(records, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
        return output
    if format == "csv":
        with output.open("w", encoding="utf-8", newline="") as handle:
            writer = csv.DictWriter(handle, fieldnames=EXPORT_FIELDS)
            writer.writeheader()
            for record in records:
                csv_record = dict(record)
                for field in ("reasons", "concerns"):
                    csv_record[field] = json.dumps(record[field], ensure_ascii=False)
                writer.writerow(csv_record)
        return output
    raise ValueError(f"Unsupported export format: {format}")
