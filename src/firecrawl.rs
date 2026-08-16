use std::{
    collections::{HashSet, VecDeque},
    env, fs,
    net::{Ipv4Addr, Ipv6Addr},
    path::Path,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::{Value, json};
use url::{Host, Url};

use crate::{
    identity::job_fingerprint,
    model::{ApplicationStatus, Job, WorkMode},
};

const API_ROOT: &str = "https://api.firecrawl.dev/v2";
const USER_AGENT: &str = "OpenJobScout/0.2 (+https://github.com/cmdr-chara/open-job-scout)";
const MAX_DESCRIPTION_CHARS: usize = 100_000;
const DEFAULT_SEARCH_LIMIT: usize = 8;
const DEFAULT_MAX_SCRAPES: usize = 16;
const DEFAULT_TIMEOUT_SECONDS: u64 = 45;
const DEFAULT_EXCLUDE_DOMAINS: &[&str] = &[
    "linkedin.com",
    "indeed.com",
    "glassdoor.com",
    "ziprecruiter.com",
    "greenhouse.io",
    "lever.co",
    "ashbyhq.com",
    "recruitee.com",
];

const EXTRACTION_PROMPT: &str = "Classify this public page as a single job posting, a careers/jobs index, or other. Use only facts visible on the page. Never infer missing company, salary, location, remote status, employment type, dates, or URLs. Salary fields are annual compensation only: include them only when the employer explicitly publishes annual values on the page; never estimate or annualize hourly, monthly, daily, or otherwise ambiguous compensation. For a single currently open job, return the normalized job fields and preserve the employer's description text without navigation/cookie/footer boilerplate. For a careers index, return public job-posting links visible on the page. Set requires_interaction only when ordinary public navigation or a load-more control is needed to reveal listings. Do not log in, fill an application, solve or bypass a CAPTCHA, or bypass any access control.";

const INTERACT_PROMPT: &str = "This is an explicitly allowed public careers page. Reveal job links only through normal public navigation or load-more controls. Do not log in, enter personal data, fill an application, solve or bypass a CAPTCHA, or bypass any access control. Return only JSON with this shape: {\"job_links\":[{\"title\":\"optional title\",\"url\":\"https://...\"}]}. If public job links cannot be revealed without a challenge or authentication, return {\"job_links\":[]}.";

#[derive(Debug, Clone, Deserialize)]
pub struct FirecrawlConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub search_enabled: bool,
    #[serde(default = "default_search_limit")]
    pub search_limit_per_term: usize,
    #[serde(default = "default_max_scrapes")]
    pub max_scrapes: usize,
    #[serde(default)]
    pub career_urls: Vec<String>,
    #[serde(default)]
    pub interact_urls: Vec<String>,
    #[serde(default)]
    pub include_domains: Vec<String>,
    #[serde(default = "default_exclude_domains")]
    pub exclude_domains: Vec<String>,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
    #[serde(default = "default_true")]
    pub zero_data_retention: bool,
}

impl Default for FirecrawlConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            search_enabled: true,
            search_limit_per_term: default_search_limit(),
            max_scrapes: default_max_scrapes(),
            career_urls: Vec::new(),
            interact_urls: Vec::new(),
            include_domains: Vec::new(),
            exclude_domains: default_exclude_domains(),
            timeout_seconds: default_timeout_seconds(),
            zero_data_retention: true,
        }
    }
}

#[derive(Debug, Default)]
pub struct FirecrawlBatch {
    pub jobs: Vec<Job>,
    pub errors: Vec<String>,
    pub searches: usize,
    pub scrapes: usize,
    pub interactions: usize,
    /// True after an empty search or at least one successful scrape.
    /// Counters alone cannot distinguish failed scrapes from a valid empty search.
    pub successful: bool,
}

#[derive(Debug, Deserialize)]
struct FirecrawlFile {
    #[serde(default)]
    firecrawl: FirecrawlConfig,
}

