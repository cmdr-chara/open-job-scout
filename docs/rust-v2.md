# Rust v2 rewrite

The native Rust implementation is now part of the repository alongside the
Python compatibility package. Both runtimes use the same tracker schema v3;
the Python implementation remains the behavioral reference for migration and
parity checks.

## Current milestone

The Ratatui application opens the same SQLite schema v3 used by the Python tracker. It loads real tracked jobs, preserves nullable remote/work-mode information, preserves manual-status ownership, writes durable status/note/verification events, supports stale marking, and resolves the database path from the existing `[storage].database` config value.

The Rust config loader validates the existing search/profile/filter/ranking/salary/storage structure. Transparent filtering and ranking have been ported, including required-vs-preferred experience handling, work-mode precedence, degree policy, salary rules, verification penalties, and the existing score weights. `rerank` recomputes local ranking metadata without changing discovery timestamps.

Live verification is implemented for employer/job URLs and the existing Greenhouse, Lever, Ashby, and Recruitee ATS enrichments. The verifier rejects non-public targets and URL credentials, validates DNS results before connecting, pins validated addresses, disables proxy routing for verification requests, follows redirects manually with validation on every hop, bounds HTML/JSON bodies, detects closed page-level notices, enriches supported ATS metadata, and records automatic closed/reopened transitions through the existing tracker refresh semantics. `recheck` verifies concurrently and reranks without changing `last_seen_at`.

CSV ingestion is also native now: Rust parses JobSpy-compatible CSVs, sanitizes descriptions, keeps source and employer URLs distinct, preserves nullable remote signals, derives the same SHA-256 source identity as Python, performs mirror deduplication, runs filtering/verification/ranking, upserts through the same discovery ownership rules, marks stale rows, and emits a Markdown report. Standalone Markdown reports and JSON/CSV tracker exports are available as well.

The no-argument command opens the TUI. Scriptable tracker commands currently include `list`, `show`, `mark`, `note`, `history`, `import-csv`, `report`, `rerank`, `recheck`, `stats`, `export`, `doctor`, and `stale`.

Structured output is available for the inspection commands:

```powershell
jobscout show JOB_ID --json
jobscout history JOB_ID --json
jobscout doctor --json
jobscout export --format json --output jobs.json
```

`export JOBS.json` remains supported as a positional-output form. Build the
native binary with `cargo build --locked --release` or install it with
`cargo install --path . --locked`. The `uv`/Python package remains available
for compatibility until a native package distribution is introduced.

Inside the TUI:

- arrow keys or `hjkl` navigate;
- `/` searches live;
- `Enter` or `o` opens the preferred employer URL;
- `n` adds a persistent note;
- `e` shows durable history;
- `r`, `a`, `i`, `x`, `Shift+O`, and `c` update pipeline status;
- `u` reloads the SQLite tracker.

The temporary diagnostic workflows used during the port are removed; both the
Python CI workflow and the strict Rust workflow run on `main`.

## Remaining release work

The local Rust implementation and strict Linux workflow cover the current
feature slice. Release follow-up still includes validation against real
provider boards, realistic Python-created trackers, the Windows/macOS release
matrix, and the remaining interactive UX edge cases.
