from __future__ import annotations

import json
from pathlib import Path

from open_job_scout.models import Job, job_fingerprint, normalize_job_url
from open_job_scout.ranking import classify_work_mode

VECTORS = json.loads(
    (Path(__file__).parent / "fixtures" / "runtime_contract_vectors.json").read_text(
        encoding="utf-8"
    )
)


def test_fingerprint_contract_vectors() -> None:
    for vector in VECTORS["fingerprints"]:
        assert (
            job_fingerprint(vector["company"], vector["title"], vector["source_url"])
            == vector["expected"]
        )


def test_url_normalization_contract_vectors() -> None:
    for vector in VECTORS["urls"]:
        assert normalize_job_url(vector["input"]) == vector["expected"]


def test_work_mode_contract_vectors() -> None:
    for vector in VECTORS["work_modes"]:
        job = Job(
            title=vector["title"],
            company="Contract Test",
            source_url="https://example.test/source",
            location=vector["location"],
            description=vector["description"],
            remote=vector["remote"],
            work_mode=vector["work_mode"],
        )
        assert classify_work_mode(job) == vector["expected"]
