from open_job_scout.presentation import format_job_detail, preferred_job_url


def sample_row() -> dict:
    return {
        "fingerprint": "abcdef1234567890",
        "title": "Junior Backend Engineer",
        "company": "Example Labs",
        "score": 82.5,
        "status": "new",
        "work_mode": "remote",
        "verification_status": "verified",
        "location": "Italy",
        "employment_type": "fulltime",
        "salary_min": 45000,
        "salary_max": 55000,
        "currency": "EUR",
        "salary_source": "greenhouse",
        "posted_at": "2026-08-10",
        "source": "linkedin",
        "last_seen_at": "2026-08-15T20:00:00+00:00",
        "canonical_url": "https://careers.example/jobs/1",
        "source_url": "https://linkedin.example/jobs/1",
        "reasons": '["title: backend", "skills: python"]',
        "concerns": '["salary below target"]',
        "notes": "Review company first",
        "replacement_url": None,
        "replacement_title": None,
        "description": "Build Python services and APIs. " * 60,
    }


def test_preferred_url_uses_canonical_then_source() -> None:
    row = sample_row()
    assert preferred_job_url(row) == "https://careers.example/jobs/1"
    assert preferred_job_url(row, source=True) == "https://linkedin.example/jobs/1"

    row["canonical_url"] = None
    assert preferred_job_url(row) == "https://linkedin.example/jobs/1"


def test_human_detail_contains_review_information() -> None:
    rendered = format_job_detail(sample_row())
    assert "Junior Backend Engineer — Example Labs" in rendered
    assert "82.5/100" in rendered
    assert "45,000 - 55,000 EUR" in rendered
    assert "Why it ranked:" in rendered
    assert "skills: python" in rendered
    assert "salary below target" in rendered
    assert "Review company first" in rendered
    assert "jobscout open abcdef1234" in rendered
    assert "jobscout show abcdef1234 --full" in rendered
