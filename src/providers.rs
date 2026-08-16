use std::{
    collections::VecDeque,
    fs,
    path::Path,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use scraper::Html;
use serde::Deserialize;
use serde_json::Value;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use url::Url;

use crate::{
    config::expand_path,
    identity::job_fingerprint,
    model::{ApplicationStatus, Job, WorkMode},
    ranking::{contains_term, normalize_text},
};

const USER_AGENT: &str = "OpenJobScout/0.2 (+https://github.com/cmdr-chara/open-job-scout)";

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProvidersConfig {
    #[serde(default)]
    pub greenhouse: Vec<String>,
    #[serde(default)]
    pub lever: Vec<String>,
    #[serde(default)]
    pub lever_eu: Vec<String>,
    #[serde(default)]
    pub ashby: Vec<String>,
    #[serde(default)]
    pub recruitee: Vec<String>,
}

impl ProvidersConfig {
    pub fn is_empty(&self) -> bool {
        self.greenhouse.is_empty()
            && self.lever.is_empty()
            && self.lever_eu.is_empty()
            && self.ashby.is_empty()
            && self.recruitee.is_empty()
    }

    fn tasks(&self) -> Vec<ProviderTask> {
        self.greenhouse
            .iter()
            .cloned()
            .map(ProviderTask::Greenhouse)
            .chain(
                self.lever
                    .iter()
                    .cloned()
                    .map(|site| ProviderTask::Lever { site, eu: false }),
            )
            .chain(
                self.lever_eu
                    .iter()
                    .cloned()
                    .map(|site| ProviderTask::Lever { site, eu: true }),
            )
            .chain(self.ashby.iter().cloned().map(ProviderTask::Ashby))
            .chain(self.recruitee.iter().cloned().map(ProviderTask::Recruitee))
            .collect()
    }
}

#[derive(Debug, Deserialize)]
struct ProviderFile {
    #[serde(default)]
    providers: ProvidersConfig,
}

#[derive(Debug, Clone)]
enum ProviderTask {
    Greenhouse(String),
    Lever { site: String, eu: bool },
    Ashby(String),
    Recruitee(String),
}

impl ProviderTask {
    fn label(&self) -> String {
        match self {
            Self::Greenhouse(board) => format!("greenhouse:{board}"),
            Self::Lever { site, eu: false } => format!("lever:{site}"),
            Self::Lever { site, eu: true } => format!("lever-eu:{site}"),
            Self::Ashby(board) => format!("ashby:{board}"),
            Self::Recruitee(company) => format!("recruitee:{company}"),
        }
    }
}

#[derive(Debug, Default)]
pub struct DiscoveryBatch {
    pub jobs: Vec<Job>,
    pub errors: Vec<String>,
    pub providers: usize,
}

pub fn load_providers(path: &Path) -> Result<ProvidersConfig> {
    let path = expand_path(path)?;
    let source = fs::read_to_string(&path)
        .with_context(|| format!("failed to read provider config {}", path.display()))?;
    let parsed: ProviderFile = toml::from_str(&source)
        .with_context(|| format!("failed to parse provider config {}", path.display()))?;
    validate(&parsed.providers)?;
    Ok(parsed.providers)
}

pub fn discover(
    providers: &ProvidersConfig,
    terms: &[String],
    workers: usize,
) -> Result<DiscoveryBatch> {
    if workers == 0 {
        bail!("provider workers must be at least 1");
    }
    let tasks = providers.tasks();
    if tasks.is_empty() {
        return Ok(DiscoveryBatch::default());
    }

    let count = tasks.len();
    let queue = Arc::new(Mutex::new(VecDeque::from_iter(
        tasks.into_iter().enumerate(),
    )));
    let (sender, receiver) = mpsc::channel();
    let worker_count = workers.min(count).max(1);
    thread::scope(|scope| {
        for _ in 0..worker_count {
            let queue = Arc::clone(&queue);
            let sender = sender.clone();
            scope.spawn(move || {
                loop {
                    let item = queue.lock().expect("provider queue poisoned").pop_front();
                    let Some((index, task)) = item else {
                        break;
                    };
                    let label = task.label();
                    let result = fetch_task(&task)
                        .map(|jobs| {
                            jobs.into_iter()
                                .filter(|job| matches_terms(job, terms))
                                .collect()
                        })
                        .map_err(|error| format!("{label}: {error:#}"));
                    if sender.send((index, result)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(sender);
    });

    let mut results = receiver.into_iter().collect::<Vec<_>>();
    results.sort_by_key(|(index, _)| *index);
    let mut batch = DiscoveryBatch {
        providers: count,
        ..DiscoveryBatch::default()
    };
    for (_, result) in results {
        match result {
            Ok(mut jobs) => batch.jobs.append(&mut jobs),
            Err(error) => batch.errors.push(error),
        }
    }
    Ok(batch)
}

fn validate(config: &ProvidersConfig) -> Result<()> {
    for (kind, values) in [
        ("greenhouse", &config.greenhouse),
        ("lever", &config.lever),
        ("lever_eu", &config.lever_eu),
        ("ashby", &config.ashby),
        ("recruitee", &config.recruitee),
    ] {
        for value in values {
            if !valid_token(value) {
                bail!(
                    "invalid [providers].{kind} token {value:?}; use only letters, digits, '-' or '_'"
                );
            }
        }
    }
    Ok(())
}

fn valid_token(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn fetch_task(task: &ProviderTask) -> Result<Vec<Job>> {
    match task {
        ProviderTask::Greenhouse(board) => fetch_greenhouse(board),
        ProviderTask::Lever { site, eu } => fetch_lever(site, *eu),
        ProviderTask::Ashby(board) => fetch_ashby(board),
        ProviderTask::Recruitee(company) => fetch_recruitee(company),
    }
}

fn client() -> Result<Client> {
    Ok(Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(10))
        .user_agent(USER_AGENT)
        .build()?)
}

fn get_json(url: Url) -> Result<Value> {
    let response = client()?.get(url.clone()).send()?;
    let status = response.status();
    if !status.is_success() {
        bail!(
            "{} returned HTTP {}",
            url.host_str().unwrap_or("provider"),
            status.as_u16()
        );
    }
    Ok(response.json()?)
}

fn fetch_greenhouse(board: &str) -> Result<Vec<Job>> {
    let mut url = Url::parse("https://api.greenhouse.io/v1/boards/")?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("invalid Greenhouse base URL"))?
        .pop_if_empty()
        .push(board)
        .push("jobs");
    url.query_pairs_mut().append_pair("content", "true");
    let payload = get_json(url)?;
    let jobs = payload
        .get("jobs")
        .and_then(Value::as_array)
        .context("Greenhouse response did not contain jobs[]")?;
    Ok(jobs
        .iter()
        .filter_map(|value| map_greenhouse(value, board))
        .collect())
}

fn map_greenhouse(value: &Value, board: &str) -> Option<Job> {
    let title = text_field(value, "title")?;
    let url = text_field(value, "absolute_url")?;
    let location = value
        .get("location")
        .and_then(|location| text_field(location, "name"))
        .unwrap_or_default();
    let description = text_field(value, "content")
        .map(|html| plain_html(&html))
        .unwrap_or_default();
    Some(base_job(
        title,
        board.to_string(),
        location,
        "greenhouse",
        url.clone(),
        Some(url),
        description,
    ))
}

fn fetch_lever(site: &str, eu: bool) -> Result<Vec<Job>> {
    let base = if eu {
        "https://api.eu.lever.co/v0/postings/"
    } else {
        "https://api.lever.co/v0/postings/"
    };
    let mut url = Url::parse(base)?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("invalid Lever base URL"))?
        .pop_if_empty()
        .push(site);
    url.query_pairs_mut().append_pair("mode", "json");
    let payload = get_json(url)?;
    let jobs = payload
        .as_array()
        .context("Lever response was not an array")?;
    Ok(jobs
        .iter()
        .filter_map(|value| map_lever(value, site))
        .collect())
}

fn map_lever(value: &Value, site: &str) -> Option<Job> {
    let title = text_field(value, "text")?;
    let source_url = text_field(value, "hostedUrl")?;
    let canonical_url = text_field(value, "applyUrl").or_else(|| Some(source_url.clone()));
    let categories = value.get("categories");
    let location = categories
        .and_then(|value| text_field(value, "location"))
        .unwrap_or_default();
    let employment_type = categories.and_then(|value| text_field(value, "commitment"));
    let description = text_field(value, "descriptionPlain").unwrap_or_default();
    let mut job = base_job(
        title,
        site.to_string(),
        location,
        "lever",
        source_url,
        canonical_url,
        description,
    );
    job.employment_type = employment_type;
    if let Some(workplace) = text_field(value, "workplaceType") {
        apply_workplace(&mut job, &workplace);
    }
    if let Some(created) = value.get("createdAt").and_then(Value::as_i64) {
        job.posted = timestamp_millis(created).unwrap_or_default();
    }
    if let Some(salary) = value.get("salaryRange")
        && salary.get("interval").and_then(Value::as_str) == Some("per-year-salary")
    {
        job.salary_min = salary.get("min").and_then(number);
        job.salary_max = salary.get("max").and_then(number);
        job.currency = salary.get("currency").and_then(value_text);
        job.salary_source = Some("lever".into());
    }
    Some(job)
}

fn fetch_ashby(board: &str) -> Result<Vec<Job>> {
    let mut url = Url::parse("https://api.ashbyhq.com/posting-api/job-board/")?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("invalid Ashby base URL"))?
        .pop_if_empty()
        .push(board);
    url.query_pairs_mut()
        .append_pair("includeCompensation", "true");
    let payload = get_json(url)?;
    let jobs = payload
        .get("jobs")
        .and_then(Value::as_array)
        .context("Ashby response did not contain jobs[]")?;
    Ok(jobs
        .iter()
        .filter_map(|value| map_ashby(value, board))
        .collect())
}

fn map_ashby(value: &Value, board: &str) -> Option<Job> {
    let title = text_field(value, "title")?;
    let source_url = text_field(value, "jobUrl")?;
    let canonical_url = text_field(value, "applyUrl").or_else(|| Some(source_url.clone()));
    let location = text_field(value, "location").unwrap_or_default();
    let description = text_field(value, "descriptionPlain").unwrap_or_default();
    let mut job = base_job(
        title,
        board.to_string(),
        location,
        "ashby",
        source_url,
        canonical_url,
        description,
    );
    job.employment_type = text_field(value, "employmentType");
    if let Some(workplace) = text_field(value, "workplaceType") {
        apply_workplace(&mut job, &workplace);
    }
    if let Some(remote) = value.get("isRemote").and_then(Value::as_bool) {
        job.remote = Some(remote);
        if job.work_mode == WorkMode::Unknown {
            job.work_mode = if remote {
                WorkMode::Remote
            } else {
                WorkMode::Onsite
            };
        }
    }
    job.posted = text_field(value, "publishedAt").unwrap_or_default();
    Some(job)
}

fn fetch_recruitee(company: &str) -> Result<Vec<Job>> {
    let url = Url::parse(&format!("https://{company}.recruitee.com/api/offers/"))?;
    let payload = get_json(url)?;
    let offers = payload
        .get("offers")
        .and_then(Value::as_array)
        .context("Recruitee response did not contain offers[]")?;
    Ok(offers
        .iter()
        .filter_map(|value| map_recruitee(value, company))
        .collect())
}

fn map_recruitee(value: &Value, company: &str) -> Option<Job> {
    let title = text_field(value, "title")?;
    let source_url = text_field(value, "careers_url").or_else(|| text_field(value, "url"))?;
    let location = location_text(value.get("location"));
    let description = text_field(value, "description")
        .map(|html| plain_html(&html))
        .unwrap_or_default();
    let mut job = base_job(
        title,
        company.to_string(),
        location,
        "recruitee",
        source_url.clone(),
        Some(source_url),
        description,
    );
    if let Some(remote) = value.get("remote").and_then(Value::as_bool) {
        job.remote = Some(remote);
        if remote {
            job.work_mode = WorkMode::Remote;
        }
    }
    job.employment_type =
        text_field(value, "employment_type").or_else(|| text_field(value, "employmentType"));
    job.posted = text_field(value, "published_at")
        .or_else(|| text_field(value, "publishedAt"))
        .unwrap_or_default();
    Some(job)
}

fn base_job(
    title: String,
    company: String,
    location: String,
    source: &str,
    source_url: String,
    canonical_url: Option<String>,
    description: String,
) -> Job {
    let id = job_fingerprint(&company, &title, &source_url);
    Job {
        id,
        title,
        company,
        location,
        remote: None,
        work_mode: WorkMode::Unknown,
        employment_type: None,
        status: ApplicationStatus::New,
        score: 0.0,
        salary_min: None,
        salary_max: None,
        currency: None,
        salary_source: None,
        source: source.into(),
        source_url,
        canonical_url,
        verification: "unverified".into(),
        verification_source: None,
        replacement_url: None,
        replacement_title: None,
        posted: String::new(),
        first_seen: String::new(),
        last_seen: String::new(),
        status_updated_at: None,
        status_manually_set: false,
        reasons: Vec::new(),
        concerns: Vec::new(),
        description,
        notes: String::new(),
    }
}

fn matches_terms(job: &Job, terms: &[String]) -> bool {
    let haystack = normalize_text(&format!("{} {}", job.title, job.description));
    terms.iter().any(|term| {
        contains_term(&haystack, term)
            || normalize_text(term)
                .split_whitespace()
                .all(|token| contains_term(&haystack, token))
    })
}

fn apply_workplace(job: &mut Job, value: &str) {
    let value = normalize_text(value).replace('-', " ");
    job.work_mode = match value.as_str() {
        "remote" => WorkMode::Remote,
        "hybrid" => WorkMode::Hybrid,
        "onsite" | "on site" => WorkMode::Onsite,
        _ => return,
    };
    job.remote = Some(job.work_mode == WorkMode::Remote);
}

fn plain_html(value: &str) -> String {
    let document = Html::parse_fragment(value);
    document
        .root_element()
        .text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn text_field(value: &Value, field: &str) -> Option<String> {
    value.get(field).and_then(value_text)
}

fn value_text(value: &Value) -> Option<String> {
    match value {
        Value::String(value) if !value.trim().is_empty() => Some(value.trim().to_string()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn location_text(value: Option<&Value>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    if let Some(value) = value_text(value) {
        return value;
    }
    let Some(object) = value.as_object() else {
        return String::new();
    };
    ["name", "city", "state", "country"]
        .iter()
        .filter_map(|field| object.get(*field).and_then(value_text))
        .collect::<Vec<_>>()
        .join(", ")
}

fn number(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_str()?.parse().ok())
}

fn timestamp_millis(value: i64) -> Option<String> {
    OffsetDateTime::from_unix_timestamp(value.div_euclid(1000))
        .ok()?
        .format(&Rfc3339)
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_config_is_optional_and_validated() {
        let parsed: ProviderFile =
            toml::from_str("[providers]\ngreenhouse=['acme']\nashby=['north-star']").unwrap();
        validate(&parsed.providers).unwrap();
        assert_eq!(parsed.providers.tasks().len(), 2);
        assert!(!valid_token("evil.example/path"));
    }

    #[test]
    fn greenhouse_mapping_keeps_public_listing_url() {
        let value = serde_json::json!({
            "title": "Backend Engineer",
            "absolute_url": "https://job-boards.greenhouse.io/acme/jobs/123",
            "location": {"name": "Italy - Remote"},
            "content": "<p>Build APIs with <b>Rust</b>.</p>",
            "departments": [{"name": "Engineering"}]
        });
        let job = map_greenhouse(&value, "acme").unwrap();
        assert_eq!(job.source, "greenhouse");
        assert_eq!(job.description, "Build APIs with Rust .");
        assert_eq!(job.canonical_url.as_deref(), Some(job.source_url.as_str()));
    }

    #[test]
    fn lever_mapping_uses_hosted_and_apply_urls_separately() {
        let value = serde_json::json!({
            "text": "Junior Backend Engineer",
            "hostedUrl": "https://jobs.lever.co/acme/abc",
            "applyUrl": "https://jobs.lever.co/acme/abc/apply",
            "descriptionPlain": "Python backend role",
            "categories": {"location": "Remote", "commitment": "Full-time"},
            "workplaceType": "remote",
            "createdAt": 1700000000000i64,
            "salaryRange": {"interval": "per-year-salary", "min": 50000, "max": 65000, "currency": "EUR"}
        });
        let job = map_lever(&value, "acme").unwrap();
        assert_eq!(job.source_url, "https://jobs.lever.co/acme/abc");
        assert_eq!(
            job.canonical_url.as_deref(),
            Some("https://jobs.lever.co/acme/abc/apply")
        );
        assert_eq!(job.work_mode, WorkMode::Remote);
        assert_eq!(job.salary_max, Some(65_000.0));
    }

    #[test]
    fn ashby_mapping_preserves_public_job_url() {
        let value = serde_json::json!({
            "title": "Graduate Engineer",
            "location": "Europe",
            "workplaceType": "Remote",
            "isRemote": true,
            "descriptionPlain": "Rust and PostgreSQL",
            "publishedAt": "2026-08-01T12:00:00Z",
            "jobUrl": "https://jobs.ashbyhq.com/acme/id",
            "applyUrl": "https://jobs.ashbyhq.com/acme/id/application"
        });
        let job = map_ashby(&value, "acme").unwrap();
        assert_eq!(job.work_mode, WorkMode::Remote);
        assert_eq!(job.source_url, "https://jobs.ashbyhq.com/acme/id");
    }

    #[test]
    fn search_terms_allow_same_words_in_different_order() {
        let mut job = base_job(
            "Software Engineer - Junior".into(),
            "Acme".into(),
            "Remote".into(),
            "lever",
            "https://jobs.lever.co/acme/id".into(),
            None,
            "Backend systems".into(),
        );
        job.remote = Some(true);
        assert!(matches_terms(&job, &["junior software engineer".into()]));
        assert!(!matches_terms(&job, &["data scientist".into()]));
    }
}
