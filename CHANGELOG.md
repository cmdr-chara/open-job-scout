# Changelog

## Unreleased

- Add an optional Firecrawl discovery source for employer-owned career sites, disabled by
  default and gated by `FIRECRAWL_API_KEY`.
- Keep JobSpy and the native Greenhouse, Lever, Ashby, and Recruitee APIs as the default
  discovery paths while merging normalized Firecrawl jobs into the same local pipeline.
- Add bounded Firecrawl web search, structured page scraping, exact-URL interaction
  opt-in, source-isolated failures, URL safety checks, and zero-data-retention requests.
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
- Make `jobscout show` human-readable by default while retaining `--json` for scripts and
  `--full` for the complete description.
- Add `jobscout next` for a focused highest-priority review workflow, with optional queue
  filters and browser opening.
- Add `jobscout review` for a guided batch-review session with open, note, reviewed,
  applied, rejected, closed, skip, and quit actions.
- Add `jobscout open` to launch the canonical employer URL or original source listing.
- Add `jobscout note` so notes can be recorded without changing application state or
  manual-state ownership.
- Add short aliases (`ls`, `view`, and `log`) plus contextual terminal tips and a more
  useful top-level help workflow.

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
