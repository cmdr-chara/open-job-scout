from __future__ import annotations

import json
import math
import textwrap
from collections.abc import Mapping


def _value(row: Mapping, key: str, default: object = None) -> object:
    try:
        return row[key]
    except (KeyError, IndexError):
        return default


def _decode_list(value: object) -> list[str]:
    if isinstance(value, list):
        return [str(item) for item in value]
    try:
        decoded = json.loads(str(value or "[]"))
    except (TypeError, ValueError, json.JSONDecodeError):
        return []
    if not isinstance(decoded, list):
        return []
    return [str(item) for item in decoded]


def _salary(row: Mapping) -> str:
    minimum = _value(row, "salary_min")
    maximum = _value(row, "salary_max")
    if minimum is None and maximum is None:
        return "not published"

    def amount(value: object) -> str:
        if value is None:
            return "?"
        number = float(value)
        if not math.isfinite(number):
            return "?"
        return f"{number:,.0f}" if number.is_integer() else f"{number:,.2f}"

    currency = str(_value(row, "currency") or "").strip()
    if minimum is not None and maximum is not None and float(minimum) == float(maximum):
        result = amount(minimum)
    else:
        result = f"{amount(minimum)} - {amount(maximum)}"
    if currency:
        result += f" {currency}"
    if source := _value(row, "salary_source"):
        result += f" ({source})"
    return result


def preferred_job_url(row: Mapping, *, source: bool = False) -> str:
    if source:
        return str(_value(row, "source_url") or "")
    return str(_value(row, "canonical_url") or _value(row, "source_url") or "")


def _description(value: object, *, full: bool) -> str:
    text = " ".join(str(value or "").split())
    if not text:
        return "not provided"
    if full:
        return text
    return textwrap.shorten(text, width=700, placeholder=" ...")


def format_job_detail(row: Mapping, *, full: bool = False) -> str:
    fingerprint = str(_value(row, "fingerprint") or "")
    title = str(_value(row, "title") or "Untitled role")
    company = str(_value(row, "company") or "Unknown company")
    status = str(_value(row, "status") or "unknown")
    work_mode = str(_value(row, "work_mode") or "unknown")
    verification = str(_value(row, "verification_status") or "unverified")
    score = float(_value(row, "score", 0) or 0)
    reasons = _decode_list(_value(row, "reasons", "[]"))
    concerns = _decode_list(_value(row, "concerns", "[]"))
    url = preferred_job_url(row)
    source_url = preferred_job_url(row, source=True)

    lines = [
        f"{title} — {company}",
        "=" * min(80, max(12, len(title) + len(company) + 3)),
        f"ID:           {fingerprint[:10]}",
        f"Score:        {score:.1f}/100",
        f"Status:       {status}",
        f"Work mode:    {work_mode}",
        f"Verification: {verification}",
        f"Location:     {_value(row, 'location') or 'not provided'}",
        f"Employment:   {_value(row, 'employment_type') or 'not provided'}",
        f"Salary:       {_salary(row)}",
        f"Posted:       {_value(row, 'posted_at') or 'not provided'}",
        f"Source:       {_value(row, 'source') or 'not provided'}",
        f"Last seen:    {_value(row, 'last_seen_at') or 'not provided'}",
        "",
        f"URL: {url or 'not provided'}",
    ]
    if source_url and source_url != url:
        lines.append(f"Source URL: {source_url}")

    lines.extend(["", "Why it ranked:"])
    lines.extend(f"  + {item}" for item in reasons)
    if not reasons:
        lines.append("  none recorded")

    lines.extend(["", "Concerns:"])
    lines.extend(f"  - {item}" for item in concerns)
    if not concerns:
        lines.append("  none recorded")

    if notes := str(_value(row, "notes") or "").strip():
        lines.extend(["", "Notes:", textwrap.indent(notes, "  ")])

    if replacement := _value(row, "replacement_url"):
        replacement_title = _value(row, "replacement_title") or "possible successor"
        lines.extend(["", f"Suggested successor: {replacement_title}", f"  {replacement}"])

    lines.extend(
        [
            "",
            "Description:" if full else "Description preview:",
            textwrap.fill(_description(_value(row, "description"), full=full), width=100),
            "",
            "Useful commands:",
            f"  jobscout open {fingerprint[:10]}",
            f"  jobscout mark {fingerprint[:10]} reviewed",
            f"  jobscout note {fingerprint[:10]} \"your note\"",
            f"  jobscout history {fingerprint[:10]}",
        ]
    )
    if not full:
        lines.append(f"  jobscout show {fingerprint[:10]} --full")
    return "\n".join(lines)
