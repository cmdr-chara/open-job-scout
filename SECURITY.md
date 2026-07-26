# Security and privacy

OpenJobScout stores job-search state locally and does not operate a hosted
service. Treat the local database, configuration, reports, and imported CSVs as
sensitive: they may reveal employers, locations, notes, and application history.

The program contacts configured job boards during discovery and public job or
ATS URLs during verification. It does not upload a CV or submit an application,
but third-party services can still observe ordinary requests from your network.

## Reporting a vulnerability

Do not open a public issue for a vulnerability that exposes user data, executes
untrusted content, or leaks credentials. Contact the maintainers privately
through GitHub's security advisory feature.

## Project boundaries

- OpenJobScout never needs a CV to perform its core workflow, and it does not
  submit applications.
- Credentials must not be written to configuration files, reports, or logs.
- HTML from job listings is treated as untrusted data.
- The project does not bypass authentication or access controls.

## Upstream discovery boundary

Discovery uses a commit-pinned JobSpy revision while its latest PyPI release
still requires an affected `markdownify` version. OpenJobScout requests HTML
descriptions and converts them with its own bounded standard-library parser, so
the vulnerable conversion path is not used.

The JobSpy Indeed adapter is disabled because that upstream adapter currently
turns off TLS certificate verification. Other configured sources remain
available. Revisit both restrictions when upstream publishes a corrected
release.
