from open_job_scout.discovery import deduplicate, row_to_job


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