pub fn load(path: &Path) -> Result<FirecrawlConfig> {
    let source = fs::read_to_string(path)
        .with_context(|| format!("failed to read Firecrawl config {}", path.display()))?;
    let parsed: FirecrawlFile = toml::from_str(&source)
        .with_context(|| format!("failed to parse Firecrawl config {}", path.display()))?;
    validate(&parsed.firecrawl)?;
    Ok(parsed.firecrawl)
}

pub fn discover(
    config: &FirecrawlConfig,
    terms: &[String],
    location: &str,
) -> Result<FirecrawlBatch> {
    if !config.enabled {
        return Ok(FirecrawlBatch::default());
    }
    if !config.search_enabled && config.career_urls.is_empty() && config.interact_urls.is_empty() {
        bail!(
            "Firecrawl is enabled but has no discovery work; enable search or configure career_urls/interact_urls"
        );
    }
    let api_key = env::var("FIRECRAWL_API_KEY")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Firecrawl is enabled but FIRECRAWL_API_KEY is not set in the environment"
            )
        })?;
    let client = FirecrawlClient::new(api_key, config.timeout_seconds)?;
    discover_with_client(config, terms, location, &client)
}

fn discover_with_client(
    config: &FirecrawlConfig,
    terms: &[String],
    location: &str,
    client: &FirecrawlClient,
) -> Result<FirecrawlBatch> {
    let mut batch = FirecrawlBatch::default();
    let mut queue = VecDeque::new();
    let mut queued = HashSet::new();
    let mut scraped = HashSet::new();
    let interact_urls = config
        .interact_urls
        .iter()
        .filter_map(|value| public_http_url(value).map(normalized_url_key))
        .collect::<HashSet<_>>();

    for value in config.career_urls.iter().chain(&config.interact_urls) {
        enqueue(value, &mut queue, &mut queued, &scraped);
    }

    if config.search_enabled {
        for term in terms {
            let query = [term.trim(), location.trim(), "jobs careers"]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            if query.is_empty() {
                continue;
            }
            match client.search(&query, config) {
                Ok(results) => {
                    batch.searches += 1;
                    let mut valid_targets = 0usize;
                    for result in results {
                        if let Some(url) = result.get("url").and_then(Value::as_str) {
                            if public_http_url(url).is_some() {
                                valid_targets += 1;
                            }
                            enqueue(url, &mut queue, &mut queued, &scraped);
                        }
                    }
                    if valid_targets == 0 {
                        batch.successful = true;
                    }
                }
                Err(error) => batch.errors.push(format!("search {term:?}: {error:#}")),
            }
        }
    }

    let mut scrape_attempts = 0usize;
    while let Some(url) = queue.pop_front() {
        if !take_scrape_budget(&mut scrape_attempts, config.max_scrapes) {
            break;
        }
        let Some(parsed_url) = public_http_url(&url) else {
            continue;
        };
        let key = normalized_url_key(parsed_url);
        queued.remove(&key);
        if !scraped.insert(key.clone()) {
            continue;
        }

        let data = match client.scrape(&url, config) {
            Ok(data) => {
                batch.scrapes += 1;
                batch.successful = true;
                data
            }
            Err(error) => {
                batch.errors.push(format!("scrape {url}: {error:#}"));
                continue;
            }
        };
        let Some(extracted) = data.get("json") else {
            batch
                .errors
                .push(format!("scrape {url}: no structured job data returned"));
            continue;
        };

        if extracted.get("page_type").and_then(Value::as_str) == Some("job")
            && let Some(job) = job_from_extracted(extracted.get("job"), &url)
        {
            batch.jobs.push(job);
        }
        if let Some(links) = extracted.get("job_links").and_then(Value::as_array) {
            for link in links {
                if let Some(value) = link.get("url").and_then(Value::as_str) {
                    enqueue(value, &mut queue, &mut queued, &scraped);
                }
            }
        }

        if extracted
            .get("requires_interaction")
            .and_then(Value::as_bool)
            != Some(true)
        {
            continue;
        }
        if !interact_urls.contains(&key) {
            batch.errors.push(format!(
                "interaction required for {url}; add the exact URL to [firecrawl].interact_urls to opt in"
            ));
            continue;
        }
        let Some(scrape_id) = scrape_id(&data) else {
            batch.errors.push(format!(
                "interaction requested for {url}, but Firecrawl returned no valid scrapeId"
            ));
            continue;
        };
        match client.interact(&scrape_id, config) {
            Ok(links) => {
                batch.interactions += 1;
                for link in links {
                    if let Some(value) = link.get("url").and_then(Value::as_str) {
                        enqueue(value, &mut queue, &mut queued, &scraped);
                    }
                }
            }
            Err(error) => batch.errors.push(format!("interact {url}: {error:#}")),
        }
        client.stop_interaction(&scrape_id);
    }

    Ok(batch)
}

