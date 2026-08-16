# Rust v2 rewrite

The Rust rewrite lives on `rewrite/rust-v2` until behavioral parity is proven against the Python implementation on `main`.

## Current milestone

The Ratatui application opens the same SQLite schema v3 used by the Python tracker. It loads real tracked jobs, preserves nullable remote/work-mode information, preserves manual-status ownership, writes durable status/note/verification events, supports stale marking, and resolves the database path from the existing `[storage].database` config value.

The Rust config loader validates the existing search/profile/filter/ranking/salary/storage structure. Transparent filtering and ranking have been ported, including required-vs-preferred experience handling, work-mode precedence, degree policy, salary rules, verification penalties, and the existing score weights. `rerank` recomputes local ranking metadata without changing discovery timestamps.

Live verification is now implemented for employer/job URLs and the existing Greenhouse, Lever, Ashby, and Recruitee ATS enrichments. The verifier rejects non-public targets and URL credentials, validates DNS results before connecting, pins validated addresses, disables proxy routing for verification requests, follows redirects manually with validation on every hop, bounds HTML/JSON bodies, detects closed page-level notices, enriches supported ATS metadata, and records automatic closed/reopened transitions through the existing tracker refresh semantics. `recheck` verifies concurrently and reranks without changing `last_seen_at`.

The no-argument command opens the TUI. Scriptable tracker commands currently include `list`, `show`, `mark`, `note`, `history`, `rerank`, `recheck`, `stats`, `export`, `doctor`, and `stale`.

Inside the TUI:

- arrow keys or `hjkl` navigate;
- `/` searches live;
- `Enter` or `o` opens the preferred employer URL;
- `n` adds a persistent note;
- `e` shows durable history;
- `r`, `a`, `i`, `x`, `Shift+O`, and `c` update pipeline status;
- `u` reloads the SQLite tracker.

## Remaining parity work

First-party discovery providers, CSV import, Markdown reports, richer queue filters, background TUI search/recheck UX, release packaging, and migration/release validation remain before Rust replaces Python on `main`. The Python implementation remains the behavioral reference until that gate is crossed.
