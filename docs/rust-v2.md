# Rust v2 rewrite

The Rust rewrite lives on `rewrite/rust-v2` until behavioral parity is proven against the Python implementation on `main`.

## Current milestone

The Ratatui application now opens the same SQLite schema v3 used by the Python tracker. It loads real tracked jobs, preserves manual-status ownership, writes durable status/note events, supports stale marking, and resolves the database path from the existing `[storage].database` config value.

The no-argument command opens the TUI. Scriptable tracker commands currently include `list`, `show`, `mark`, `note`, `history`, and `stale`.

Inside the TUI:

- arrow keys or `hjkl` navigate;
- `/` searches live;
- `Enter` or `o` opens the preferred employer URL;
- `n` adds a persistent note;
- `e` shows durable history;
- `r`, `a`, `i`, `x`, `Shift+O`, and `c` update pipeline status;
- `u` reloads the SQLite tracker.

Discovery, verification, ranking, import/export, diagnostics, and release packaging remain to be ported before Rust replaces Python on `main`.