struct FirecrawlClient {
    api_key: String,
    client: Client,
}

impl FirecrawlClient {
    fn new(api_key: String, timeout_seconds: u64) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_seconds))
            .user_agent(USER_AGENT)
            .build()
            .context("failed to create Firecrawl HTTP client")?;
        Ok(Self { api_key, client })
    }

    fn post(&self, path: &str, payload: &Value) -> Result<Value> {
        let response = self
            .client
            .post(format!("{API_ROOT}{path}"))
            .bearer_auth(&self.api_key)
            .json(payload)
            .send()
            .context("Firecrawl request failed")?;
        let status = response.status();
        let value: Value = response.json().context("Firecrawl returned invalid JSON")?;
        if !status.is_success() || value.get("success").and_then(Value::as_bool) == Some(false) {
            let detail = value
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("request failed");
            bail!("Firecrawl returned {status}: {}", truncate(detail, 200));
        }
        Ok(value)
    }

    fn search(&self, query: &str, config: &FirecrawlConfig) -> Result<Vec<Value>> {
        let mut payload = json!({
            "query": query,
            "limit": config.search_limit_per_term,
            "sources": ["web"],
            "safe": true,
            "timeout": config.timeout_seconds * 1_000,
            "ignoreInvalidURLs": true,
        });
        let object = payload
            .as_object_mut()
            .expect("Firecrawl search payload must be an object");
        if !config.include_domains.is_empty() {
            object.insert("includeDomains".into(), json!(config.include_domains));
        } else if !config.exclude_domains.is_empty() {
            object.insert("excludeDomains".into(), json!(config.exclude_domains));
        }
        let response = self.post("/search", &payload)?;
        Ok(response
            .get("data")
            .and_then(|data| data.get("web"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
    }

    fn scrape(&self, url: &str, config: &FirecrawlConfig) -> Result<Value> {
        let response = self.post(
            "/scrape",
            &json!({
                "url": url,
                "formats": [{
                    "type": "json",
                    "prompt": EXTRACTION_PROMPT,
                    "schema": job_schema(),
                }],
                "onlyMainContent": true,
                "removeBase64Images": true,
                "blockAds": true,
                "zeroDataRetention": config.zero_data_retention,
                "timeout": config.timeout_seconds * 1_000,
            }),
        )?;
        Ok(response.get("data").cloned().unwrap_or(Value::Null))
    }

    fn interact(&self, scrape_id: &str, config: &FirecrawlConfig) -> Result<Vec<Value>> {
        let response = self.post(
            &format!("/scrape/{scrape_id}/interact"),
            &json!({
                "prompt": INTERACT_PROMPT,
                "timeout": config.timeout_seconds,
            }),
        )?;
        let Some(output) = response.get("output").and_then(Value::as_str) else {
            return Ok(Vec::new());
        };
        let parsed = parse_json_text(output);
        Ok(parsed
            .get("job_links")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
    }

    fn stop_interaction(&self, scrape_id: &str) {
        if let Ok(response) = self
            .client
            .delete(format!("{API_ROOT}/scrape/{scrape_id}/interact"))
            .bearer_auth(&self.api_key)
            .send()
        {
            let _ = response.error_for_status();
        }
    }
}

fn job_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "page_type": {"type": "string", "enum": ["job", "careers", "other"]},
            "requires_interaction": {"type": "boolean"},
            "job": {
                "type": ["object", "null"],
                "properties": {
                    "title": {"type": ["string", "null"]},
                    "company": {"type": ["string", "null"]},
                    "location": {"type": ["string", "null"]},
                    "remote": {"type": ["boolean", "null"]},
                    "work_mode": {"type": ["string", "null"], "enum": ["remote", "hybrid", "onsite", "unknown", null]},
                    "employment_type": {"type": ["string", "null"]},
                    "salary_min": {"type": ["number", "null"]},
                    "salary_max": {"type": ["number", "null"]},
                    "currency": {"type": ["string", "null"]},
                    "posted_at": {"type": ["string", "null"]},
                    "canonical_url": {"type": ["string", "null"]},
                    "description": {"type": ["string", "null"]},
                }
            },
            "job_links": {
                "type": "array",
                "maxItems": 50,
                "items": {
                    "type": "object",
                    "properties": {
                        "title": {"type": ["string", "null"]},
                        "url": {"type": "string"}
                    },
                    "required": ["url"]
                }
            }
        },
        "required": ["page_type", "requires_interaction", "job_links"]
    })
}

