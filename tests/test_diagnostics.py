from pathlib import Path

from open_job_scout import diagnostics
from open_job_scout.database import save_jobs
from open_job_scout.models import Job


def settings_for(tmp_path: Path) -> dict:
    return {
        "search": {
            "terms": ["python"],
            "sites": ["linkedin"],
            "location": "Italy",
            "country_indeed": "Italy",
            "results_per_term": 10,
            "max_age_days": 7,
        },
        "filters": {
            "require_remote": False,
            "allowed_employment_types": [],
            "blocked_title_terms": [],
            "blocked_description_terms": [],
            "max_required_years": 99,
        },
        "ranking": {
            "preferred_title_terms": [],
            "preferred_skills": [],
            "junior_signals": [],
            "concern_signals": [],
        },
        "storage": {
            "database": str(tmp_path / "jobs.sqlite3"),
            "report_directory": str(tmp_path / "reports"),
        },
    }


def test_doctor_reports_healthy_current_database(tmp_path: Path, monkeypatch) -> None:
    config = tmp_path / "config.toml"
    config.write_text("placeholder", encoding="utf-8")
    settings = settings_for(tmp_path)
    save_jobs(
        [Job(title="Backend Engineer", company="Example", source_url="https://example.test/1")],
        Path(settings["storage"]["database"]),
    )
    monkeypatch.setattr(diagnostics, "load_config", lambda path: settings)
    monkeypatch.setattr(diagnostics.importlib.util, "find_spec", lambda name: object())

    checks = diagnostics.run_diagnostics(config)

    assert not [check for check in checks if check.level == "error"]
    assert any(check.check == "database schema" and check.level == "ok" for check in checks)
    assert any(check.check == "database integrity" and check.message == "ok" for check in checks)


def test_doctor_flags_disabled_indeed(tmp_path: Path, monkeypatch) -> None:
    config = tmp_path / "config.toml"
    config.write_text("placeholder", encoding="utf-8")
    settings = settings_for(tmp_path)
    settings["search"]["sites"] = ["indeed"]
    monkeypatch.setattr(diagnostics, "load_config", lambda path: settings)
    monkeypatch.setattr(diagnostics.importlib.util, "find_spec", lambda name: object())

    checks = diagnostics.run_diagnostics(config)

    sources = next(check for check in checks if check.check == "sources")
    assert sources.level == "error"
    assert "disabled" in sources.message


def test_doctor_reports_missing_config(tmp_path: Path) -> None:
    checks = diagnostics.run_diagnostics(tmp_path / "missing.toml")
    assert len(checks) == 1
    assert checks[0].level == "error"
    assert checks[0].check == "configuration"
