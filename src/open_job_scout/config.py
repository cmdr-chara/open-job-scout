from __future__ import annotations

import shutil
import tomllib
from importlib.resources import files
from pathlib import Path
from typing import Any

APP_DIR = Path.home() / ".openjobscout"
DEFAULT_CONFIG = APP_DIR / "config.toml"


def expand_path(value: str) -> Path:
    return Path(value).expanduser().resolve()


def load_config(path: Path | None = None) -> dict[str, Any]:
    selected = path or DEFAULT_CONFIG
    if not selected.exists():
        raise FileNotFoundError(
            f"Config not found: {selected}. Run `jobscout init` or pass --config."
        )
    with selected.open("rb") as handle:
        return tomllib.load(handle)


def initialize_config(destination: Path | None = None, *, force: bool = False) -> Path:
    target = destination or DEFAULT_CONFIG
    if target.exists() and not force:
        raise FileExistsError(f"Config already exists: {target}")
    target.parent.mkdir(parents=True, exist_ok=True)
    resource = files("open_job_scout").joinpath("default_config.toml")
    with resource.open("rb") as source, target.open("wb") as output:
        shutil.copyfileobj(source, output)
    return target
