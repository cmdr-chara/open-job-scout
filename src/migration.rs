use std::collections::HashMap;

use anyhow::Result;

use crate::{identity::normalize_job_url, model::Job, ranking::normalize_text, storage::Storage};

pub fn reconcile_existing_ids(storage: &Storage, jobs: &mut [Job]) -> Result<usize> {
    let existing = storage.load_jobs()?;
    if existing.is_empty() || jobs.is_empty() {
        return Ok(0);
    }

    let mut by_url: HashMap<String, String> = HashMap::new();
    let mut ambiguous: HashMap<String, bool> = HashMap::new();
    for job in &existing {
        for candidate in [job.canonical_url.as_deref(), Some(job.source_url.as_str())]
            .into_iter()
            .flatten()
        {
            let normalized = normalize_job_url(candidate);
            if normalized.is_empty() {
                continue;
            }
            match by_url.get(&normalized) {
                Some(current) if current != &job.id => {
                    ambiguous.insert(normalized.clone(), true);
                }
                None => {
                    by_url.insert(normalized, job.id.clone());
                }
                _ => {}
            }
        }
    }
    for key in ambiguous.keys() {
        by_url.remove(key);
    }

    let mut reconciled = 0;
    for job in jobs {
        let matched = [job.canonical_url.as_deref(), Some(job.source_url.as_str())]
            .into_iter()
            .flatten()
            .map(normalize_job_url)
            .filter(|value| !value.is_empty())
            .find_map(|url| by_url.get(&url).cloned());
        if let Some(existing_id) = matched
            && existing_id != job.id
            && existing
                .iter()
                .find(|candidate| candidate.id == existing_id)
                .is_some_and(|candidate| compatible_identity(candidate, job))
        {
            job.id = existing_id;
            reconciled += 1;
        }
    }
    Ok(reconciled)
}

fn compatible_identity(existing: &Job, incoming: &Job) -> bool {
    let existing_title = normalize_text(&existing.title);
    let incoming_title = normalize_text(&incoming.title);
    let existing_company = normalize_text(&existing.company);
    let incoming_company = normalize_text(&incoming.company);
    !existing_title.is_empty()
        && !existing_company.is_empty()
        && existing_title == incoming_title
        && existing_company == incoming_company
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{importing, model::demo_jobs};
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn canonical_url_reuses_existing_python_style_identity() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("openjobscout-reconcile-{unique}.sqlite3"));
        let storage = Storage::open(&path).unwrap();
        let mut existing = demo_jobs().remove(0);
        existing.id = "a".repeat(64);
        existing.company = "acme".into();
        existing.source_url = "https://linkedin.example/job/123".into();
        existing.canonical_url = Some("https://jobs.ashbyhq.com/acme/abc".into());
        importing::save_jobs(&storage, std::slice::from_ref(&existing)).unwrap();

        let mut native = demo_jobs().remove(1);
        native.id = "b".repeat(64);
        native.title = existing.title.clone();
        native.company = "acme".into();
        native.source_url = "https://jobs.ashbyhq.com/acme/abc/application".into();
        native.canonical_url = Some("https://jobs.ashbyhq.com/acme/abc/application".into());
        let changed = reconcile_existing_ids(&storage, std::slice::from_mut(&mut native)).unwrap();
        assert_eq!(changed, 1);
        assert_eq!(native.id, existing.id);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn conflicting_metadata_does_not_reuse_existing_identity() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("openjobscout-reconcile-conflict-{unique}.sqlite3"));
        let storage = Storage::open(&path).unwrap();
        let mut existing = demo_jobs().remove(0);
        existing.id = "a".repeat(64);
        existing.source_url = "https://example.test/job/123".into();
        existing.canonical_url = None;
        importing::save_jobs(&storage, std::slice::from_ref(&existing)).unwrap();

        let mut incoming = existing.clone();
        incoming.id = "b".repeat(64);
        incoming.title = "Different role".into();
        let changed =
            reconcile_existing_ids(&storage, std::slice::from_mut(&mut incoming)).unwrap();
        assert_eq!(changed, 0);
        assert_eq!(incoming.id, "b".repeat(64));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn ambiguous_urls_are_not_reused() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("openjobscout-reconcile-ambiguous-{unique}.sqlite3"));
        let storage = Storage::open(&path).unwrap();
        let mut first = demo_jobs().remove(0);
        first.id = "a".repeat(64);
        first.source_url = "https://example.test/shared".into();
        first.canonical_url = None;
        importing::save_jobs(&storage, std::slice::from_ref(&first)).unwrap();
        let mut second = first.clone();
        second.id = "b".repeat(64);
        second.title = "Another role".into();
        importing::save_jobs(&storage, std::slice::from_ref(&second)).unwrap();

        let mut candidate = first.clone();
        candidate.id = "c".repeat(64);
        let changed =
            reconcile_existing_ids(&storage, std::slice::from_mut(&mut candidate)).unwrap();
        assert_eq!(changed, 0);
        assert_eq!(candidate.id, "c".repeat(64));
        let _ = fs::remove_file(path);
    }
}
