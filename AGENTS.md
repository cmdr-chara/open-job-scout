# OpenJobScout agent instructions

## Product contracts

- Keep CVs, notes, search history, and the SQLite tracker local. No hosted account, telemetry, or data upload should appear as an incidental implementation change.
- The native Rust runtime and supported Python runtime share tracker semantics. Treat SQLite schema, status transitions, CLI output, and [runtime-contract.toml](runtime-contract.toml) as compatibility surfaces.
- Preserve manually selected application states and notes during discovery, refresh, and verification. Rechecking a listing must not pretend it was rediscovered.
- Preserve uncertainty: unknown work mode, unavailable salary, failed source, or suggested successor must not become a confident classification, invented value, successful scan, or automatic replacement.
- No automatic applications, CAPTCHA bypasses, or credential harvesting. Optional paid discovery remains explicitly configured rather than a test prerequisite.

## Guidance and verification

Use [CONTRIBUTING.md](CONTRIBUTING.md) for contributor checks and [SECURITY.md](SECURITY.md) for privacy/security reporting. Use `uv run pytest` and `uv run ruff check .` for Python changes; validate affected Rust behavior with the committed Cargo manifest and lockfile. Schema or shared-behavior changes need cross-runtime compatibility coverage, not only one runtime's unit tests.

Use fictional jobs, temporary databases, and mocked or local network fixtures. Never run tests against personal tracker data or commit real CVs, application answers, emails, imports, or reports.

A change is complete when the requested behavior and affected compatibility/privacy contracts are checked, documentation matches, and live-source or platform limitations are explicit. A fixture pass does not prove a job is still open or an employer is legitimate.
