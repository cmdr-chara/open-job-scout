# Native provider discovery

Rust v2 can discover jobs directly from configured public ATS career APIs. These providers are intentionally isolated from the broader job-board adapters so one provider failure can be reported without aborting the rest of the search.

Add provider identifiers to `config.toml`:

```toml
[providers]
greenhouse = ["company-board-token"]
lever = ["company-site"]
lever_eu = []
ashby = ["company-job-board-name"]
recruitee = ["company-subdomain"]
```

Then run:

```console
jobscout search
```

The native search pipeline is:

1. query configured career APIs concurrently;
2. apply configured search terms;
3. normalize and deduplicate mirrors;
4. apply OpenJobScout filters;
5. safely verify employer URLs and enrich ATS metadata;
6. transparently rank retained jobs;
7. upsert into the schema-v3 tracker without stealing manually managed statuses;
8. mark old automatically managed jobs stale;
9. write a Markdown report.

Provider identifiers are tokens, not arbitrary URLs. This keeps discovery requests on fixed provider hosts. Link verification remains separately protected by public-IP validation, DNS pinning, proxy disabling, bounded bodies, and redirect revalidation.
