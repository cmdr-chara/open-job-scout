from __future__ import annotations

from collections import Counter
from contextlib import closing
from datetime import UTC, date, datetime, timedelta
from pathlib import Path
from typing import Any

from .database import VALID_STATUSES, connect

VALID_WORK_MODES = {"remote", "hybrid", "onsite", "unknown"}
SORT_ORDERS = {
    "score": "score DESC, last_seen_at DESC",
    "newest": "last_seen_at DESC, score DESC",
}


def _escape_like(value: str) -> str:
    return value.replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_")


def query_jobs(
    path: Path,
    *,
    status: str | None = None,
    work_mode: str | None = None,
    source: str | None = None,
    min_score: float | None = None,
    query: str | None = None,
    sort: str = "score",
    limit: int | None = 20,
) -> list:
    if status is not None and status not in VALID_STATUSES:
        raise ValueError(f"Invalid status: {status}")
    if work_mode is not None and work_mode not in VALID_WORK_MODES:
        raise ValueError(f"Invalid work mode: {work_mode}")
    if min_score is not None and not 0 <= min_score <= 100:
        raise ValueError("Minimum score must be between 0 and 100.")
    if sort not in SORT_ORDERS:
        raise ValueError(f"Invalid sort order: {sort}")
    if limit is not None and limit < 1:
        raise ValueError("Limit must be at least 1.")

    clauses: list[str] = []
    parameters: list[Any] = []
    if status:
        clauses.append("status=?")
        parameters.append(status)
    if work_mode:
        clauses.append("work_mode=?")
        parameters.append(work_mode)
    if source:
        clauses.append("LOWER(source)=LOWER(?)")
        parameters.append(source.strip())
    if min_score is not None:
        clauses.append("score>=?")
        parameters.append(min_score)
    if query and query.strip():
        needle = f"%{_escape_like(query.strip())}%"
        clauses.append(
            "("
            "title LIKE ? ESCAPE '\\' OR "
            "company LIKE ? ESCAPE '\\' OR "
            "COALESCE(location, '') LIKE ? ESCAPE '\\' OR "
            "COALESCE(description, '') LIKE ? ESCAPE '\\' OR "
            "COALESCE(notes, '') LIKE ? ESCAPE '\\'"
            ")"
        )
        parameters.extend([needle] * 5)

    where = f" WHERE {' AND '.join(clauses)}" if clauses else ""
    sql = f"SELECT * FROM jobs{where} ORDER BY {SORT_ORDERS[sort]}"
    if limit is not None:
        sql += " LIMIT ?"
        parameters.append(limit)

    with closing(connect(path)) as connection:
        return connection.execute(sql, parameters).fetchall()


def due_actions(path: Path, *, through: date | None = None, limit: int | None = 20) -> list:
    """Return scheduled follow-ups due by ``through`` in date order."""
    through = through or date.today()
    if limit is not None and limit < 1:
        raise ValueError("Limit must be at least 1.")
    sql = (
        "SELECT * FROM jobs "
        "WHERE next_action_at IS NOT NULL AND next_action_at <= ? "
        "ORDER BY next_action_at ASC, score DESC, last_seen_at DESC"
    )
    parameters: list[Any] = [through.isoformat()]
    if limit is not None:
        sql += " LIMIT ?"
        parameters.append(limit)
    with closing(connect(path)) as connection:
        return connection.execute(sql, parameters).fetchall()


def tracker_summary(path: Path) -> dict[str, object]:
    with closing(connect(path)) as connection:
        overview = connection.execute(
            """
            SELECT
                COUNT(*) AS total,
                COALESCE(AVG(score), 0) AS average_score,
                SUM(CASE WHEN salary_min IS NOT NULL OR salary_max IS NOT NULL THEN 1 ELSE 0 END)
                    AS salary_published
            FROM jobs
            """
        ).fetchone()
        statuses = {
            row["status"]: row["count"]
            for row in connection.execute(
                "SELECT status, COUNT(*) AS count FROM jobs GROUP BY status ORDER BY status"
            )
        }
        work_modes = {
            row["work_mode"]: row["count"]
            for row in connection.execute(
                """
                SELECT work_mode, COUNT(*) AS count
                FROM jobs
                GROUP BY work_mode
                ORDER BY count DESC, work_mode
                """
            )
        }
        sources = {
            row["source"] or "unknown": row["count"]
            for row in connection.execute(
                """
                SELECT source, COUNT(*) AS count
                FROM jobs
                GROUP BY source
                ORDER BY count DESC, source
                """
            )
        }
        top_new = [
            dict(row)
            for row in connection.execute(
                """
                SELECT fingerprint, title, company, score
                FROM jobs
                WHERE status='new'
                ORDER BY score DESC, last_seen_at DESC
                LIMIT 5
                """
            )
        ]

    total = int(overview["total"])
    return {
        "total": total,
        "average_score": round(float(overview["average_score"]), 1),
        "salary_published": int(overview["salary_published"] or 0),
        "statuses": statuses,
        "work_modes": work_modes,
        "sources": sources,
        "top_new": top_new,
    }


