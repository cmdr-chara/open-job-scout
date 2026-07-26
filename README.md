# OpenJobScout

**Find, verify, rank, and track jobs locally.**

OpenJobScout is a local-first, open-source job-search workbench. It discovers
job listings, applies transparent filters, verifies links, stores results in
SQLite, and tracks applications without uploading your CV or search history to
an OpenJobScout server.

> Alpha software: always confirm a listing on the employer's official careers
> page before applying.

## Why OpenJobScout?

- Local SQLite database; no account required.
- Transparent keyword scoring, never presented as an "ATS score".
- Search through JobSpy or import an existing CSV.
- Link and public ATS verification.
- Stale Google results are retained as history and marked `closed` when the
  official page returns `404`/`410`, displays an expiration message, or is
  absent from its public ATS API.
- Application states: `new`, `reviewed`, `applied`, `interview`, `rejected`,
  `offer`, and `closed`.
- Markdown reports suitable for review or archival.
- No automatic applications.

## Quick start

Requirements:

- Python 3.11 or newer
- [uv](https://docs.astral.sh/uv/)

Install directly from a clone:

```powershell
git clone https://github.com/cmdr-chara/open-job-scout.git
cd open-job-scout
uv tool install .
jobscout init
```

The `init` command creates:

```text
~/.openjobscout/config.toml
```

Edit that file before the first search. For example:

```toml
[search]
terms = [
  "junior backend developer",
  "graduate software engineer",
]
sites = ["linkedin", "google"]
location = "Italy"
country_indeed = "Italy"
results_per_term = 20
max_age_days = 14
```

Run the first search:

```powershell
jobscout search
jobscout list --status new
```

Open one result using the ID shown by `list`:

```powershell
jobscout show 425a56c785
```

After reviewing the official employer page, update its state:

```powershell
jobscout mark 425a56c785 reviewed
jobscout mark 425a56c785 applied --note "Applied on the employer careers page"
jobscout mark 425a56c785 interview --note "Technical interview on Friday"
```

Generate a report:

```powershell
jobscout report
jobscout report --status interview
```

JobSpy is installed as the default discovery engine. The tracker and CSV
importer remain usable when a particular job board is unavailable.

For the complete walkthrough, configuration reference, data locations, and
troubleshooting, read:

- [Getting started](docs/getting-started.md)
- [Guida introduttiva in italiano](docs/getting-started.it.md)

## Install for development

```powershell
git clone https://github.com/cmdr-chara/open-job-scout.git
cd open-job-scout
uv sync --extra dev
uv run jobscout init
uv run jobscout search
```

## Commands

```text
jobscout init
jobscout search [--config PATH] [--no-verify]
jobscout import-csv FILE [--config PATH] [--no-verify]
jobscout list [--config PATH] [--status new] [--limit 20]
jobscout show ID [--config PATH]
jobscout mark ID reviewed|applied|interview|rejected|offer|closed [--config PATH]
jobscout report [--config PATH] [--output PATH]
```

By default, personal state is written beneath `~/.openjobscout/`. The repository
does not need to contain a CV, database, generated report, or private config.

## Scoring

The score is a configurable prioritization heuristic based on title terms,
skills, junior signals, concerns, and work-location evidence. It is intended to
help order a review queue. It does not predict whether an employer's ATS or
recruiter will accept an application.

## Responsible use

Job boards may rate-limit or prohibit some forms of automated access. Use modest
query volumes, cache results, review the terms of each source, and prefer public
ATS APIs or official careers pages. OpenJobScout does not bypass authentication,
CAPTCHAs, or access controls.

## Development

```powershell
uv run pytest
uv run ruff check .
```

See [CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md).
Third-party attribution is recorded in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
