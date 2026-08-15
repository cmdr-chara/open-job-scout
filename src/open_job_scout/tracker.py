from __future__ import annotations

from contextlib import closing
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
