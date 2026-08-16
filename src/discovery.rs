use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use anyhow::{Result, bail};
use time::{OffsetDateTime, format_description};

use crate::{
    config::{expand_path, load_config},
    importing, migration, providers, ranking, reporting,
    storage::Storage,
    verification,
};

pub fn search(storage: &Storage, config_path: &Path, workers: usize) -> Result<PathBuf> {
    if workers == 0 {
        bail!("workers must be at least 1");
    }
    let config = load_config(config_path)?;
    let provider_config = providers::load_providers(config_path)?;
    if provider_config.is_empty() {
        bail!(
            "no first-party providers are configured; add a [providers] section with Greenhouse, Lever, Ashby, or Recruitee board identifiers"
        );
    }

    let batch = providers::discover(&provider_config, &config.search.terms, workers)?;
    let discovered = batch.jobs.len();
    let unique = importing::deduplicate(batch.jobs);
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

    println!("Providers queried: {}", batch.providers);
    println!("Provider rows: {discovered}");
    println!("Unique valid jobs: {unique_count}");
    println!("Rejected by filters: {}", rejected.len());
    println!("Stored: {stored}");
    println!("Marked stale: {stale}");
    for error in &batch.errors {
        eprintln!("Provider warning: {error}");
    }
    println!("Report: {}", output.display());
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
