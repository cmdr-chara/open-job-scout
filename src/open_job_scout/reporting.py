from __future__ import annotations

import json
import math
import re
from collections.abc import Iterable, Mapping
from datetime import datetime
from pathlib import Path


def _decode_list(value: str) -> str:
    try:
        decoded = json.loads(value)
        if not isinstance(decoded, list):
            return _inline(value)
        return _inline(", ".join(str(item) for item in decoded))
    except (TypeError, ValueError, json.JSONDecodeError):
        return _inline(value)


def _remote_label(value: object) -> str:
    labels = {None: "unknown", 0: "no", 1: "yes"}
    return labels.get(value, "unknown")


def _inline(value: object) -> str:
    text = re.sub(r"\s+", " ", str(value or "")).strip()
    return re.sub(r"([\\`*_{}\[\]<>()#+!|])", r"\\\1", text)


def _url(value: object) -> str:
    return str(value or "").replace("<", "%3C").replace(">", "%3E")


def _amount(value: object) -> str:
    if value is None:
        return "?"
    number = float(value)
    if not math.isfinite(number):
        return "?"
    return f"{number:,.0f}" if number.is_integer() else f"{number:,.2f}"


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
                f"{_amount(row['salary_min'])}-{_amount(row['salary_max'])} "
                f"{_inline(row['currency'])}"
            ).strip()
        title = _inline(row["title"])
        company = _inline(row["company"])
        canonical_url = row["canonical_url"]
        source_url = row["source_url"]
        lines.extend(
            [
                f"## {index}. {title} - {company}",
                "",
                f"- ID: `{row['fingerprint'][:10]}`",
                f"- Score: **{row['score']:.1f}/100**",
                f"- Status: `{row['status']}`",
                f"- Location: {_inline(row['location']) or 'not provided'}",
                f"- Remote: {_remote_label(row['remote'])}",
                f"- Employment: {_inline(row['employment_type']) or 'not provided'}",
                f"- Salary: {salary}",
                f"- Posted: {_inline(row['posted_at']) or 'not provided'}",
                f"- Source: {_inline(row['source']) or 'not provided'}",
                f"- Verification: {row['verification_status']}",
                f"- Reasons: {_decode_list(row['reasons']) or 'none'}",
                f"- Concerns: {_decode_list(row['concerns']) or 'none'}",
                f"- URL: <{_url(canonical_url or source_url)}>",
                *(
                    [f"- Source URL: <{_url(source_url)}>"]
                    if canonical_url and canonical_url != source_url
                    else []
                ),
                *([f"- Notes: {_inline(row['notes'])}"] if row["notes"] else []),
                "",
            ]
        )
    output.write_text("\n".join(lines), encoding="utf-8")
    return output
