import math
import sys
from types import SimpleNamespace

import pytest

from open_job_scout.discovery import _plain_description, deduplicate, discover, row_to_job
from open_job_scout.models import Job


def test_jobspy_row_mapping() -> None:
    job = row_to_job(
        {
            "title": "Software Engineer",
            "company": "Example",
            "job_url": "https://example.test/job",
            "is_remote": True,
            "min_amount": 40_000,
            "max_amount": 50_000,
            "currency": "EUR",
            "salary_source": "description",
        }
    )
    assert job.remote is True
    assert job.salary_min == 40_000
    assert job.salary_source == "description"


def test_mapping_preserves_source_listing_and_direct_url() -> None:
    job = row_to_job(
        {
            "title": "Software Engineer",
            "company": "Example",
            "job_url": "https://linkedin.example/jobs/1",
            "job_url_direct": "https://careers.example/jobs/1",
        }
    )
    assert job.source_url == "https://linkedin.example/jobs/1"
    assert job.canonical_url == "https://careers.example/jobs/1"


def test_mapping_rejects_nan_and_non_http_url_values() -> None:
    job = row_to_job(
        {
            "title": "Software Engineer",
            "company": "Example",
            "job_url": "nan",
            "job_url_direct": "javascript:alert(1)",
        }
    )
    assert job.source_url == ""
    assert job.canonical_url is None


def test_mapping_treats_other_nan_fields_as_missing() -> None:
    job = row_to_job(
        {
            "title": float("nan"),
            "company": "Example",
            "job_url": "https://example.test/job",
            "min_amount": float("nan"),
            "salary_min": 40_000,
        }
    )
    assert job.title == ""
    assert job.salary_min == 40_000
    assert not math.isnan(job.salary_min)


def test_html_description_is_converted_without_script_content() -> None:
    html = "<h2>Build APIs</h2><p>Python &amp; Go</p><script>secret()</script>"
    assert _plain_description(html) == "Build APIs Python & Go"


def test_deduplicate_prefers_richer_description() -> None:
    first = row_to_job(
        {"title": "Engineer", "company": "Example", "job_url": "https://example.test/job"}
    )
    second = row_to_job(
        {
            "title": "Engineer",
            "company": "Example",
            "job_url": "https://example.test/job",
            "description": "A much richer description",
        }
    )
    assert deduplicate([first, second])[0].description == "A much richer description"


def test_discovery_requests_html_descriptions(monkeypatch) -> None:
    captured: dict = {}

    class Frame:
        @staticmethod
        def to_dict(*, orient: str) -> list[dict]:
            assert orient == "records"
            return []

    def fake_scrape_jobs(**kwargs):
        captured.update(kwargs)
        return Frame()

    monkeypatch.setitem(
        sys.modules,
        "jobspy",
        SimpleNamespace(scrape_jobs=fake_scrape_jobs),
    )
    settings = {
        "search": {
            "terms": ["backend engineer"],
            "sites": ["linkedin"],
            "location": "Italy",
            "country_indeed": "Italy",
            "results_per_term": 5,
            "max_age_days": 7,
        }
    }

    assert discover(settings) == []
    assert captured["description_format"] == "html"


def test_discovery_fails_clearly_when_every_search_errors(monkeypatch) -> None:
    def fail(**_kwargs):
        raise TimeoutError("source timed out")

    monkeypatch.setitem(sys.modules, "jobspy", SimpleNamespace(scrape_jobs=fail))
    settings = {
        "search": {
            "terms": ["backend engineer"],
            "sites": ["linkedin"],
            "location": "Italy",
            "results_per_term": 5,
            "max_age_days": 7,
        }
    }

    with pytest.raises(RuntimeError, match="All 1 configured searches failed"):
        discover(settings)


def test_indeed_is_disabled_before_loading_jobspy() -> None:
    settings = {
        "search": {
            "terms": ["backend engineer"],
            "sites": ["indeed"],
        }
    }
    with pytest.raises(RuntimeError, match="Indeed source is temporarily disabled"):
        discover(settings)


def test_deduplicate_ignores_tracking_parameters() -> None:
    first = Job(
        title="Backend Engineer",
        company="Example",
        source_url="https://board.example/jobs/1?utm_source=linkedin",
        canonical_url="https://careers.example/jobs/1?utm_campaign=social",
    )
    second = Job(
        title=first.title,
        company=first.company,
        source_url="https://board.example/jobs/1",
        canonical_url="https://careers.example/jobs/1",
        description="Richer official description",
    )
    result = deduplicate([first, second])
    assert len(result) == 1
    assert result[0].description == "Richer official description"


def test_discovery_isolates_source_failures(monkeypatch) -> None:
    calls: list[str] = []

    class Frame:
        @staticmethod
        def to_dict(*, orient: str) -> list[dict]:
            assert orient == "records"
            return [{"title": "Engineer", "company": "Example", "job_url": "https://x.test/1"}]

    def fake_scrape_jobs(**kwargs):
        site = kwargs["site_name"][0]
        calls.append(site)
        if site == "linkedin":
            raise TimeoutError("source timed out")
        return Frame()

    monkeypatch.setitem(sys.modules, "jobspy", SimpleNamespace(scrape_jobs=fake_scrape_jobs))
    settings = {
        "search": {
            "terms": ["backend engineer"],
            "sites": ["linkedin", "google"],
            "location": "Italy",
            "results_per_term": 5,
            "max_age_days": 7,
        }
    }

    assert len(discover(settings)) == 1
    assert calls == ["linkedin", "google"]