def tracker_insights(path: Path, *, now: datetime | None = None) -> dict[str, object]:
    """Return decision-oriented pipeline metrics without changing the tracker.

    The result is deliberately JSON-safe so the CLI can be used as a small
    local API by dashboards and shell scripts. All values come from the local
    SQLite tracker; no network request is made.
    """

    now = now or datetime.now().astimezone()
    today = now.date()
    now = now.replace(tzinfo=UTC) if now.tzinfo is None else now.astimezone(UTC)
    with closing(connect(path)) as connection:
        rows = connection.execute("SELECT * FROM jobs").fetchall()
        events = connection.execute(
            "SELECT event_type, old_value, new_value FROM job_events"
        ).fetchall()

    total = len(rows)
    statuses = Counter(str(row["status"] or "unknown") for row in rows)
    verification = Counter(str(row["verification_status"] or "unknown") for row in rows)
    work_modes = Counter(str(row["work_mode"] or "unknown") for row in rows)
    sources = Counter(str(row["source"] or "unknown") for row in rows)

    def ratio(numerator: int, denominator: int) -> float:
        return round(numerator / denominator * 100, 1) if denominator else 0.0

    def age_days(value: object) -> int | None:
        if not value:
            return None
        try:
            parsed = datetime.fromisoformat(str(value).replace("Z", "+00:00"))
        except ValueError:
            return None
        if parsed.tzinfo is None:
            parsed = parsed.replace(tzinfo=UTC)
        age = (now - parsed.astimezone(UTC)).days
        return age if age >= 0 else None

    recently_seen = sum(
        (age := age_days(row["last_seen_at"])) is not None and age <= 7 for row in rows
    )
    recently_posted = sum(
        (age := age_days(row["posted_at"])) is not None and age <= 7 for row in rows
    )
    due_today = 0
    overdue = 0
    due_next_7_days = 0
    for row in rows:
        value = str(row["next_action_at"] or "")
        if not value:
            continue
        try:
            action_date = date.fromisoformat(value)
        except ValueError:
            continue
        if action_date < today:
            overdue += 1
        elif action_date == today:
            due_today += 1
        elif action_date <= today + timedelta(days=7):
            due_next_7_days += 1
    unverified = sum(
        row["verification_status"] in {"unverified", "unreachable"} for row in rows
    )
    status_transitions = sum(
        event["event_type"] == "status"
        and event["old_value"] is not None
        and event["new_value"] is not None
        for event in events
    )
    applied = sum(statuses.get(status, 0) for status in ("applied", "interview", "offer"))
    interviews = sum(statuses.get(status, 0) for status in ("interview", "offer"))
    offers = statuses.get("offer", 0)
    reviewed = sum(
        statuses.get(status, 0)
        for status in ("reviewed", "applied", "interview", "rejected", "offer")
    )

    action_items: list[dict[str, object]] = []
    if statuses.get("new", 0):
        action_items.append(
            {
                "type": "review",
                "count": statuses["new"],
                "message": f"Review {statuses['new']} new job(s)",
            }
        )
    if unverified:
        action_items.append(
            {
                "type": "verify",
                "count": unverified,
                "message": f"Recheck {unverified} job(s) with uncertain verification",
            }
        )
    if statuses.get("stale", 0):
        action_items.append(
            {
                "type": "stale",
                "count": statuses["stale"],
                "message": f"Refresh {statuses['stale']} stale job(s) or archive them",
            }
        )
    if overdue:
        action_items.append(
            {
                "type": "follow_up_overdue",
                "count": overdue,
                "message": f"Follow up on {overdue} overdue action(s)",
            }
        )
    if due_today:
        action_items.append(
            {
                "type": "follow_up_today",
                "count": due_today,
                "message": f"Complete {due_today} follow-up action(s) today",
            }
        )

    return {
        "total": total,
        "statuses": dict(sorted(statuses.items())),
        "verification": dict(sorted(verification.items())),
        "work_modes": dict(sorted(work_modes.items(), key=lambda item: (-item[1], item[0]))),
        "sources": dict(sorted(sources.items(), key=lambda item: (-item[1], item[0]))),
        "freshness": {
            "seen_last_7_days": recently_seen,
            "posted_last_7_days": recently_posted,
        },
        "follow_ups": {
            "overdue": overdue,
            "due_today": due_today,
            "due_next_7_days": due_next_7_days,
        },
        "funnel": {
            "reviewed_or_beyond": reviewed,
            "applied_or_beyond": applied,
            "interview_or_beyond": interviews,
            "offers": offers,
            "review_to_application_pct": ratio(applied, reviewed),
            "application_to_interview_pct": ratio(interviews, applied),
            "interview_to_offer_pct": ratio(offers, interviews),
            "recorded_status_transitions": status_transitions,
        },
        "average_score": round(
            sum(float(row["score"] or 0) for row in rows) / total, 1
        )
        if total
        else 0.0,
        "action_items": action_items,
    }
