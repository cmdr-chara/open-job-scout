# Changelog

## Unreleased

- Filter tracked jobs by status, work mode, source, minimum score, and free-text query.
- Sort queue views by score or most recently seen and display work mode in `list` output.
- Add `jobscout stats` for pipeline, source, work-mode, salary, and top-new summaries.
- Add filtered CSV and JSON exports for spreadsheets and local analysis.
- Apply the richer queue filters to manually generated Markdown reports.

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