fn job_from_extracted(value: Option<&Value>, source_url: &str) -> Option<Job> {
    let value = value?.as_object()?;
    let title = string_value(value.get("title"))?;
    let company = string_value(value.get("company"))?;
    let source_url = public_http_url(source_url)?.to_string();
    let canonical_url = value
        .get("canonical_url")
        .and_then(Value::as_str)
        .and_then(public_http_url)
        .map(|url| url.to_string())
        .unwrap_or_else(|| source_url.clone());
    let work_mode = match value
        .get("work_mode")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "remote" => WorkMode::Remote,
        "hybrid" => WorkMode::Hybrid,
        "onsite" | "on-site" => WorkMode::Onsite,
        _ => WorkMode::Unknown,
    };
    let mut remote = value.get("remote").and_then(Value::as_bool);
    if remote.is_none() {
        remote = match work_mode {
            WorkMode::Remote => Some(true),
            WorkMode::Onsite => Some(false),
            WorkMode::Hybrid | WorkMode::Unknown => None,
        };
    }
    let salary_min = nonnegative_number(value.get("salary_min"));
    let salary_max = nonnegative_number(value.get("salary_max"));
    let description = value
        .get("description")
        .and_then(Value::as_str)
        .map(|value| value.trim().chars().take(MAX_DESCRIPTION_CHARS).collect())
        .unwrap_or_default();
    let id = job_fingerprint(&company, &title, &source_url);
    Some(Job {
        id,
        title,
        company,
        location: optional_string(value.get("location")).unwrap_or_default(),
        remote,
        work_mode,
        employment_type: optional_string(value.get("employment_type")),
        status: ApplicationStatus::New,
        score: 0.0,
        salary_min,
        salary_max,
        currency: optional_string(value.get("currency")),
        salary_source: (salary_min.is_some() || salary_max.is_some()).then(|| "firecrawl".into()),
        source: "firecrawl".into(),
        source_url,
        canonical_url: Some(canonical_url),
        verification: "unverified".into(),
        verification_source: None,
        replacement_url: None,
        replacement_title: None,
        posted: optional_string(value.get("posted_at")).unwrap_or_default(),
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

fn validate(config: &FirecrawlConfig) -> Result<()> {
    if !(1..=50).contains(&config.search_limit_per_term) {
        bail!("[firecrawl].search_limit_per_term must be between 1 and 50");
    }
    if !(1..=100).contains(&config.max_scrapes) {
        bail!("[firecrawl].max_scrapes must be between 1 and 100");
    }
    if !(5..=300).contains(&config.timeout_seconds) {
        bail!("[firecrawl].timeout_seconds must be between 5 and 300");
    }
    for (key, values) in [
        ("career_urls", &config.career_urls),
        ("interact_urls", &config.interact_urls),
    ] {
        for value in values {
            if public_http_url(value).is_none() {
                bail!("[firecrawl].{key} contains an unsafe or invalid public HTTP(S) URL");
            }
        }
    }
    for (key, values) in [
        ("include_domains", &config.include_domains),
        ("exclude_domains", &config.exclude_domains),
    ] {
        for value in values {
            if !valid_domain(value) {
                bail!("[firecrawl].{key} contains invalid hostname {value:?}");
            }
        }
    }
    Ok(())
}

fn enqueue(
    value: &str,
    queue: &mut VecDeque<String>,
    queued: &mut HashSet<String>,
    scraped: &HashSet<String>,
) {
    let Some(url) = public_http_url(value) else {
        return;
    };
    let key = normalized_url_key(url.clone());
    if !scraped.contains(&key) && queued.insert(key) {
        queue.push_back(url.to_string());
    }
}

fn public_http_url(value: &str) -> Option<Url> {
    let url = Url::parse(value.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    match url.host()? {
        Host::Domain(host) => {
            let host = host.trim_end_matches('.').to_ascii_lowercase();
            if host == "localhost"
                || host.ends_with(".localhost")
                || host.ends_with(".local")
                || host.ends_with(".internal")
                || looks_like_numeric_host(&host)
            {
                return None;
            }
        }
        Host::Ipv4(address) if !public_ipv4(address) => return None,
        Host::Ipv6(address) if !public_ipv6(address) => return None,
        Host::Ipv4(_) | Host::Ipv6(_) => {}
    }
    Some(url)
}

fn looks_like_numeric_host(host: &str) -> bool {
    host.as_bytes().first().is_some_and(u8::is_ascii_digit)
        && host
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'.' | b'x' | b'a'..=b'f'))
}

fn take_scrape_budget(attempts: &mut usize, maximum: usize) -> bool {
    if *attempts >= maximum {
        return false;
    }
    *attempts += 1;
    true
}

fn public_ipv4(address: Ipv4Addr) -> bool {
    !(address.is_private()
        || address.is_loopback()
        || address.is_link_local()
        || address.is_broadcast()
        || address.is_documentation()
        || address.is_multicast()
        || address.is_unspecified())
}

fn public_ipv6(address: Ipv6Addr) -> bool {
    !(address.is_loopback()
        || address.is_unspecified()
        || address.is_multicast()
        || address.is_unique_local()
        || address.is_unicast_link_local())
}

fn valid_domain(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty()
        || value.chars().any(char::is_whitespace)
        || value.contains("//")
        || value.contains('/')
        || value.contains(':')
    {
        return false;
    }
    public_http_url(&format!("https://{value}/")).is_some()
}

fn normalized_url_key(mut url: Url) -> String {
    url.set_fragment(None);
    if url.path().len() > 1 {
        let path = url.path().trim_end_matches('/').to_owned();
        url.set_path(&path);
    }
    url.to_string()
}

fn scrape_id(data: &Value) -> Option<String> {
    let metadata = data.get("metadata")?.as_object()?;
    metadata
        .get("scrapeId")
        .or_else(|| metadata.get("scrape_id"))
        .and_then(Value::as_str)
        .filter(|value| valid_scrape_id(value))
        .map(str::to_owned)
}

fn valid_scrape_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn parse_json_text(value: &str) -> Value {
    let mut value = value.trim();
    if let Some(stripped) = value.strip_prefix("```json") {
        value = stripped.trim_start();
    } else if let Some(stripped) = value.strip_prefix("```") {
        value = stripped.trim_start();
    }
    if let Some(stripped) = value.strip_suffix("```") {
        value = stripped.trim_end();
    }
    serde_json::from_str(value).unwrap_or(Value::Null)
}

fn string_value(value: Option<&Value>) -> Option<String> {
    optional_string(value).filter(|value| !value.is_empty())
}

fn optional_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn nonnegative_number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value >= 0.0)
}

