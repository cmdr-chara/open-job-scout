# Changelog

## Unreleased

- Filter tracked jobs by status, work mode, source, minimum score, and free-text query.
- Sort queue views by score or most recently seen and display work mode in `list` output.
- Add `jobscout stats` for pipeline, source, work-mode, salary, and top-new summaries.
- Add filtered CSV and JSON exports for spreadsheets and local analysis.
- Apply the richer queue filters to manually generated Markdown reports.
- Upgrade the local database to schema v3 with durable per-job event history.
- Add `jobscout history` for discovery, status, verification, note, and migration events.
- Add `jobscout recheck` to re-verify and re-rank tracked jobs without changing discovery
  timestamps; manual application states remain authoritative.
- Add `jobscout doctor` for config validation, SQLite schema/integrity checks, local
  permission checks, report-storage diagnostics, source safety, and JobSpy availability.
- Restrict newly initialized config files to mode `0600` on Unix-like systems, matching
  the existing database privacy behavior.

## 0.1.0 - 2026-08-11

First public release.

- Discover listings through JobSpy or import a compatible CSV.
- Filter and rank jobs with rules recorded in the local configuration.
- Check job links and supported public ATS endpoints for stale listings.
- Store jobs, notes, and application status in a local SQLite database.
- Export timestamped Markdown reports.
- Track manual applications without submitting forms or uploading a CV.

OpenJobScout is still alpha software. Confirm every listing on the employer's
official careers page before applying.
