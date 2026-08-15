from __future__ import annotations

import json
import os
import sqlite3
from collections.abc import Iterable
from contextlib import closing
from datetime import UTC, datetime, timedelta
from pathlib import Path

from .models import Job, job_fingerprint, normalize_text

VALID_STATUSES = {
    "new",
    "reviewed",
    "applied",
    "interview",
    "rejected",
    "offer",
    "closed",
    "stale",
}
SCHEMA_VERSION = 3

SCHEMA = """
CREATE TABLE IF NOT EXISTS jobs (
    fingerprint TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    company TEXT NOT NULL,
    location TEXT,
    remote INTEGER,
    work_mode TEXT NOT NULL DEFAULT 'unknown',
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
    replacement_url TEXT,
    replacement_title TEXT,
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'new',
    status_updated_at TEXT,
    status_manually_set INTEGER NOT NULL DEFAULT 0,
    notes TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_jobs_status_score ON jobs(status, score DESC);
CREATE INDEX IF NOT EXISTS idx_jobs_last_seen ON jobs(last_seen_at DESC);
CREATE TABLE IF NOT EXISTS job_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_fingerprint TEXT NOT NULL,
    event_type TEXT NOT NULL,
    old_value TEXT,
    new_value TEXT,
    note TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY(job_fingerprint) REFERENCES jobs(fingerprint)
        ON UPDATE CASCADE ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_job_events_job_created
ON job_events(job_fingerprint, created_at DESC, id DESC);
"""


def _record_event(
    connection: sqlite3.Connection,
    fingerprint: str,
    event_type: str,
    *,
    old_value: str | None = None,
    new_value: str | None = None,
    note: str | None = None,
    created_at: str | None = None,
) -> None:
    connection.execute(
        """
        INSERT INTO job_events (
            job_fingerprint, event_type, old_value, new_value, note, created_at
        ) VALUES (?, ?, ?, ?, ?, ?)
        """,
        (
            fingerprint,
            event_type,
            old_value,
            new_value,
            note,
            created_at or datetime.now(UTC).isoformat(),
        ),
    )


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
            f"UPDATE job_events SET job_fingerprint=? "
            f"WHERE job_fingerprint IN ({placeholders})",
            (stable, *obsolete),
        )
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


def _migrate_schema_v2(connection: sqlite3.Connection) -> None:
    columns = {row["name"] for row in connection.execute("PRAGMA table_info(jobs)")}
    additions = {
        "work_mode": "TEXT NOT NULL DEFAULT 'unknown'",
        "replacement_url": "TEXT",
        "replacement_title": "TEXT",
        "status_manually_set": "INTEGER NOT NULL DEFAULT 0",
    }
    for name, definition in additions.items():
        if name not in columns:
            connection.execute(f"ALTER TABLE jobs ADD COLUMN {name} {definition}")
    connection.execute(
        "UPDATE jobs SET status_manually_set=1 "
        "WHERE status IN ('reviewed','applied','interview','rejected','offer')"
    )


