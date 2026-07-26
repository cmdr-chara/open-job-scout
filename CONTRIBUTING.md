# Contributing

Contributions are welcome when they keep OpenJobScout local-first, transparent,
and safe for job seekers.

1. Open an issue for substantial behavior or schema changes.
2. Keep changes small and explain their user impact in the pull request.
3. Add or update tests with the implementation.
4. Run `uv run pytest` and `uv run ruff check .`.
5. Do not commit real CVs, application answers, emails, databases, imported
   CSVs, or generated reports. Use `data/` for local imports.
6. Do not add CAPTCHA bypasses, credential harvesting, or automatic application
   submission.

Small, focused pull requests are easier to review and merge.
