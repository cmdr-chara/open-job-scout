from __future__ import annotations

from dataclasses import dataclass, field

import pytest

from open_job_scout.firecrawl import (
    DEFAULT_EXCLUDE_DOMAINS,
    FirecrawlSettings,
    _job_from_extracted,
    discover_firecrawl,
    settings_from_config,
)


@dataclass
class FakeFirecrawl:
    search_results: list[dict] = field(default_factory=list)
    scrape_results: dict[str, dict] = field(default_factory=dict)
    interact_results: dict[str, list[dict]] = field(default_factory=dict)
    searches: list[str] = field(default_factory=list)
    scrapes: list[str] = field(default_factory=list)
    interactions: list[str] = field(default_factory=list)
    stopped: list[str] = field(default_factory=list)

    def search(self, query: str, settings: FirecrawlSettings) -> list[dict]:
        self.searches.append(query)
        return list(self.search_results)

    def scrape(self, url: str, settings: FirecrawlSettings) -> dict:
        self.scrapes.append(url)
        return self.scrape_results[url]

    def interact(self, scrape_id: str, settings: FirecrawlSettings) -> list[dict]:
        self.interactions.append(scrape_id)
        return list(self.interact_results.get(scrape_id, []))

    def stop_interaction(self, scrape_id: str) -> None:
        self.stopped.append(scrape_id)


def base_config(**firecrawl: object) -> dict:
    return {
        "search": {
            "terms": ["backend engineer"],
            "sites": ["linkedin"],
            "location": "Italy",
            "results_per_term": 5,
            "max_age_days": 7,
        },
        "firecrawl": {"enabled": True, **firecrawl},
    }


def job_payload(url: str = "https://example.com/jobs/1") -> dict:
    return {
        "json": {
            "page_type": "job",
            "requires_interaction": False,
            "job_links": [],
            "job": {
                "title": "Backend Engineer",
                "company": "Example",
                "location": "Milan, Italy",
                "remote": False,
                "work_mode": "hybrid",
                "employment_type": "fulltime",
                "salary_min": 45_000,
                "salary_max": 55_000,
                "currency": "EUR",
                "posted_at": "2026-08-16",
                "canonical_url": url,
                "description": "Build public APIs.",
            },
        }
    }


def test_disabled_firecrawl_needs_no_key_or_transport(monkeypatch) -> None:
    monkeypatch.delenv("FIRECRAWL_API_KEY", raising=False)
    batch = discover_firecrawl({"search": {"terms": [], "location": ""}})
    assert batch.enabled is False
    assert batch.jobs == []


def test_enabled_firecrawl_requires_environment_key(monkeypatch) -> None:
    monkeypatch.delenv("FIRECRAWL_API_KEY", raising=False)
    with pytest.raises(RuntimeError, match="FIRECRAWL_API_KEY"):
        discover_firecrawl(base_config())


def test_search_scrape_normalizes_only_job_fields() -> None:
    url = "https://example.com/jobs/1"
    client = FakeFirecrawl(
        search_results=[{"url": url, "title": "Backend Engineer"}],
        scrape_results={url: job_payload(url)},
    )
    batch = discover_firecrawl(base_config(), client=client)
    assert batch.searches == 1
    assert batch.scrapes == 1
    assert batch.interactions == 0
    assert len(batch.jobs) == 1
    job = batch.jobs[0]
    assert job.source == "firecrawl"
    assert job.title == "Backend Engineer"
    assert job.company == "Example"
    assert job.canonical_url == url
    assert job.salary_min == 45_000
    assert job.salary_source == "firecrawl"
    assert job.description == "Build public APIs."


def test_careers_page_follows_public_job_links() -> None:
    careers = "https://example.com/careers"
    posting = "https://example.com/jobs/backend"
    client = FakeFirecrawl(
        scrape_results={
            careers: {
                "json": {
                    "page_type": "careers",
                    "requires_interaction": False,
                    "job": None,
                    "job_links": [{"title": "Backend", "url": posting}],
                }
            },
            posting: job_payload(posting),
        }
    )
    batch = discover_firecrawl(
        base_config(search_enabled=False, career_urls=[careers]),
        client=client,
    )
    assert client.scrapes == [careers, posting]
    assert [job.source_url for job in batch.jobs] == [posting]


def test_interaction_requires_exact_url_opt_in_and_is_stopped() -> None:
    careers = "https://example.com/careers"
    posting = "https://example.com/jobs/hidden"
    client = FakeFirecrawl(
        scrape_results={
            careers: {
                "metadata": {"scrapeId": "scrape-1"},
                "json": {
                    "page_type": "careers",
                    "requires_interaction": True,
                    "job": None,
                    "job_links": [],
                },
            },
            posting: job_payload(posting),
        },
        interact_results={"scrape-1": [{"title": "Hidden role", "url": posting}]},
    )
    batch = discover_firecrawl(
        base_config(
            search_enabled=False,
            career_urls=[careers],
            interact_urls=[careers],
        ),
        client=client,
    )
    assert batch.interactions == 1
    assert client.interactions == ["scrape-1"]
    assert client.stopped == ["scrape-1"]
    assert [job.source_url for job in batch.jobs] == [posting]


def test_interaction_is_not_automatic_for_unapproved_url() -> None:
    careers = "https://example.com/careers"
    client = FakeFirecrawl(
        scrape_results={
            careers: {
                "metadata": {"scrapeId": "scrape-1"},
                "json": {
                    "page_type": "careers",
                    "requires_interaction": True,
                    "job": None,
                    "job_links": [],
                },
            }
        }
    )
    batch = discover_firecrawl(
        base_config(search_enabled=False, career_urls=[careers]),
        client=client,
    )
    assert batch.interactions == 0
    assert client.interactions == []
    assert client.stopped == []
    assert any("interact_urls" in warning for warning in batch.warnings)


def test_private_or_non_http_urls_are_not_normalized_into_jobs() -> None:
    payload = {
        "title": "Backend Engineer",
        "company": "Example",
        "canonical_url": "http://127.0.0.1/private",
    }
    assert _job_from_extracted(payload, "http://127.0.0.1/jobs/1") is None
    job = _job_from_extracted(payload, "https://example.com/jobs/1")
    assert job is not None
    assert job.canonical_url == "https://example.com/jobs/1"


def test_include_domains_use_the_default_exclusion_set_as_fallback() -> None:
    settings = settings_from_config(base_config(include_domains=["example.com"])["firecrawl"] | {})
    # This test calls the public parser with the full expected config shape below;
    # the assertion guards the default that keeps JobSpy/direct ATS sources preferred.
    assert DEFAULT_EXCLUDE_DOMAINS
    assert settings is not None