fn truncate(value: &str, maximum: usize) -> String {
    value.chars().take(maximum).collect()
}

fn default_true() -> bool {
    true
}

const fn default_search_limit() -> usize {
    DEFAULT_SEARCH_LIMIT
}

const fn default_max_scrapes() -> usize {
    DEFAULT_MAX_SCRAPES
}

const fn default_timeout_seconds() -> u64 {
    DEFAULT_TIMEOUT_SECONDS
}

fn default_exclude_domains() -> Vec<String> {
    DEFAULT_EXCLUDE_DOMAINS
        .iter()
        .map(|value| (*value).to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_keep_firecrawl_disabled_and_ats_domains_excluded() {
        let config = FirecrawlConfig::default();
        assert!(!config.enabled);
        assert!(config.search_enabled);
        assert!(config.exclude_domains.contains(&"greenhouse.io".into()));
        assert!(config.exclude_domains.contains(&"lever.co".into()));
    }

    #[test]
    fn extracted_job_uses_only_normalized_fields() {
        let value = json!({
            "title": "Backend Engineer",
            "company": "Example",
            "location": "Milan",
            "work_mode": "hybrid",
            "salary_min": 45000,
            "salary_max": 55000,
            "currency": "EUR",
            "description": "Build APIs",
            "canonical_url": "https://example.com/jobs/1"
        });
        let job = job_from_extracted(Some(&value), "https://example.com/jobs/1").unwrap();
        assert_eq!(job.source, "firecrawl");
        assert_eq!(job.company, "Example");
        assert_eq!(job.work_mode, WorkMode::Hybrid);
        assert_eq!(job.salary_source.as_deref(), Some("firecrawl"));
        assert_eq!(job.description, "Build APIs");
    }

    #[test]
    fn public_url_guard_rejects_local_and_private_targets() {
        assert!(public_http_url("https://example.com/careers").is_some());
        assert!(public_http_url("http://127.0.0.1/jobs").is_none());
        assert!(public_http_url("http://127.1/jobs").is_none());
        assert!(public_http_url("http://2130706433/jobs").is_none());
        assert!(public_http_url("http://0x7f000001/jobs").is_none());
        assert!(public_http_url("http://10.0.0.1/jobs").is_none());
        assert!(public_http_url("https://localhost/jobs").is_none());
        assert!(public_http_url("file:///tmp/jobs").is_none());
    }

    #[test]
    fn scrape_attempt_budget_counts_failed_attempts() {
        let mut attempts = 0;
        assert!(take_scrape_budget(&mut attempts, 2));
        assert!(take_scrape_budget(&mut attempts, 2));
        assert!(!take_scrape_budget(&mut attempts, 2));
        assert_eq!(attempts, 2);
    }

    #[test]
    fn interact_output_accepts_json_fences_only_as_transport_wrapping() {
        let value = parse_json_text("```json\n{\"job_links\":[]}\n```");
        assert_eq!(value["job_links"], json!([]));
    }

    #[test]
    fn scrape_ids_are_path_safe() {
        assert!(valid_scrape_id("550e8400-e29b-41d4-a716-446655440000"));
        assert!(!valid_scrape_id("../escape"));
        assert!(!valid_scrape_id("id/child"));
    }

    #[test]
    fn ip_address_public_checks_reject_non_global_ranges() {
        assert!(!public_ipv4(Ipv4Addr::new(192, 168, 1, 1)));
        assert!(!public_ipv6("::1".parse().unwrap()));
    }
}
