use std::{collections::HashMap, fs, path::Path, sync::OnceLock};

use anyhow::{Context, Result};
use csv::{ReaderBuilder, StringRecord};
use regex::Regex;
use scraper::Html;
use url::Url;

use crate::{
    identity::{job_dedup_key, job_fingerprint},
    model::{ApplicationStatus, Job, WorkMode},
    ranking::normalize_text,
};

const MISSING_VALUES: &[&str] = &["", "<na>", "na", "nan", "nat", "none", "null"];
const MAX_DESCRIPTION_CHARS: usize = 1_000_000;

pub fn import_csv(path: &Path) -> Result<Vec<Job>> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
    let mut reader = ReaderBuilder::new().flexible(true).from_reader(bytes);
    let headers = reader.headers()?.clone();
    let mut jobs = Vec::new();
    for record in reader.records() {
        jobs.push(row_to_job(&headers, &record?)?);
    }
    Ok(jobs)
}

pub fn deduplicate(jobs: Vec<Job>) -> Vec<Job> {
    let mut positions: HashMap<String, usize> = HashMap::new();
    let mut unique: Vec<Job> = Vec::new();
    for job in jobs {
        if job.title.is_empty() || job.company.is_empty() || job.source_url.is_empty() {
            continue;
        }
        let key = job_dedup_key(&job);
        if let Some(index) = positions.get(&key).copied() {
            let current = &unique[index];
            if (job.canonical_url.is_some(), job.description.len())
                > (current.canonical_url.is_some(), current.description.len())
            {
                unique[index] = job;
            }
        } else {
            positions.insert(key, unique.len());
            unique.push(job);
        }
    }
    unique
}

fn row_to_job(headers: &StringRecord, record: &StringRecord) -> Result<Job> {
    let mut row = HashMap::new();
    for (key, value) in headers.iter().zip(record.iter()) {
        row.insert(key.to_ascii_lowercase(), value);
    }
    let source_url = first_url(&row, &["job_url", "source_url", "job_url_direct", "canonical_url"])
        .unwrap_or_default();
    let canonical_url = first_url(&row, &["job_url_direct", "canonical_url"]);
    let title = clean_text(row.get("title").copied());
    let company = clean_text(row.get("company").copied());
    let description = plain_description(row.get("description").copied().unwrap_or(""));
    let remote_source = if row.contains_key("is_remote") {
        row.get("is_remote").copied()
    } else {
        row.get("remote").copied()
    };
    let employment_type = nonempty(
        clean_text(row.get("job_type").copied())
            .or_else_text(clean_text(row.get("employment_type").copied())),
    );
    let salary_min = first_float(&row, &["min_amount", "salary_min"]);
    let salary_max = first_float(&row, &["max_amount", "salary_max"]);
    let source = nonempty(
        clean_text(row.get("site").copied())
            .or_else_text(clean_text(row.get("source").copied())),
    )
    .unwrap_or_else(|| "import".into());
    let posted = nonempty(
        clean_text(row.get("date_posted").copied())
            .or_else_text(clean_text(row.get("posted_at").copied())),
    )
    .unwrap_or_default();
    let id = job_fingerprint(&company, &title, &source_url);

    Ok(Job {
        id,
        title,
        company,
        location: clean_text(row.get("location").copied()),
        remote: parse_bool(remote_source),
        work_mode: WorkMode::Unknown,
        employment_type,
        status: ApplicationStatus::New,
        score: 0.0,
        salary_min,
        salary_max,
        currency: nonempty(clean_text(row.get("currency").copied())),
        salary_source: nonempty(clean_text(row.get("salary_source").copied())),
        source,
        source_url,
        canonical_url,
        verification: "unverified".into(),
        verification_source: None,
        replacement_url: None,
        replacement_title: None,
        posted,
        first_seen: String::new(),
        last_seen: String::new(),
        status_updated_at: None,
        status_manually_set: false,
        reasons: Vec::new(),
        concerns: Vec::new(),
        description,
        notes: String::new(),
    })
}

fn first_url(row: &HashMap<String, &str>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|key| row.get(*key).copied())
        .find_map(clean_http_url)
}

