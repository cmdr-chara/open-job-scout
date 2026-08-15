# Getting started

This guide takes OpenJobScout from a fresh installation to a tracked
application. OpenJobScout does not submit applications.

Examples use Windows PowerShell. Equivalent commands work in another shell;
the default data directory outside Windows is `~/.openjobscout/`.

## 1. Requirements

- Python 3.11 or 3.12
- `uv`
- An internet connection for discovery and verification

Install `uv` by following its
[official installation guide](https://docs.astral.sh/uv/getting-started/installation/).

## 2. Install OpenJobScout

Install the command directly from the public repository:

```powershell
uv tool install git+https://github.com/cmdr-chara/open-job-scout.git@v0.1.0
```

Confirm that the command is available:

```powershell
jobscout --help
```

For development, clone the repository, run `uv sync --extra dev`, and prefix
commands with `uv run`.

## 3. Create the local configuration

```powershell
jobscout init
```

Default locations:

| Data     | Windows path                               |
| -------- | ------------------------------------------ |
| Config   | `%USERPROFILE%\.openjobscout\config.toml`  |
| Database | `%USERPROFILE%\.openjobscout\jobs.sqlite3` |
| Reports  | `%USERPROFILE%\.openjobscout\reports\`     |

On macOS or Linux, the corresponding defaults are
`~/.openjobscout/config.toml`, `~/.openjobscout/jobs.sqlite3`, and
`~/.openjobscout/reports/`. Newly created config and database files use mode
`0600` on Unix-like systems so other local users cannot read them by default.

Open the config on Windows:

```powershell
notepad "$env:USERPROFILE\.openjobscout\config.toml"
```

Running `jobscout init` again does not overwrite an existing config. Use
`jobscout init --force` only when replacing it intentionally.

The [full configuration template](../examples/config.example.toml) documents
every supported section in one place.

## 4. Configure discovery

Example:

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

Supported sources depend on JobSpy. Start with one or two sources and modest
result counts. Google-specific queries are generated automatically from each
configured search term. Indeed is temporarily disabled because the current
upstream adapter does not verify TLS certificates; see
[SECURITY.md](../SECURITY.md).

## 5. Configure filtering and ranking

```toml
[filters]
require_remote = true
allowed_employment_types = ["fulltime", "internship", "contract", ""]
blocked_title_terms = ["senior", "staff", "principal", "director"]
blocked_description_terms = ["mandatory relocation"]
max_required_years = 3

[ranking]
preferred_title_terms = ["software engineer", "backend", "python"]
preferred_skills = ["python", "django", "fastapi", "postgresql", "docker"]
junior_signals = ["junior", "graduate", "entry level", "new grad"]
concern_signals = ["unpaid", "on-site only"]
```

```toml
[salary]
minimum_annual = 0
preferred_annual = 50000
unknown_policy = "allow"
unknown_penalty = 0
preferred_bonus = 10
```

The score orders the local review queue. It is not an ATS score and does not
predict an employer's decision.

## 6. Search

```powershell
jobscout search
```

To keep discovery but skip outgoing verification requests:

```powershell
jobscout search --no-verify
```

OpenJobScout deduplicates results, applies hard filters, checks reachable links
and public ATS APIs, ranks retained jobs, and writes them to SQLite.
It also writes a timestamped Markdown report to the configured report directory.

## 7. Review the shortlist

```powershell
jobscout list --status new
jobscout list --status new --work-mode remote --min-score 60 --query python
jobscout show JOB_ID
```

`JOB_ID` can be the short ID printed by `list`. `show` prints the complete local
record as JSON; it does not open a browser page.

The queue can be filtered by application state, work mode, source, minimum
score, and free text. It can be sorted by score or by the most recent discovery
timestamp with `--sort newest`.

Always open the canonical URL and confirm:

- the job is still accepting applications;
- the employer and role are legitimate;
- the location and remote-work conditions are compatible;
- mandatory requirements are accurate;
- compensation and contract terms are acceptable.

Google may retain expired results. If the official page returns `404`, `410`,
shows an expiration message, or no longer contains the ATS posting,
OpenJobScout stores the historical record as `closed`.

## 8. Track the application

```powershell
jobscout mark JOB_ID reviewed
jobscout mark JOB_ID applied --note "Applied on the official careers page"
jobscout mark JOB_ID interview --note "First interview scheduled"
jobscout mark JOB_ID offer
```

Available states:

```text
new
reviewed
applied
interview
rejected
offer
closed
stale
```

A refreshed listing does not overwrite an `applied`, `interview`, `rejected`,
or `offer` state.

Schema v3 also stores an append-only event history for each job. Inspect it with:

```powershell
jobscout history JOB_ID
jobscout history JOB_ID --json
```

History contains discovery, verification changes, automatic state transitions,
manual status changes, notes, and a migration snapshot for trackers created by
older OpenJobScout versions.

## 9. Recheck existing jobs without rediscovery

Use `recheck` when you want to know whether already tracked URLs are still
active without running the configured job-board searches again:

```powershell
jobscout recheck JOB_ID
jobscout recheck JOB_ID OTHER_ID
```

You can also recheck a filtered queue slice:

```powershell
jobscout recheck --status new --work-mode remote --min-score 60
jobscout recheck --status closed --limit 20
```

The default queue limit is 50. `--workers` controls parallel URL verification.
A recheck updates verification metadata, ATS enrichment, and ranking, but it
does **not** update `last_seen_at`, because the job was not rediscovered.

Automatic `closed` jobs return to `new` when a later recheck proves they are
active. Manual states remain authoritative, and `stale` remains stale until a
new discovery sees the listing again.

## 10. Generate reports and exports

```powershell
jobscout report
jobscout report --status applied
jobscout report --status interview --output interviews.md
jobscout export --status applied --format csv
jobscout export --work-mode remote --min-score 70 --format json
```

Markdown reports and CSV/JSON exports can use the same tracker filters as
`list`. Reports are snapshots; SQLite remains the source of truth.

For a compact tracker overview:

```powershell
jobscout stats
```

`stats` includes status, source, and work-mode counts, salary coverage, average
score, and the highest-ranked new jobs.

## 11. Import an existing CSV

OpenJobScout accepts JobSpy-compatible CSV columns:

```powershell
jobscout import-csv .\jobs.csv
jobscout import-csv .\jobs.csv --no-verify
```

Common recognized fields include `title`, `company`, `job_url`,
`job_url_direct`, `location`, `is_remote`, `job_type`, `description`,
`date_posted`, `min_amount`, `max_amount`, and `currency`.

An imported CSV can contain personal notes or a job-search history. Keep it
outside version control; `data/` is ignored by the bundled `.gitignore` and is
a good local location when working from this repository.

## 12. Check local health

Run the local diagnostic command before debugging search failures or manually
inspecting the SQLite file:

```powershell
jobscout doctor
jobscout doctor --json
```

`doctor` checks:

- configuration validity;
- disabled source choices such as the current Indeed adapter;
- SQLite schema compatibility and `PRAGMA quick_check`;
- Unix config/database file permissions;
- report-directory writability;
- whether JobSpy is importable.

It does not run a live job-board search.

## Troubleshooting

### `jobscout` is not recognized

Restart the terminal after `uv tool install .`, or run from the repository:

```powershell
uv run jobscout --help
```

### A source returns no jobs

- Run `jobscout doctor` first.
- Try one search term and one source.
- Lower `results_per_term`.
- Confirm the location spelling.
- Wait before retrying after a `429` or rate-limit response.
- Import a CSV or use another configured source.

### A Google result says `Job not found`

This is a stale index result. Keep the record as `closed`; do not apply through
mirrors or submit personal data to unrelated pages. You can later use
`jobscout recheck JOB_ID` to see whether the official listing became active
again without changing its discovery timestamp.

### Where is personal data stored?

The configuration, SQLite database, reports, notes, event history, and imported
CSV stay on your machine. The program does make requests to configured job
boards and to public job or ATS URLs while discovering or verifying listings.
`recheck` only performs the public job/ATS verification part. OpenJobScout does
not upload a CV or submit applications. Local data paths and `data/` are
excluded from the repository by the default `.gitignore`.
