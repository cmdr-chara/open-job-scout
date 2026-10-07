import json

from open_job_scout.capture import capture_job


def test_capture_job_extracts_jsonld_and_preserves_public_snapshot(monkeypatch) -> None:
    page = {
        "@context": "https://schema.org",
        "@type": "JobPosting",
        "title": "Backend Engineer",
        "description": "Build reliable services.",
        "datePosted": "2026-10-01",
        "employmentType": ["FULL_TIME"],
        "hiringOrganization": {"name": "Example Labs"},
        "jobLocation": {"address": {"addressLocality": "Milan", "addressCountry": "IT"}},
        "jobLocationType": "TELECOMMUTE",
        "baseSalary": {
            "currency": "EUR",
            "value": {"minValue": 50000, "maxValue": 65000},
        },
    }
    html = f"""
    <html><head><title>Example Labs - Backend Engineer</title>
    <link rel="canonical" href="/jobs/backend-engineer">
    <script type="application/ld+json">{json.dumps(page, indent=2)}</script></head>
    <body><p>Build reliable services.</p></body></html>
    """
    monkeypatch.setattr("open_job_scout.capture.is_safe_public_url", lambda _url: True)
    monkeypatch.setattr(
        "open_job_scout.capture.resolve_url",
        lambda _url: ("reachable", "https://jobs.example.test/backend", html),
    )

    job = capture_job("https://jobs.example.test/backend?utm_source=newsletter")

    assert job.title == "Backend Engineer"
    assert job.company == "Example Labs"
    assert job.canonical_url == "https://jobs.example.test/jobs/backend-engineer"
    assert job.source_url == "https://jobs.example.test/backend"
    assert job.location == "Milan, IT"
    assert job.remote is True
    assert job.employment_type == "FULL_TIME"
    assert job.salary_min == 50000
    assert job.salary_max == 65000
    assert job.currency == "EUR"
    assert job.description == "Build reliable services."
    assert job.verification_status == "verified"


def test_capture_job_reads_multiple_jsonld_blocks(monkeypatch) -> None:
    page = {
        "@context": "https://schema.org",
        "@type": "JobPosting",
        "title": "Platform Engineer",
        "hiringOrganization": {"name": "Acme"},
    }
    html = (
        "<script type='application/ld+json'>{}</script>"
        "<script type='application/ld+json'>{}</script>"
    ).format(json.dumps({"@type": "BreadcrumbList"}, indent=2), json.dumps(page, indent=2))
    monkeypatch.setattr("open_job_scout.capture.is_safe_public_url", lambda _url: True)
    monkeypatch.setattr(
        "open_job_scout.capture.resolve_url",
        lambda _url: ("reachable", "https://jobs.example.test/platform", html),
    )

    job = capture_job("  https://jobs.example.test/platform  ")

    assert job.title == "Platform Engineer"
    assert job.company == "Acme"
    assert job.source_url == "https://jobs.example.test/platform"


def test_capture_job_accepts_scalar_salary_value(monkeypatch) -> None:
    page = {
        "@context": "https://schema.org",
        "@type": "JobPosting",
        "title": "Support Engineer",
        "hiringOrganization": {"name": "Acme"},
        "baseSalary": {"currency": "USD", "value": 50000},
    }
    html = f'<script type="application/ld+json">{json.dumps(page)}</script>'
    monkeypatch.setattr("open_job_scout.capture.is_safe_public_url", lambda _url: True)
    monkeypatch.setattr(
        "open_job_scout.capture.resolve_url",
        lambda _url: ("reachable", "https://jobs.example.test/support", html),
    )

    job = capture_job("https://jobs.example.test/support?utm_campaign=launch")

    assert job.salary_min == 50000
    assert job.salary_max == 50000
    assert job.currency == "USD"
    assert job.source_url == "https://jobs.example.test/support"


def test_capture_job_requires_a_title_when_metadata_is_missing(monkeypatch) -> None:
    monkeypatch.setattr("open_job_scout.capture.is_safe_public_url", lambda _url: True)
    monkeypatch.setattr(
        "open_job_scout.capture.resolve_url",
        lambda _url: ("reachable", "https://jobs.example.test/blank", "<html></html>"),
    )

    try:
        capture_job("https://jobs.example.test/blank")
    except ValueError as error:
        assert "job title" in str(error)
    else:
        raise AssertionError("capture should reject pages without a title")
