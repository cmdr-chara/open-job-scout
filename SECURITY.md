# Security and privacy

OpenJobScout stores job-search state locally and does not operate a hosted
service. Treat the local database and config as sensitive: they may reveal
employers, locations, notes, and application history.

## Reporting a vulnerability

Do not open a public issue for a vulnerability that exposes user data, executes
untrusted content, or leaks credentials. Contact the maintainers privately
through GitHub's security advisory feature.

## Project boundaries

- OpenJobScout never needs a CV to perform its core workflow.
- Credentials must not be written to config files or logs.
- HTML from job listings is treated as untrusted data.
- The project does not bypass authentication or access controls.