def _migrate_schema_v3(connection: sqlite3.Connection) -> None:
    connection.execute(
        """
        INSERT INTO job_events (job_fingerprint, event_type, new_value, note, created_at)
        SELECT
            fingerprint,
            'snapshot',
            status,
            'State recorded during history migration',
            COALESCE(status_updated_at, first_seen_at)
        FROM jobs
        WHERE NOT EXISTS (
            SELECT 1 FROM job_events WHERE job_events.job_fingerprint=jobs.fingerprint
        )
        """
    )


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
                connection.execute("PRAGMA user_version = 1")
            version = 1
        if version < 2:
            with connection:
                _migrate_schema_v2(connection)
                connection.execute("PRAGMA user_version = 2")
            version = 2
        if version < 3:
            with connection:
                _migrate_schema_v3(connection)
                connection.execute("PRAGMA user_version = 3")
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
            before = connection.execute(
                "SELECT * FROM jobs WHERE fingerprint=?", (job.fingerprint,)
            ).fetchone()
            connection.execute(
                """
                INSERT INTO jobs (
                    fingerprint,title,company,location,remote,work_mode,employment_type,
                    salary_min,salary_max,currency,salary_source,description,posted_at,
                    source,source_url,canonical_url,score,reasons,concerns,
                    verification_status,verification_source,replacement_url,replacement_title,
                    first_seen_at,last_seen_at,status,status_manually_set
                ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
                ON CONFLICT(fingerprint) DO UPDATE SET
                    title=excluded.title,
                    company=excluded.company,
                    location=excluded.location,
                    remote=excluded.remote,
                    work_mode=excluded.work_mode,
                    employment_type=excluded.employment_type,
                    salary_min=COALESCE(excluded.salary_min,jobs.salary_min),
                    salary_max=COALESCE(excluded.salary_max,jobs.salary_max),
                    currency=COALESCE(excluded.currency,jobs.currency),
                    salary_source=COALESCE(excluded.salary_source,jobs.salary_source),
                    description=excluded.description,
                    posted_at=excluded.posted_at,
                    source=excluded.source,
                    source_url=excluded.source_url,
                    canonical_url=COALESCE(excluded.canonical_url,jobs.canonical_url),
                    score=excluded.score,
                    reasons=excluded.reasons,
                    concerns=excluded.concerns,
                    verification_status=excluded.verification_status,
                    verification_source=excluded.verification_source,
                    replacement_url=excluded.replacement_url,
                    replacement_title=excluded.replacement_title,
                    last_seen_at=excluded.last_seen_at,
                    status=CASE
                        WHEN jobs.status_manually_set=1 THEN jobs.status
                        WHEN excluded.verification_status='closed' THEN 'closed'
                        WHEN jobs.status IN ('closed','stale') THEN 'new'
                        ELSE jobs.status
                    END,
                    status_updated_at=CASE
                        WHEN jobs.status_manually_set=1 THEN jobs.status_updated_at
                        WHEN excluded.verification_status='closed' AND jobs.status<>'closed'
                        THEN excluded.last_seen_at
                        WHEN excluded.verification_status<>'closed'
                             AND jobs.status IN ('closed','stale')
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
                    job.work_mode,
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
                    job.replacement_url,
                    job.replacement_title,
                    now,
                    now,
                    "closed" if job.verification_status == "closed" else "new",
                    0,
                ),
            )
            after = connection.execute(
                "SELECT * FROM jobs WHERE fingerprint=?", (job.fingerprint,)
            ).fetchone()
            if before is None:
                _record_event(
                    connection,
                    job.fingerprint,
                    "discovered",
                    new_value=after["status"],
                    note=f"source={job.source}; verification={job.verification_status}",
                    created_at=now,
                )
            else:
                if before["verification_status"] != after["verification_status"]:
                    _record_event(
                        connection,
                        job.fingerprint,
                        "verification",
                        old_value=before["verification_status"],
                        new_value=after["verification_status"],
                        created_at=now,
                    )
                if before["status"] != after["status"]:
                    _record_event(
                        connection,
                        job.fingerprint,
                        "status",
                        old_value=before["status"],
                        new_value=after["status"],
                        note="automatic discovery refresh",
                        created_at=now,
                    )
            count += 1
    return count


def refresh_jobs(jobs: Iterable[Job], path: Path) -> int:
    """Update verification/ranking metadata without pretending a job was rediscovered."""
    now = datetime.now(UTC).isoformat()
    count = 0
    with closing(connect(path)) as connection, connection:
        for job in jobs:
            before = connection.execute(
                "SELECT * FROM jobs WHERE fingerprint=?", (job.fingerprint,)
            ).fetchone()
            if before is None:
                raise LookupError(f"Tracked job disappeared during recheck: {job.fingerprint[:10]}")

            next_status = before["status"]
            if not before["status_manually_set"]:
                if job.verification_status == "closed":
                    next_status = "closed"
                elif before["status"] == "closed":
                    next_status = "new"
            status_updated_at = (
                now if next_status != before["status"] else before["status_updated_at"]
            )
            connection.execute(
                """
                UPDATE jobs SET
                    remote=?,
                    work_mode=?,
                    salary_min=COALESCE(?, salary_min),
                    salary_max=COALESCE(?, salary_max),
                    currency=COALESCE(?, currency),
                    salary_source=COALESCE(?, salary_source),
                    description=?,
                    posted_at=?,
                    canonical_url=COALESCE(?, canonical_url),
                    score=?,
                    reasons=?,
                    concerns=?,
                    verification_status=?,
                    verification_source=?,
                    replacement_url=?,
                    replacement_title=?,
                    status=?,
                    status_updated_at=?
                WHERE fingerprint=?
                """,
                (
                    job.remote,
                    job.work_mode,
                    job.salary_min,
                    job.salary_max,
                    job.currency,
                    job.salary_source,
                    job.description,
                    job.posted_at,
                    job.canonical_url,
                    job.score,
                    json.dumps(job.reasons, ensure_ascii=False),
                    json.dumps(job.concerns, ensure_ascii=False),
                    job.verification_status,
                    job.verification_source,
                    job.replacement_url,
                    job.replacement_title,
                    next_status,
                    status_updated_at,
                    job.fingerprint,
                ),
            )
            if before["verification_status"] != job.verification_status:
                _record_event(
                    connection,
                    job.fingerprint,
                    "verification",
                    old_value=before["verification_status"],
                    new_value=job.verification_status,
                    note="manual recheck",
                    created_at=now,
                )
            if before["status"] != next_status:
                _record_event(
                    connection,
                    job.fingerprint,
                    "status",
                    old_value=before["status"],
                    new_value=next_status,
                    note="automatic verification recheck",
                    created_at=now,
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


def list_job_events(path: Path, identifier: str, limit: int = 50) -> list[sqlite3.Row]:
    if limit < 1:
        raise ValueError("Limit must be at least 1.")
    row = find_job(path, identifier)
    with closing(connect(path)) as connection:
        return connection.execute(
            """
            SELECT id, event_type, old_value, new_value, note, created_at
            FROM job_events
            WHERE job_fingerprint=?
            ORDER BY created_at DESC, id DESC
            LIMIT ?
            """,
            (row["fingerprint"], limit),
        ).fetchall()


def mark_stale_jobs(path: Path, stale_after_days: int = 30) -> int:
    cutoff = (datetime.now(UTC) - timedelta(days=stale_after_days)).isoformat()
    now = datetime.now(UTC).isoformat()
    with closing(connect(path)) as connection, connection:
        candidates = connection.execute(
            """
            SELECT fingerprint, status FROM jobs
            WHERE status_manually_set=0
              AND status IN ('new','reviewed')
              AND last_seen_at < ?
            """,
            (cutoff,),
        ).fetchall()
        if not candidates:
            return 0
        connection.executemany(
            "UPDATE jobs SET status='stale', status_updated_at=? WHERE fingerprint=?",
            ((now, row["fingerprint"]) for row in candidates),
        )
        for row in candidates:
            _record_event(
                connection,
                row["fingerprint"],
                "status",
                old_value=row["status"],
                new_value="stale",
                note="not seen in configured discovery window",
                created_at=now,
            )
        return len(candidates)


def mark_job(path: Path, identifier: str, status: str, note: str | None = None) -> None:
    if status not in VALID_STATUSES:
        raise ValueError(f"Invalid status: {status}")
    row = find_job(path, identifier)
    now = datetime.now(UTC).isoformat()
    with closing(connect(path)) as connection, connection:
        if note is None:
            connection.execute(
                """
                UPDATE jobs
                SET status=?, status_updated_at=?, status_manually_set=1
                WHERE fingerprint=?
                """,
                (status, now, row["fingerprint"]),
            )
        else:
            connection.execute(
                """
                UPDATE jobs
                SET status=?, status_updated_at=?, status_manually_set=1,
                    notes=CASE
                        WHEN notes='' THEN ?
                        WHEN notes=? OR notes LIKE '%' || char(10) || ? THEN notes
                        ELSE notes || char(10) || '[' || ? || '] ' || ?
                    END
                WHERE fingerprint=?
                """,
                (status, now, note, note, note, now, note, row["fingerprint"]),
            )
        if row["status"] != status:
            _record_event(
                connection,
                row["fingerprint"],
                "status",
                old_value=row["status"],
                new_value=status,
                note=note,
                created_at=now,
            )
        elif note:
            _record_event(
                connection,
                row["fingerprint"],
                "note",
                note=note,
                created_at=now,
            )
