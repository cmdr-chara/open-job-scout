# OpenJobScout

**Find, verify, rank, and track jobs locally.**

> 🇮🇹 **Preferisci l'italiano?**
> Leggi la **[guida completa in italiano](docs/getting-started.it.md)**.

OpenJobScout is a local-first, open-source job-search workbench. It discovers
job listings, applies transparent filters, verifies links, stores results in
SQLite, and tracks applications without uploading your CV or search history to
an OpenJobScout server.

> Alpha software: always confirm a listing on the employer's official careers
> page before applying.

## Why OpenJobScout?

- Local SQLite database; no account required.
- Transparent keyword scoring, never presented as an "ATS score".
- Search through JobSpy or import an existing CSV; each source fails independently.
- Conservative `remote`, `hybrid`, `onsite`, or `unknown` classification.
- Link and public ATS verification, with employer-published compensation when
  an ATS exposes it as structured data.
- Expired results are retained as `closed`; a unique same-title Ashby successor
  is shown as a suggestion and never substituted automatically.
- Unreviewed records not seen for the configured interval become `stale`.
- Application states: `new`, `reviewed`, `applied`, `interview`, `rejected`,
  `offer`, `closed`, and `stale`. Manual states survive crawler refreshes.
- Markdown reports suitable for review or archival.
- No automatic applications.

## Quick start

Requirements:

- [Git](https://git-scm.com/downloads)
- Python 3.11 or 3.12
- [uv](https://docs.astral.sh/uv/)

The commands below use PowerShell. On macOS or Linux, use the matching shell
syntax; OpenJobScout still uses `~/.openjobscout/` as its default data folder.

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

Inspect one result using the ID shown by `list`:

```powershell
jobscout show 425a56c785
```

After reviewing the official employer page, update its state:

```powershell
jobscout mark 425a56c785 reviewed
jobscout mark 425a56c785 applied --note "Applied on the employer careers page"
jobscout mark 425a56c785 interview --note "Technical interview on Friday"
```

Each `search` and `import-csv` command also writes a timestamped Markdown
snapshot automatically. Generate a fresh report from the current local tracker
when you need one:

```powershell
jobscout report
jobscout report --status interview
```

JobSpy is installed as the default discovery engine. The tracker and CSV
importer remain usable when a particular job board is unavailable.
The Indeed adapter is temporarily disabled because its current upstream
implementation does not verify TLS certificates; see [SECURITY.md](SECURITY.md).

For every option, see the annotated
[full configuration template](examples/config.example.toml).

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
jobscout --version
jobscout init [--output PATH] [--force]
jobscout search [--config PATH] [--no-verify]
jobscout import-csv FILE [--config PATH] [--no-verify]
jobscout list [--config PATH] [--status STATUS] [--limit N]
jobscout show ID [--config PATH]
jobscout mark ID STATUS [--config PATH] [--note TEXT]
jobscout report [--config PATH] [--status STATUS] [--limit N] [--output PATH]
```

By default, personal state is written beneath `~/.openjobscout/`. The repository
does not need to contain a CV, database, generated report, or private config.

## Privacy and network use

Your configuration, SQLite database, reports, notes, and any imported CSV stay
on your machine. Put imported files in `data/` if you keep them beside the
repository: that directory is ignored by Git. OpenJobScout does not submit a CV
or an application.

Discovery and verification do make network requests to the job boards you
configure and to public job or ATS URLs in the results. Those services receive
the requests they normally receive from your network connection; review their
terms and privacy notices before using them.

## Scoring

The score is a configurable prioritization heuristic based on title terms,
skills, junior signals, concerns, and work-location evidence. It is intended to
help order a review queue. It does not predict whether an employer's ATS or
recruiter will accept an application.

## Responsible use

Job boards may rate-limit or prohibit some forms of automated access. Use modest
query volumes, avoid repeated runs, review the terms of each source, and prefer
public ATS APIs or official careers pages. OpenJobScout does not bypass
authentication, CAPTCHAs, or access controls.

## Development

```powershell
uv run pytest
uv run ruff check .
```

See [CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md).
Third-party attribution is recorded in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
