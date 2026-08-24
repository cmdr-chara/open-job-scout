use sha2::{Digest, Sha256};
use url::Url;

use crate::{model::Job, ranking::normalize_text};

const TRACKING_QUERY_KEYS: &[&str] = &[
    "ref",
    "source",
    "trk",
    "trackingid",
    "utm_campaign",
    "utm_content",
    "utm_medium",
    "utm_source",
    "utm_term",
];

pub fn job_fingerprint(company: &str, title: &str, source_url: &str) -> String {
    let identity = [company, title, source_url]
        .into_iter()
        .map(normalize_text)
        .collect::<Vec<_>>()
        .join("|");
    format!("{:x}", Sha256::digest(identity.as_bytes()))
}

pub fn normalize_job_url(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        return String::new();
    }
    let Ok(mut url) = Url::parse(value) else {
        return String::new();
    };
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return String::new();
    }

    let mut path = url.path().trim_end_matches('/').to_string();
    if path.to_ascii_lowercase().ends_with("/application") {
        path.truncate(path.len() - "/application".len());
    }
    url.set_path(&path);
    url.set_fragment(None);

    let mut pairs = url
        .query_pairs()
        .filter(|(key, _)| {
            !TRACKING_QUERY_KEYS
                .iter()
                .any(|blocked| key.eq_ignore_ascii_case(blocked))
        })
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    pairs.sort();
    url.set_query(None);
    if !pairs.is_empty() {
        url.query_pairs_mut().extend_pairs(pairs);
    }
    url.to_string()
}

pub fn job_dedup_key(job: &Job) -> String {
    let direct = job
        .canonical_url
        .as_deref()
        .map(normalize_job_url)
        .unwrap_or_default();
    if !direct.is_empty() {
        return format!("url:{direct}");
    }
    let base = format!(
        "{}|{}",
        normalize_text(&job.company),
        normalize_text(&job.title)
    );
    if job.remote == Some(true) {
        return format!("remote:{base}");
    }
    format!("source:{}", normalize_job_url(&job.source_url))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::demo_jobs;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct ContractVectors {
        fingerprints: Vec<FingerprintVector>,
        urls: Vec<UrlVector>,
    }

    #[derive(Debug, Deserialize)]
    struct FingerprintVector {
        company: String,
        title: String,
        source_url: String,
        expected: String,
    }

    #[derive(Debug, Deserialize)]
    struct UrlVector {
        input: String,
        expected: String,
    }

    fn contract_vectors() -> ContractVectors {
        serde_json::from_str(include_str!(
            "../tests/fixtures/runtime_contract_vectors.json"
        ))
        .unwrap()
    }

    #[test]
    fn fingerprint_matches_shared_runtime_contract() {
        for vector in contract_vectors().fingerprints {
            assert_eq!(
                job_fingerprint(&vector.company, &vector.title, &vector.source_url),
                vector.expected
            );
        }
    }

    #[test]
    fn url_normalization_matches_shared_runtime_contract() {
        for vector in contract_vectors().urls {
            assert_eq!(normalize_job_url(&vector.input), vector.expected);
        }
    }

    #[test]
    fn remote_mirrors_deduplicate_by_company_and_title() {
        let mut left = demo_jobs().remove(0);
        left.canonical_url = None;
        left.remote = Some(true);
        let mut right = left.clone();
        right.source_url = "https://another-board.test/mirror".into();
        assert_eq!(job_dedup_key(&left), job_dedup_key(&right));
    }
}
