from __future__ import annotations

import re
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def _load_toml(path: Path) -> dict[str, object]:
    return tomllib.loads(path.read_text(encoding="utf-8"))


def _schema_version(path: Path, pattern: str) -> int:
    match = re.search(pattern, path.read_text(encoding="utf-8"))
    if match is None:
        raise RuntimeError(f"Could not find schema version in {path.relative_to(ROOT)}")
    return int(match.group(1))


def main() -> int:
    contract = _load_toml(ROOT / "runtime-contract.toml")
    python_manifest = _load_toml(ROOT / "pyproject.toml")
    rust_manifest = _load_toml(ROOT / "Cargo.toml")

    project = python_manifest["project"]
    package = rust_manifest["package"]
    binaries = rust_manifest.get("bin", [])

    expected_package = contract["package_name"]
    expected_command = contract["command_name"]
    expected_schema = int(contract["schema_version"])

    checks = {
        "Python package name": project["name"] == expected_package,
        "Rust package name": package["name"] == expected_package,
        "Python version": project["version"] == contract["python_version"],
        "Rust version": package["version"] == contract["rust_version"],
        "Python command": expected_command in project["scripts"],
        "Rust command": any(binary.get("name") == expected_command for binary in binaries),
        "Python schema": _schema_version(
            ROOT / "src" / "open_job_scout" / "database.py",
            r"SCHEMA_VERSION\s*=\s*(\d+)",
        )
        == expected_schema,
        "Rust schema": _schema_version(
            ROOT / "src" / "storage.rs",
            r"const SCHEMA_VERSION:\s*i64\s*=\s*(\d+);",
        )
        == expected_schema,
    }

    failures = [name for name, passed in checks.items() if not passed]
    if failures:
        for name in failures:
            print(f"contract mismatch: {name}")
        return 1

    print(
        "runtime contract OK: "
        f"python={contract['python_version']} rust={contract['rust_version']} "
        f"schema={expected_schema} command={expected_command}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
