# Getting started

This guide takes OpenJobScout from a fresh installation to a tracked
application. OpenJobScout does not submit applications.

## 1. Requirements

- Python 3.11 or 3.12
- `uv`
- An internet connection for discovery and verification

Install `uv` by following its
[official installation guide](https://docs.astral.sh/uv/getting-started/installation/).

## 2. Install OpenJobScout

From a repository clone:

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

Open the config on Windows:

```powershell
notepad "$env:USERPROFILE\.openjobscout\config.toml"
```

Running `jobscout init` again does not overwrite an existing config. Use
`jobscout init --force` only when replacing it intentionally.

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
configured search term.

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

## 7. Review the shortlist

```powershell
jobscout list --status new
jobscout show JOB_ID
```

`JOB_ID` can be the short ID printed by `list`.

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

Only in the configured local SQLite file and generated reports. Those paths are
excluded from the repository by the default `.gitignore`.
