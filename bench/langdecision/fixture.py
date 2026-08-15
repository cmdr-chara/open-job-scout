from __future__ import annotations

import json
import sqlite3
from pathlib import Path

ROOT = Path(__file__).resolve().parent
COUNT = 10_000


def build_jobs() -> list[dict]:
    titles = [
        "Junior Backend Engineer",
        "Python Developer",
        "Platform Engineer",
        "Graduate Software Engineer",
        "API Developer",
        "Software Engineer",
    ]
    companies = ["Acme", "Example Labs", "Contoso", "Northstar", "Globex"]
    modes = ["remote", "hybrid", "onsite", "remote"]
    statuses = ["new", "reviewed", "applied", "new", "new", "interview"]
    skills = ["python", "fastapi", "postgresql", "docker", "aws", "rest"]
    jobs = []
    for index in range(COUNT):
        title = titles[index % len(titles)]
        company = companies[index % len(companies)]
        mode = modes[index % len(modes)]
        status = statuses[index % len(statuses)]
        skill = skills[index % len(skills)]
        score = float((index * 37) % 101)
        jobs.append(
            {
                "id": f"job-{index:05d}",
                "title": title,
                "company": company,
                "location": "Italy" if index % 3 else "Remote - Europe",
                "work_mode": mode,
                "status": status,
                "source": "google" if index % 2 else "linkedin",
                "score": score,
                "last_seen_at": f"2026-08-{1 + (index % 15):02d}T12:{index % 60:02d}:00Z",
                "description": (
                    f"{title} at {company}. We are looking for {skill}, teamwork, APIs, "
                    "testing, cloud infrastructure, and pragmatic software engineering. "
                    "This role supports junior engineers and includes mentorship. "
                    * 3
                ),
            }
        )
    return jobs


def write_json(jobs: list[dict]) -> None:
    (ROOT / "jobs.json").write_text(json.dumps(jobs, separators=(",", ":")), encoding="utf-8")


def write_html(jobs: list[dict]) -> None:
    cards = []
    for job in jobs[:1_000]:
        cards.append(
            '<article class="job">'
            f'<h2 class="title">{job["title"]}</h2>'
            f'<span class="company">{job["company"]}</span>'
            f'<span class="score">{job["score"]}</span>'
            f'<p class="description">{job["description"]}</p>'
            "</article>"
        )
    (ROOT / "jobs.html").write_text("<main>" + "".join(cards) + "</main>", encoding="utf-8")


def write_sqlite(jobs: list[dict]) -> None:
    path = ROOT / "jobs.sqlite3"
    if path.exists():
        path.unlink()
    with sqlite3.connect(path) as connection:
        connection.executescript(
            """
            PRAGMA journal_mode=OFF;
            PRAGMA synchronous=OFF;
            CREATE TABLE jobs (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                company TEXT NOT NULL,
                location TEXT NOT NULL,
                work_mode TEXT NOT NULL,
                status TEXT NOT NULL,
                source TEXT NOT NULL,
                score REAL NOT NULL,
                last_seen_at TEXT NOT NULL,
                description TEXT NOT NULL
            );
            CREATE INDEX idx_jobs_status_score ON jobs(status, score DESC, last_seen_at DESC);
            """
        )
        connection.executemany(
            """
            INSERT INTO jobs (
                id,title,company,location,work_mode,status,source,score,last_seen_at,description
            ) VALUES (?,?,?,?,?,?,?,?,?,?)
            """,
            [
                (
                    job["id"],
                    job["title"],
                    job["company"],
                    job["location"],
                    job["work_mode"],
                    job["status"],
                    job["source"],
                    job["score"],
                    job["last_seen_at"],
                    job["description"],
                )
                for job in jobs
            ],
        )
        connection.commit()


if __name__ == "__main__":
    records = build_jobs()
    write_json(records)
    write_html(records)
    write_sqlite(records)
    print(f"generated {len(records)} jobs in {ROOT}")
