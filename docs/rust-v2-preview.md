# Rust v2 preview

OpenJobScout v2 is being rebuilt as a native Rust terminal application with Ratatui.
The existing Python implementation on `main` remains the behavioral reference until the rewrite is feature-complete.

## Why Rust

A same-runner Go-versus-Rust decision benchmark compared process startup, a realistic two-pane TUI render, 10k-job interactive filtering, JSON parsing, SQLite queries, and HTML parsing. With UI and filtering weighted most heavily, Rust exceeded the agreed 1.50x threshold by a wide margin.

## Current preview

The current branch is intentionally UI-first and uses realistic demo data while the interaction design stabilizes.

- Responsive two-pane layout, stacking vertically in narrower terminals.
- Recommended, Applied, Interviews, and Pipeline tabs.
- Live keyboard search with `/`.
- Arrow keys or `j`/`k` for navigation.
- Arrow keys or `h`/`l` for tab switching.
- Mouse-wheel job navigation.
- Match gauge, salary, verification/source metadata, skills, concerns, description, and listing URL.
- In-memory status transitions for reviewed, applied, interview, rejected, offer, and closed.
- `?` shortcut overlay.
- `jobscout` and `jobscout ui` both launch the interface.

The preview does not write to the existing tracker yet. SQLite compatibility, event history, providers, verification, configuration, exports, and migration support are subsequent rewrite layers.
