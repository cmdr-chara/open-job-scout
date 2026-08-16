# Optional Firecrawl discovery

OpenJobScout can use Firecrawl as a complementary discovery source for public employer
careers sites. It does **not** replace JobSpy in the Python CLI or the native Greenhouse,
Lever, Ashby, and Recruitee API providers in the Rust CLI.

Firecrawl is disabled by default. Enabling it requires a `FIRECRAWL_API_KEY` environment
variable; the key should never be written into `config.toml`.

## When it helps

Use Firecrawl when you want to broaden discovery beyond aggregator results, especially
for employer-owned career sites whose job lists or descriptions are rendered with
JavaScript.

The adapter can:

1. search the web from the existing `[search].terms` and `[search].location`;
2. scrape selected public pages into a small normalized job schema;
3. follow public job links found on careers index pages;
4. optionally use a Firecrawl interaction session for an exact careers URL that needs
   ordinary public navigation or a load-more control.

Direct ATS providers remain the preferred path for Greenhouse, Lever, Ashby, and
Recruitee because they are faster, cheaper, and based on structured public APIs. The
default Firecrawl domain exclusions keep those paths separate.

## Enable it

Set the API key in the process environment:

```sh
export FIRECRAWL_API_KEY="fc-..."
```

PowerShell:

```powershell
$env:FIRECRAWL_API_KEY = "fc-..."
```

Then enable the section in your local config:

```toml
[firecrawl]
enabled = true
search_enabled = true
search_limit_per_term = 8
max_scrapes = 16
career_urls = []
interact_urls = []
include_domains = []
exclude_domains = [
  "linkedin.com",
  "indeed.com",
  "glassdoor.com",
  "ziprecruiter.com",
  "greenhouse.io",
  "lever.co",
  "ashbyhq.com",
  "recruitee.com",
]
timeout_seconds = 45
zero_data_retention = true
```

Run the normal discovery command; there is no separate Firecrawl database or report:

```sh
jobscout search
```

The resulting jobs enter the existing deduplication, filtering, verification, ranking,
SQLite, stale-job, and Markdown-report pipeline with `source = "firecrawl"`.

## Target known company career sites

You can disable web search and seed employer pages directly:

```toml
[firecrawl]
enabled = true
search_enabled = false
career_urls = [
  "https://example.com/careers",
  "https://careers.example.org/jobs",
]
interact_urls = []
include_domains = []
exclude_domains = []
search_limit_per_term = 8
max_scrapes = 20
timeout_seconds = 45
zero_data_retention = true
```

This is useful when you already maintain a shortlist of employers and only want their
public postings.

## Domain allow-listing

Firecrawl's search API treats `includeDomains` and `excludeDomains` as mutually
exclusive. OpenJobScout therefore gives a non-empty `include_domains` list precedence
over its default exclusions:

```toml
include_domains = ["example.com", "example.org"]
```

If you configure a custom non-default `exclude_domains` list at the same time as an
allow-list, configuration validation rejects the ambiguity.

## Interaction is explicit opt-in

A normal scrape may report that a careers page needs interaction to expose additional
listings. OpenJobScout does **not** automatically start a browser interaction session.
The exact URL must also appear in `interact_urls`:

```toml
career_urls = ["https://example.com/careers"]
interact_urls = ["https://example.com/careers"]
```

The interaction prompt is deliberately narrow: it may reveal job links through normal
public navigation or load-more controls. It must not:

- log in or create an account;
- enter a CV, email address, phone number, or other personal data;
- fill or submit an application;
- solve or bypass a CAPTCHA;
- bypass an access control, paywall, or other protection.

After an interaction, OpenJobScout asks Firecrawl to stop the browser session even when
an extraction step fails.

## Data and privacy boundary

Firecrawl receives only information needed for this optional discovery source:

- the configured job-search term;
- the configured search location;
- configured or discovered public careers/job URLs.

The adapter does not send your:

- CV or cover letter;
- SQLite tracker;
- application status/history;
- notes;
- ranking preferences beyond the search terms/location;
- local report contents.

Only normalized `Job` fields are retained locally. OpenJobScout does not persist raw
Firecrawl API responses, raw page HTML, screenshots, browser session data, or the API
key.

`zero_data_retention = true` is sent to Firecrawl's scrape endpoint by default. Review
your Firecrawl plan, retention terms, credit usage, and privacy requirements before
turning the hosted source on.

## Cost and failure behavior

Firecrawl is intentionally bounded by `search_limit_per_term` and `max_scrapes`. A
single corporate careers page may lead to multiple posting scrapes, so API usage and
latency can be materially higher than direct ATS discovery.

Failures are isolated by source. A Firecrawl timeout or extraction failure is reported
as a discovery warning and does not discard successful JobSpy or direct-ATS results. If
every configured source fails, `jobscout search` exits with an error instead of
pretending that an empty result is authoritative.

Run diagnostics before a search:

```sh
jobscout doctor
```

When Firecrawl is enabled, diagnostics report an error if `FIRECRAWL_API_KEY` is absent.

## Security boundary

Only public HTTP(S) career/job URLs are accepted by the adapter. Literal private,
loopback, link-local, and local/internal targets are rejected where the runtime can
identify them. The integration never disables TLS verification and never forwards
arbitrary local authentication headers or cookies to a target site.

As with the other discovery sources, respect employer-site terms and use modest query
volumes. Firecrawl is a discovery/reading service here, not an application automation
mechanism.
