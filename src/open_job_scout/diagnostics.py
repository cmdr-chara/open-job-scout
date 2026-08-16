from __future__ import annotations

import importlib.util
import os
import sqlite3
import stat
from dataclasses import asdict, dataclass
from pathlib import Path

from .config import DEFAULT_CONFIG, expand_path, load_config
from .database import SCHEMA_VERSION
from .firecrawl import settings_from_config


@dataclass(frozen=True, slots=True)
class Diagnostic:
    level: str
    check: str
    message: str

    def as_dict(self) -> dict[str, str]:
        return asdict(self)


def _permission_check(path: Path, label: str) -> Diagnostic | None:
    if os.name == "nt" or not path.exists():
        return None
    mode = stat.S_IMODE(path.stat().st_mode)
    if mode & 0o077:
        return Diagnostic(
            "warn",
            f"{label} permissions",
            f"{path} is mode {mode:04o}; consider restricting it to 0600.",
        )
    return Diagnostic("ok", f"{label} permissions", f"{path} is restricted to the current user.")


def _writable_parent(path: Path) -> Path | None:
    candidate = path if path.exists() and path.is_dir() else path.parent
    while not candidate.exists() and candidate != candidate.parent:
        candidate = candidate.parent
    return candidate if candidate.exists() and candidate.is_dir() else None


def run_diagnostics(config_path: Path | None = None) -> list[Diagnostic]:
    selected = (config_path or DEFAULT_CONFIG).expanduser().resolve()
    checks: list[Diagnostic] = []
    if not selected.exists():
        return [
            Diagnostic(
                "error",
                "configuration",
                f"Config not found: {selected}. Run `jobscout init` first.",
            )
        ]

    try:
        config = load_config(selected)
    except (OSError, ValueError) as exc:
        return [Diagnostic("error", "configuration", str(exc))]

    checks.append(Diagnostic("ok", "configuration", f"Valid config: {selected}"))
    permission = _permission_check(selected, "config")
    if permission:
        checks.append(permission)

    sites = [str(site).lower() for site in config["search"]["sites"]]
    if "indeed" in sites:
        checks.append(
            Diagnostic(
                "error",
                "sources",
                "Indeed is configured but currently disabled because the upstream adapter "
                "does not verify TLS certificates.",
            )
        )
    else:
        checks.append(Diagnostic("ok", "sources", f"Configured sources: {', '.join(sites)}"))

    firecrawl = settings_from_config(config)
    if not firecrawl.enabled:
        checks.append(
            Diagnostic("ok", "Firecrawl", "Optional Firecrawl discovery is disabled.")
        )
    elif os.environ.get("FIRECRAWL_API_KEY", "").strip():
        checks.append(
            Diagnostic(
                "ok",
                "Firecrawl",
                "Enabled and FIRECRAWL_API_KEY is present in the environment.",
            )
        )
    else:
        checks.append(
            Diagnostic(
                "error",
                "Firecrawl",
                "Enabled but FIRECRAWL_API_KEY is not set in the environment.",
            )
        )

    database = expand_path(config["storage"]["database"])
    if database.exists():
        try:
            with sqlite3.connect(database, timeout=2.0) as connection:
                version = int(connection.execute("PRAGMA user_version").fetchone()[0])
                quick_check = str(connection.execute("PRAGMA quick_check").fetchone()[0])
            if version > SCHEMA_VERSION:
                checks.append(
                    Diagnostic(
                        "error",
                        "database schema",
                        f"Schema {version} is newer than supported schema {SCHEMA_VERSION}.",
                    )
                )
            elif version < SCHEMA_VERSION:
                message = (
                    f"Schema {version} will migrate to {SCHEMA_VERSION} "
                    "on the next database command."
                )
                checks.append(Diagnostic("warn", "database schema", message))
            else:
                checks.append(
                    Diagnostic("ok", "database schema", f"Schema {version} is current.")
                )
            level = "ok" if quick_check == "ok" else "error"
            checks.append(Diagnostic(level, "database integrity", quick_check))
        except sqlite3.Error as exc:
            checks.append(Diagnostic("error", "database", f"Cannot inspect {database}: {exc}"))
        permission = _permission_check(database, "database")
        if permission:
            checks.append(permission)
    else:
        checks.append(
            Diagnostic("warn", "database", f"Database does not exist yet: {database}")
        )

    report_directory = expand_path(config["storage"]["report_directory"])
    writable = _writable_parent(report_directory)
    if writable is None or not os.access(writable, os.W_OK):
        checks.append(
            Diagnostic(
                "error",
                "report directory",
                f"No writable parent is available for {report_directory}.",
            )
        )
    else:
        checks.append(
            Diagnostic(
                "ok",
                "report directory",
                f"Reports can be written beneath {report_directory}.",
            )
        )

    if importlib.util.find_spec("jobspy") is None:
        checks.append(
            Diagnostic(
                "error",
                "JobSpy",
                "JobSpy is not importable; reinstall OpenJobScout or run `uv sync`.",
            )
        )
    else:
        checks.append(Diagnostic("ok", "JobSpy", "Discovery dependency is importable."))
    return checks
