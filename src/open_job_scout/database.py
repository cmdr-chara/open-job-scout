from __future__ import annotations

import json
import os
import sqlite3
from collections.abc import Iterable
from contextlib import closing
from datetime import UTC, datetime
from pathlib import Path

from .models import Job, job_fingerprint, normalize_text

VALID_STATUSES = {"new", "reviewed", "applied", "interview", "rejected", "offer", "closed"}
SCHEMA_VERSION = 1

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


def _merge_identity_rows(
    connection: sqlite3.Connection, stable: str, rows: Iterable[sqlite3.Row]
) -> None:
    candidates = list({row["fingerprint"]: row for row in rows}.values())
    if not candidates:
        return
    keeper = next(
        (row for row in candidates if row["fingerprint"] == stable),
        candidates[0],
    )
    if keeper["fingerprint"] != stable:
        connection.execute(
            "UPDATE jobs SET fingerprint=? WHERE fingerprint=?",
            (stable, keeper["fingerprint"]),
        )

    tracking = max(
        candidates,
        key=lambda item: (
            item["status_updated_at"] or "",
            item["status"] != "new",
            item["last_seen_at"],
        ),
    )
    notes = "\n".join(
        dict.fromkeys(item["notes"].strip() for item in candidates if item["notes"].strip())
    )
    connection.execute(
        """
        UPDATE jobs
        SET status=?, status_updated_at=?, notes=?, first_seen_at=?, last_seen_at=?
        WHERE fingerprint=?
        """,
        (
            tracking["status"],
            tracking["status_updated_at"],
            notes,
            min(item["first_seen_at"] for item in candidates),
            max(item["last_seen_at"] for item in candidates),
            stable,
        ),
    )
    obsolete = [
        row["fingerprint"]
        for row in candidates
        if row["fingerprint"] not in {stable, keeper["fingerprint"]}
    ]
    if obsolete:
        placeholders = ",".join("?" for _ in obsolete)
        connection.execute(
            f"DELETE FROM jobs WHERE fingerprint IN ({placeholders})",
            obsolete,
        )


def _migrate_fingerprints(connection: sqlite3.Connection) -> None:
    """Move pre-0.1 identities away from mutable canonical URLs."""
    for snapshot in connection.execute("SELECT * FROM jobs").fetchall():
        row = connection.execute(
            "SELECT * FROM jobs WHERE fingerprint=?", (snapshot["fingerprint"],)
        ).fetchone()
        if row is None:
            continue
        stable = job_fingerprint(row["company"], row["title"], row["source_url"])
        if stable == row["fingerprint"]:
            continue
        existing = connection.execute(
            "SELECT * FROM jobs WHERE fingerprint=?", (stable,)
        ).fetchone()
        _merge_identity_rows(
            connection,
            stable,
            (row,) if existing is None else (existing, row),
        )


def _reconcile_legacy_job(connection: sqlite3.Connection, job: Job) -> None:
    """Merge v0.1 rows whose source URL was actually the direct employer URL."""
    urls = list(
        dict.fromkeys(value for value in (job.original_canonical_url, job.canonical_url) if value)
    )
    if not urls:
        return
    placeholders = ",".join("?" for _ in urls)
    matches = connection.execute(
        f"""
        SELECT * FROM jobs
        WHERE canonical_url IN ({placeholders}) OR source_url IN ({placeholders})
        """,
        (*urls, *urls),
    ).fetchall()
    matches = [
        row
        for row in matches
        if normalize_text(row["company"]) == normalize_text(job.company)
        and normalize_text(row["title"]) == normalize_text(job.title)
    ]
    current = connection.execute(
        "SELECT * FROM jobs WHERE fingerprint=?", (job.fingerprint,)
    ).fetchone()
    if current is not None:
        matches.append(current)
    if any(row["fingerprint"] != job.fingerprint for row in matches):
        _merge_identity_rows(connection, job.fingerprint, matches)


def connect(path: Path) -> sqlite3.Connection:
    path.parent.mkdir(parents=True, exist_ok=True)
    existed = path.exists()
    connection = sqlite3.connect(path, timeout=5.0)
    try:
        connection.row_factory = sqlite3.Row
        connection.execute("PRAGMA busy_timeout = 5000")
        connection.execute("PRAGMA foreign_keys = ON")
        connection.execute("PRAGMA journal_mode = WAL")
        connection.executescript(SCHEMA)
        version = connection.execute("PRAGMA user_version").fetchone()[0]
        if version > SCHEMA_VERSION:
            raise RuntimeError(
                f"Database schema {version} is newer than supported schema {SCHEMA_VERSION}."
            )
        if version < 1:
            with connection:
                _migrate_fingerprints(connection)
                connection.execute(f"PRAGMA user_version = {SCHEMA_VERSION}")
        if not existed and os.name != "nt":
            path.chmod(0o600)
    except Exception:
        connection.close()
        raise
    return connection


def save_jobs(jobs: Iterable[Job], path: Path) -> int:
    now = datetime.now(UTC).isoformat()
    count = 0
    with closing(connect(path)) as connection, connection:
        for job in jobs:
            _reconcile_legacy_job(connection, job)
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


def list_jobs(path: Path, *, status: str | None = None, limit: int = 20) -> list[sqlite3.Row]:
    with closing(connect(path)) as connection:
        if status:
            return connection.execute(
                "SELECT * FROM jobs WHERE status=? ORDER BY score DESC, last_seen_at DESC LIMIT ?",
                (status, limit),
            ).fetchall()
        return connection.execute(
            "SELECT * FROM jobs ORDER BY score DESC, last_seen_at DESC LIMIT ?", (limit,)
        ).fetchall()


def get_jobs_by_fingerprints(path: Path, fingerprints: Iterable[str]) -> list[sqlite3.Row]:
    identifiers = list(dict.fromkeys(fingerprints))
    if not identifiers:
        return []
    placeholders = ",".join("?" for _ in identifiers)
    with closing(connect(path)) as connection:
        return connection.execute(
            f"""
            SELECT * FROM jobs
            WHERE fingerprint IN ({placeholders})
            ORDER BY score DESC, last_seen_at DESC
            """,
            identifiers,
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
