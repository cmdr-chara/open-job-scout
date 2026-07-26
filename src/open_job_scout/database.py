from __future__ import annotations

import json
import sqlite3
from collections.abc import Iterable
from contextlib import closing
from datetime import UTC, datetime
from pathlib import Path

from .models import Job

VALID_STATUSES = {"new", "reviewed", "applied", "interview", "rejected", "offer", "closed"}

SCHEMA = """
CREATE TABLE IF NOT EXISTS jobs (
    fingerprint TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    company TEXT NOT NULL,
    location TEXT,
    remote INTEGER,
    employment_type TEXT,
    salary_min REAL,
    salary_max REAL,
    currency TEXT,
    salary_source TEXT,
    description TEXT,
    posted_at TEXT,
    source TEXT,
    source_url TEXT,
    canonical_url TEXT,
    score REAL NOT NULL DEFAULT 0,
    reasons TEXT NOT NULL DEFAULT '[]',
    concerns TEXT NOT NULL DEFAULT '[]',
    verification_status TEXT NOT NULL DEFAULT 'unverified',
    verification_source TEXT,
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'new',
    status_updated_at TEXT,
    notes TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_jobs_status_score ON jobs(status, score DESC);
CREATE INDEX IF NOT EXISTS idx_jobs_last_seen ON jobs(last_seen_at DESC);
"""


def connect(path: Path) -> sqlite3.Connection:
    path.parent.mkdir(parents=True, exist_ok=True)
    connection = sqlite3.connect(path)
    connection.row_factory = sqlite3.Row
    connection.executescript(SCHEMA)
    return connection


def save_jobs(jobs: Iterable[Job], path: Path) -> int:
    now = datetime.now(UTC).isoformat()
    count = 0
    with closing(connect(path)) as connection, connection:
        for job in jobs:
            connection.execute(
                """
                INSERT INTO jobs (
                    fingerprint,title,company,location,remote,employment_type,
                    salary_min,salary_max,currency,salary_source,description,posted_at,
                    source,source_url,canonical_url,score,reasons,concerns,
                    verification_status,verification_source,first_seen_at,last_seen_at,status
                ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
                ON CONFLICT(fingerprint) DO UPDATE SET
                    title=excluded.title,
                    company=excluded.company,
                    location=excluded.location,
                    remote=excluded.remote,
                    employment_type=excluded.employment_type,
                    salary_min=excluded.salary_min,
                    salary_max=excluded.salary_max,
                    currency=excluded.currency,
                    salary_source=excluded.salary_source,
                    description=excluded.description,
                    posted_at=excluded.posted_at,
                    source=excluded.source,
                    source_url=excluded.source_url,
                    canonical_url=excluded.canonical_url,
                    score=excluded.score,
                    reasons=excluded.reasons,
                    concerns=excluded.concerns,
                    verification_status=excluded.verification_status,
                    verification_source=excluded.verification_source,
                    last_seen_at=excluded.last_seen_at,
                    status=CASE
                        WHEN jobs.status IN ('new', 'reviewed')
                             AND excluded.verification_status='closed'
                        THEN 'closed'
                        ELSE jobs.status
                    END,
                    status_updated_at=CASE
                        WHEN jobs.status IN ('new', 'reviewed')
                             AND excluded.verification_status='closed'
                        THEN excluded.last_seen_at
                        ELSE jobs.status_updated_at
                    END
                """,
                (
                    job.fingerprint,
                    job.title,
                    job.company,
                    job.location,
                    job.remote,
                    job.employment_type,
                    job.salary_min,
                    job.salary_max,
                    job.currency,
                    job.salary_source,
                    job.description,
                    job.posted_at,
                    job.source,
                    job.source_url,
                    job.canonical_url,
                    job.score,
                    json.dumps(job.reasons, ensure_ascii=False),
                    json.dumps(job.concerns, ensure_ascii=False),
                    job.verification_status,
                    job.verification_source,
                    now,
                    now,
                    "closed" if job.verification_status == "closed" else "new",
                ),
            )
            count += 1
    return count


def list_jobs(
    path: Path, *, status: str | None = None, limit: int = 20
) -> list[sqlite3.Row]:
    with closing(connect(path)) as connection:
        if status:
            return connection.execute(
                "SELECT * FROM jobs WHERE status=? ORDER BY score DESC, last_seen_at DESC LIMIT ?",
                (status, limit),
            ).fetchall()
        return connection.execute(
            "SELECT * FROM jobs ORDER BY score DESC, last_seen_at DESC LIMIT ?", (limit,)
        ).fetchall()


def find_job(path: Path, identifier: str) -> sqlite3.Row:
    with closing(connect(path)) as connection:
        rows = connection.execute(
            "SELECT * FROM jobs WHERE fingerprint LIKE ?", (f"{identifier}%",)
        ).fetchall()
    if not rows:
        raise LookupError(f"No job matches ID {identifier!r}")
    if len(rows) > 1:
        raise LookupError(f"ID {identifier!r} is ambiguous; use more characters")
    return rows[0]


def mark_job(path: Path, identifier: str, status: str, note: str | None = None) -> None:
    if status not in VALID_STATUSES:
        raise ValueError(f"Invalid status: {status}")
    row = find_job(path, identifier)
    now = datetime.now(UTC).isoformat()
    with closing(connect(path)) as connection, connection:
        if note is None:
            connection.execute(
                "UPDATE jobs SET status=?, status_updated_at=? WHERE fingerprint=?",
                (status, now, row["fingerprint"]),
            )
        else:
            connection.execute(
                "UPDATE jobs SET status=?, status_updated_at=?, notes=? WHERE fingerprint=?",
                (status, now, note, row["fingerprint"]),
            )
