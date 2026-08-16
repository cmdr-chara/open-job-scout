use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use anyhow::{Result, bail};
use time::{OffsetDateTime, format_description};

use crate::{
    config::{expand_path, load_config},
    importing, migration, ranking, reporting,
    storage::Storage,
    verification,
};

pub fn import_csv(
    storage: &Storage,
    config_path: &Path,
    csv_path: &Path,
    verify: bool,
    workers: usize,
) -> Result<PathBuf> {
    if workers == 0 {
        bail!("workers must be at least 1");
    }
    let config = load_config(config_path)?;
    let raw = importing::import_csv(csv_path)?;
    let raw_count = raw.len();
    let unique = importing::deduplicate(raw);
    let unique_count = unique.len();

    let mut retained = Vec::new();
    let mut rejected = 0;
    for mut job in unique {
        let decision = ranking::filter_job(&mut job, &config);
        if decision.allowed {
            retained.push(job);
        } else {
            rejected += 1;
        }
    }

    let mut retained = if verify {
        verification::verify_jobs(retained, workers)
    } else {
        retained
    };
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

    println!("Imported rows: {raw_count}");
    println!("Unique valid jobs: {unique_count}");
    println!("Rejected by filters: {rejected}");
    println!("Stored: {stored}");
    println!("Marked stale: {stale}");
    println!("Report: {}", output.display());
    Ok(output)
}

pub fn report(storage: &Storage, output: Option<&Path>, limit: usize) -> Result<PathBuf> {
    if limit == 0 {
        bail!("limit must be at least 1");
    }
    let jobs = storage
        .load_jobs()?
        .into_iter()
        .take(limit)
        .collect::<Vec<_>>();
    let output = match output {
        Some(path) => path.to_path_buf(),
        None => std::env::current_dir()?.join(report_name()?),
    };
    reporting::write_markdown(&jobs, &output)
}

fn report_name() -> Result<String> {
    let timestamp = OffsetDateTime::now_utc().format(&format_description::parse_borrowed::<3>(
        "[year][month][day]-[hour][minute][second]",
    )?)?;
    Ok(format!("openjobscout-{timestamp}.md"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_names_are_markdown_files() {
        let name = report_name().unwrap();
        assert!(name.starts_with("openjobscout-"));
        assert!(name.ends_with(".md"));
    }
}
