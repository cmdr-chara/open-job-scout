import tomllib
from importlib.resources import files

import pytest

from open_job_scout.config import validate_config


def default_config() -> dict:
    resource = files("open_job_scout").joinpath("default_config.toml")
    with resource.open("rb") as handle:
        return tomllib.load(handle)


def test_bundled_config_is_valid() -> None:
    settings = default_config()
    assert validate_config(settings) is settings


def test_search_terms_must_not_be_empty() -> None:
    settings = default_config()
    settings["search"]["terms"] = []
    with pytest.raises(ValueError, match=r"\[search\]\.terms must not be empty"):
        validate_config(settings)


def test_degree_policy_must_be_supported() -> None:
    settings = default_config()
    settings["profile"]["degree_policy"] = "guess"
    with pytest.raises(ValueError, match="ignore, penalize, or filter"):
        validate_config(settings)

    settings["profile"]["degree_policy"] = ["penalize"]
    with pytest.raises(ValueError, match="ignore, penalize, or filter"):
        validate_config(settings)
