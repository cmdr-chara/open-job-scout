from __future__ import annotations

import json
from collections.abc import Iterable, Mapping
from datetime import datetime
from pathlib import Path


def _decode_list(value: str) -> str:
    try:
        return ", ".join(json.loads(value))
    except (TypeError, ValueError, json.JSONDecodeError):
        return value or ""


def _remote_label(value: object) -> str:
    labels = {None: "unknown", 0: "no", 1: "yes"}
    return labels.get(value, "unknown")


def write_markdown(rows: Iterable[Mapping], output: Path) -> Path:
    rows = list(rows)
    output.parent.mkdir(parents=True, exist_ok=True)
    lines = [
        f"# OpenJobScout report - {datetime.now():%Y-%m-%d %H:%M}",
        "",
        f"Jobs: **{len(rows)}**",
        "",
    ]
    for index, row in enumerate(rows, 1):
        salary = "not published"
        if row["salary_min"] is not None or row["salary_max"] is not None:
            salary = (
                f"{row['salary_min'] or '?'}-{row['salary_max'] or '?'} "
                f"{row['currency'] or ''}"
            ).strip()
        lines.extend(
            [
                f"## {index}. {row['title']} - {row['company']}",
                "",
                f"- ID: `{row['fingerprint'][:10]}`",
                f"- Score: **{row['score']:.1f}/100**",
                f"- Status: `{row['status']}`",
                f"- Location: {row['location'] or 'not provided'}",
                f"- Remote: {_remote_label(row['remote'])}",
                f"- Salary: {salary}",
                f"- Verification: {row['verification_status']}",
                f"- Reasons: {_decode_list(row['reasons']) or 'none'}",
                f"- Concerns: {_decode_list(row['concerns']) or 'none'}",
                f"- URL: {row['canonical_url'] or row['source_url']}",
                "",
            ]
        )
    output.write_text("\n".join(lines), encoding="utf-8")
    return output
