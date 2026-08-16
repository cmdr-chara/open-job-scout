use std::{
    collections::HashSet,
    env,
    path::{Path, PathBuf},
};

use anyhow::{Result, bail};
use time::{OffsetDateTime, format_description};

#[path = "firecrawl.rs"]
mod firecrawl;

use crate::{
    config::{expand_path, load_config},
    importing, migration, providers, ranking, reporting,
    safety::terminal_text,
    storage::Storage,
    verification,
};

pub(crate) fn firecrawl_status(config_path: &Path) -> Result<(bool, bool)> {
    let config = firecrawl::load(config_path)?;
    let key_present = env::var("FIRECRAWL_API_KEY")
        .ok()
        .is_some_and(|value| !value.trim().is_empty());
    Ok((config.enabled, key_present))
}

pub fn search(storage: &Storage, config_path: &Path, workers: usize) -> Result<PathBuf> {
    if workers == 0 {
        bail!("workers must be at least 1");
    }
    let config = load_config(config_path)?;
    let provider_config = providers::load_providers(config_path)?;
    let firecrawl_config = firecrawl::load(config_path)?;
    if provider_config.is_empty() && !firecrawl_config.enabled {
        bail!(
            "no discovery source is configured; add first-party [providers] board identifiers or explicitly enable [firecrawl]"
        );
    }

    let mut jobs = Vec::new();
    let mut errors = Vec::new();
    let mut provider_tasks = 0usize;
    let mut successful_sources = 0usize;

    if !provider_config.is_empty() {
        let batch = providers::discover(&provider_config, &config.search.terms, workers)?;
        provider_tasks = batch.providers;
        let failed = batch.errors.len();
        if batch.providers > failed {
            successful_sources += 1;
        }
        jobs.extend(batch.jobs);
        errors.extend(batch.errors);
    }

    let mut firecrawl_searches = 0usize;
    let mut firecrawl_scrapes = 0usize;
    let mut firecrawl_interactions = 0usize;
    if firecrawl_config.enabled {
        match firecrawl::discover(
            &firecrawl_config,
            &config.search.terms,
            &config.search.location,
        ) {
            Ok(batch) => {
                firecrawl_searches = batch.searches;
                firecrawl_scrapes = batch.scrapes;
                firecrawl_interactions = batch.interactions;
                if batch.searches > 0 || batch.scrapes > 0 {
                    successful_sources += 1;
                }
                jobs.extend(batch.jobs);
                errors.extend(
                    batch
                        .errors
                        .into_iter()
                        .map(|error| format!("firecrawl: {error}")),
                );
            }
            Err(error) => errors.push(format!("firecrawl: {error:#}")),
        }
    }

    if successful_sources == 0 && !errors.is_empty() {
        bail!(
            "all configured discovery sources failed: {}",
            errors.join("; ")
        );
    }

    let discovered = jobs.len();
    let unique = importing::deduplicate(jobs);
    let unique_count = unique.len();

    let mut retained = Vec::new();
    let mut rejected = Vec::new();
    for mut job in unique {
        let decision = ranking::filter_job(&mut job, &config);
        if decision.allowed {
            retained.push(job);
        } else {
            rejected.push((job, decision.reason.unwrap_or_else(|| "filtered".into())));
        }
    }

    let mut retained = verification::verify_jobs(retained, workers);
    for job in &mut retained {
        ranking::rank_job(job, &config);
    }
    retained.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| right.posted.cmp(&left.posted))
    });

    migration::reconcile_existing_ids(storage, &mut retained)?;
    let stored = importing::save_jobs(storage, &retained)?;
    let stale = storage.mark_stale_jobs(config.storage.stale_after_days)?;
    let ids = retained
        .iter()
        .map(|job| job.id.as_str())
        .collect::<HashSet<_>>();
    let current = storage
        .load_jobs()?
        .into_iter()
        .filter(|job| ids.contains(job.id.as_str()))
        .collect::<Vec<_>>();

    let report_dir = expand_path(Path::new(&config.storage.report_directory))?;
    let output = report_dir.join(report_name()?);
    reporting::write_markdown(&current, &output)?;

    println!("First-party provider tasks queried: {provider_tasks}");
    if firecrawl_config.enabled {
        println!(
            "Firecrawl: searches={firecrawl_searches}, scrapes={firecrawl_scrapes}, interactions={firecrawl_interactions}"
        );
    }
    println!("Discovery rows: {discovered}");
    println!("Unique valid jobs: {unique_count}");
    println!("Rejected by filters: {}", rejected.len());
    println!("Stored: {stored}");
    println!("Marked stale: {stale}");
    for error in &errors {
        eprintln!("Discovery warning: {}", terminal_text(error));
    }
    println!("Report: {}", terminal_text(&output.display().to_string()));
    Ok(output)
}

fn report_name() -> Result<String> {
    let timestamp = OffsetDateTime::now_utc().format(&format_description::parse_borrowed::<3>(
        "[year][month][day]-[hour][minute][second]",
    )?)?;
    Ok(format!("openjobscout-search-{timestamp}.md"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_names_are_stable_markdown_names() {
        let value = report_name().unwrap();
        assert!(value.starts_with("openjobscout-search-"));
        assert!(value.ends_with(".md"));
    }
}
