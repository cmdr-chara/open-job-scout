use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use anyhow::{Result, bail};
use time::{OffsetDateTime, format_description};

use crate::{
    config::{expand_path, load_config},
    importing, providers, ranking, reporting,
    storage::Storage,
    verification,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationKind {
    Search,
    Recheck,
}

impl OperationKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Search => "Search",
            Self::Recheck => "Recheck",
        }
    }
}

#[derive(Debug, Clone)]
pub struct OperationSummary {
    pub kind: OperationKind,
    pub processed: usize,
    pub stored: usize,
    pub rejected: usize,
    pub stale: usize,
    pub closed: usize,
    pub unreachable: usize,
    pub providers: usize,
    pub provider_errors: Vec<String>,
    pub report: Option<PathBuf>,
}

impl OperationSummary {
    pub fn notice(&self) -> String {
        match self.kind {
            OperationKind::Search => {
                let warning = if self.provider_errors.is_empty() {
                    String::new()
                } else {
                    format!(" · {} provider warning(s)", self.provider_errors.len())
                };
                format!(
                    "Search complete · {} stored · {} filtered · {} stale{}",
                    self.stored, self.rejected, self.stale, warning
                )
            }
            OperationKind::Recheck => format!(
                "Recheck complete · {} jobs · {} closed · {} unreachable",
                self.processed, self.closed, self.unreachable
            ),
        }
    }
}

pub fn search(database_path: &Path, config_path: &Path, workers: usize) -> Result<OperationSummary> {
    if workers == 0 {
        bail!("workers must be at least 1");
    }
    let config = load_config(config_path)?;
    let provider_config = providers::load_providers(config_path)?;
    if provider_config.is_empty() {
        bail!(
            "no first-party providers are configured; add a [providers] section to config.toml"
        );
    }
    let storage = Storage::open(database_path.to_path_buf())?;
    let batch = providers::discover(&provider_config, &config.search.terms, workers)?;
    let providers = batch.providers;
    let provider_errors = batch.errors;
    let processed = batch.jobs.len();
    let unique = importing::deduplicate(batch.jobs);

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

    let stored = importing::save_jobs(&storage, &retained)?;
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
    let report = report_dir.join(report_name("search")?);
    reporting::write_markdown(&current, &report)?;

    Ok(OperationSummary {
        kind: OperationKind::Search,
        processed,
        stored,
        rejected,
        stale,
        closed: current
            .iter()
            .filter(|job| job.verification == "closed")
            .count(),
        unreachable: current
            .iter()
            .filter(|job| job.verification == "unreachable")
            .count(),
        providers,
        provider_errors,
        report: Some(report),
    })
}

pub fn recheck(
    database_path: &Path,
    config_path: &Path,
    workers: usize,
) -> Result<OperationSummary> {
    if workers == 0 {
        bail!("workers must be at least 1");
    }
    let config = load_config(config_path)?;
    let storage = Storage::open(database_path.to_path_buf())?;
    let jobs = storage.load_jobs()?;
    let processed = jobs.len();
    if jobs.is_empty() {
        return Ok(OperationSummary {
            kind: OperationKind::Recheck,
            processed: 0,
            stored: 0,
            rejected: 0,
            stale: 0,
            closed: 0,
            unreachable: 0,
            providers: 0,
            provider_errors: Vec::new(),
            report: None,
        });
    }

    let mut refreshed = verification::verify_jobs(jobs, workers);
    for job in &mut refreshed {
        ranking::rank_job(job, &config);
    }
    let closed = refreshed
        .iter()
        .filter(|job| job.verification == "closed")
        .count();
    let unreachable = refreshed
        .iter()
        .filter(|job| job.verification == "unreachable")
        .count();
    let stored = storage.refresh_jobs(&refreshed)?;

    Ok(OperationSummary {
        kind: OperationKind::Recheck,
        processed,
        stored,
        rejected: 0,
        stale: 0,
        closed,
        unreachable,
        providers: 0,
        provider_errors: Vec::new(),
        report: None,
    })
}

fn report_name(prefix: &str) -> Result<String> {
    let timestamp = OffsetDateTime::now_utc().format(&format_description::parse_borrowed::<3>(
        "[year][month][day]-[hour][minute][second]",
    )?)?;
    Ok(format!("openjobscout-{prefix}-{timestamp}.md"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_notices_are_compact_and_user_facing() {
        let summary = OperationSummary {
            kind: OperationKind::Search,
            processed: 20,
            stored: 7,
            rejected: 11,
            stale: 2,
            closed: 0,
            unreachable: 1,
            providers: 4,
            provider_errors: vec!["lever: timeout".into()],
            report: None,
        };
        assert!(summary.notice().contains("7 stored"));
        assert!(summary.notice().contains("1 provider warning"));
    }

    #[test]
    fn operation_kind_labels_are_stable() {
        assert_eq!(OperationKind::Search.label(), "Search");
        assert_eq!(OperationKind::Recheck.label(), "Recheck");
    }
}