fn clean_http_url(value: &str) -> Option<String> {
    let value = value.trim();
    if is_missing(value) || value.chars().any(char::is_whitespace) {
        return None;
    }
    let parsed = Url::parse(value).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return None;
    }
    Some(value.into())
}

fn clean_text(value: Option<&str>) -> String {
    let value = value.unwrap_or("");
    let normalized = whitespace_regex().replace_all(value, " ").trim().to_string();
    if is_missing(&normalized) {
        String::new()
    } else {
        normalized
    }
}

fn plain_description(value: &str) -> String {
    let truncated = value.chars().take(MAX_DESCRIPTION_CHARS).collect::<String>();
    let mut source = truncated;
    for regex in ignored_element_regexes() {
        source = regex.replace_all(&source, " ").into_owned();
    }
    let document = Html::parse_fragment(&source);
    normalize_text(&document.root_element().text().collect::<Vec<_>>().join(" "))
}

fn first_float(row: &HashMap<String, &str>, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| {
        let value = row.get(*key)?.trim();
        if value.is_empty() {
            return None;
        }
        let parsed = value.parse::<f64>().ok()?;
        parsed.is_finite().then_some(parsed)
    })
}

fn parse_bool(value: Option<&str>) -> Option<bool> {
    match value.unwrap_or("").trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "remote" => Some(true),
        "false" | "0" | "no" | "onsite" | "on-site" => Some(false),
        _ => None,
    }
}

fn nonempty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn is_missing(value: &str) -> bool {
    MISSING_VALUES
        .iter()
        .any(|missing| value.eq_ignore_ascii_case(missing))
}

trait OrElseText {
    fn or_else_text(self, other: String) -> String;
}

impl OrElseText for String {
    fn or_else_text(self, other: String) -> String {
        if self.is_empty() { other } else { self }
    }
}

fn whitespace_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\s+").expect("valid whitespace regex"))
}

fn ignored_element_regexes() -> &'static [Regex] {
    static REGEXES: OnceLock<Vec<Regex>> = OnceLock::new();
    REGEXES.get_or_init(|| {
        ["script", "style", "noscript", "svg"]
            .into_iter()
            .map(|tag| {
                Regex::new(&format!(r"(?is)<{tag}\b[^>]*>.*?</{tag}\s*>")).expect("valid tag regex")
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(headers: &[&str], values: &[&str]) -> Job {
        row_to_job(
            &StringRecord::from(headers.to_vec()),
            &StringRecord::from(values.to_vec()),
        )
        .unwrap()
    }

    #[test]
    fn jobspy_fields_keep_source_and_employer_urls_distinct() {
        let job = row(
            &["title", "company", "job_url", "job_url_direct", "is_remote", "min_amount"],
            &["Backend Engineer", "Example", "https://linkedin.test/job", "https://employer.test/job", "true", "50000"],
        );
        assert_eq!(job.source_url, "https://linkedin.test/job");
        assert_eq!(job.canonical_url.as_deref(), Some("https://employer.test/job"));
        assert_eq!(job.remote, Some(true));
        assert_eq!(job.salary_min, Some(50_000.0));
    }

    #[test]
    fn html_description_discards_script_and_style_content() {
        let job = row(
            &["title", "company", "job_url", "description"],
            &["Backend", "Example", "https://example.test/job", "<p>Hello <b>world</b></p><script>evil()</script><style>.x{}</style>"],
        );
        assert_eq!(job.description, "hello world");
    }

    #[test]
    fn missing_placeholders_and_invalid_urls_are_cleaned() {
        let job = row(
            &["title", "company", "job_url", "job_url_direct", "currency"],
            &["Backend", "Example", "not a url", "https://example.test/job", "NaN"],
        );
        assert_eq!(job.source_url, "https://example.test/job");
        assert!(job.currency.is_none());
    }

    #[test]
    fn dedup_prefers_richer_direct_listing() {
        let left = row(
            &["title", "company", "job_url", "job_url_direct", "description"],
            &["Backend", "Example", "https://board-a.test/job", "https://employer.test/job", "short"],
        );
        let right = row(
            &["title", "company", "job_url", "job_url_direct", "description"],
            &["Backend", "Example", "https://board-b.test/job", "https://employer.test/job", "a much richer description"],
        );
        let jobs = deduplicate(vec![left, right]);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].description, "a much richer description");
    }
}
