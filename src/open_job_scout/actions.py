from __future__ import annotations

from contextlib import closing
from datetime import UTC, datetime
from pathlib import Path

from .database import connect, find_job


def add_note(path: Path, identifier: str, note: str) -> bool:
    """Append a note without changing application status or manual-state ownership."""
    text = note.strip()
    if not text:
        raise ValueError("Note must not be blank.")

    row = find_job(path, identifier)
    now = datetime.now(UTC).isoformat()
    existing = str(row["notes"] or "").strip()
    if existing == text or existing.endswith(f"] {text}"):
        return False

    updated = text if not existing else f"{existing}\n[{now}] {text}"
    with closing(connect(path)) as connection, connection:
        connection.execute(
            "UPDATE jobs SET notes=? WHERE fingerprint=?",
            (updated, row["fingerprint"]),
        )
        connection.execute(
            """
            INSERT INTO job_events (
                job_fingerprint, event_type, note, created_at
            ) VALUES (?, 'note', ?, ?)
            """,
            (row["fingerprint"], text, now),
        )
    return True
