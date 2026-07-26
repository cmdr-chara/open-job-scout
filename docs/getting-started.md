# Getting started

This guide takes OpenJobScout from a fresh installation to a tracked
application. OpenJobScout does not submit applications.

Examples use Windows PowerShell. Equivalent commands work in another shell;
the default data directory outside Windows is `~/.openjobscout/`.

## 1. Requirements

- Python 3.11 or 3.12
- `uv`
- [Git](https://git-scm.com/downloads)
- An internet connection for discovery and verification

Install `uv` by following its
[official installation guide](https://docs.astral.sh/uv/getting-started/installation/).

## 2. Install OpenJobScout

From the public repository:

```powershell
git clone https://github.com/cmdr-chara/open-job-scout.git
cd open-job-scout
uv tool install .
```

Confirm that the command is available:

```powershell
jobscout --help
```

For development, use `uv sync --extra dev` and prefix commands with `uv run`.

## 3. Create the local configuration

```powershell
jobscout init
```

Default locations:

| Data | Windows path |
| --- | --- |
| Config | `%USERPROFILE%\.openjobscout\config.toml` |
| Database | `%USERPROFILE%\.openjobscout\jobs.sqlite3` |
| Reports | `%USERPROFILE%\.openjobscout\reports\` |

On macOS or Linux, the corresponding defaults are
`~/.openjobscout/config.toml`, `~/.openjobscout/jobs.sqlite3`, and
`~/.openjobscout/reports/`.

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
jobscout show JOB_ID
```

`JOB_ID` can be the short ID printed by `list`. `show` prints the complete local
record as JSON; it does not open a browser page.

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
```

A refreshed listing does not overwrite an `applied`, `interview`, `rejected`,
or `offer` state.

## 9. Generate reports

```powershell
jobscout report
jobscout report --status applied
jobscout report --status interview --output interviews.md
```

Reports are Markdown snapshots. SQLite remains the source of truth.

## 10. Import an existing CSV

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

## Troubleshooting

### `jobscout` is not recognized

Restart the terminal after `uv tool install .`, or run from the repository:

```powershell
uv run jobscout --help
```

### A source returns no jobs

- Try one search term and one source.
- Lower `results_per_term`.
- Confirm the location spelling.
- Wait before retrying after a `429` or rate-limit response.
- Import a CSV or use another configured source.

### A Google result says `Job not found`

This is a stale index result. Keep the record as `closed`; do not apply through
mirrors or submit personal data to unrelated pages.

### Where is personal data stored?

The configuration, SQLite database, reports, notes, and imported CSV stay on
your machine. The program does make requests to configured job boards and to
public job or ATS URLs while discovering or verifying listings. It does not
upload a CV or submit applications. Local data paths and `data/` are excluded
from the repository by the default `.gitignore`.
