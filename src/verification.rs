use std::{
    collections::VecDeque,
    fmt,
    io::Read,
    net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs},
    sync::{Arc, Mutex, OnceLock, mpsc},
    thread,
    time::Duration,
};

use percent_encoding::percent_decode_str;
use reqwest::{
    StatusCode,
    blocking::{Client, Response},
    header::{ACCEPT, CONTENT_LENGTH, CONTENT_TYPE, LOCATION},
    redirect::Policy,
};
use scraper::{Html, Selector};
use serde_json::Value;
use url::{Host, Url};

use crate::{
    model::{Job, WorkMode},
    ranking::normalize_text,
};

const USER_AGENT: &str = "OpenJobScout/0.2 (+https://github.com/cmdr-chara/open-job-scout)";
const MAX_HTML_BYTES: usize = 1_000_000;
const MAX_JSON_BYTES: usize = 5_000_000;
const MAX_REDIRECTS: usize = 10;
const DNS_RESOLVER_WORKERS: usize = 4;
const DNS_RESOLVER_QUEUE: usize = 8;
const CLOSED_MARKERS: &[&str] = &[
    "job not found",
    "job is no longer available",
    "job no longer available",
    "position is no longer available",
    "position no longer available",
    "this job has expired",
    "this position has been filled",
    "no longer accepting applications",
    "vacancy has been filled",
    "offerta non disponibile",
    "posizione non è più disponibile",
    "annuncio non è più disponibile",
    "non accetta più candidature",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PageStatus {
    Reachable,
    Closed,
    Unreachable,
}

impl PageStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Reachable => "reachable",
            Self::Closed => "closed",
            Self::Unreachable => "unreachable",
        }
    }
}

#[derive(Debug)]
struct ResolveResult {
    status: PageStatus,
    resolved: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Ats {
    Greenhouse {
        board: String,
        job_id: String,
    },
    Lever {
        region: LeverRegion,
        site: String,
        posting: String,
    },
    Ashby {
        board: String,
        posting: String,
    },
    Recruitee {
        company: String,
        slug: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LeverRegion {
    Global,
    Eu,
}

impl Ats {
    const fn provider(&self) -> &'static str {
        match self {
            Self::Greenhouse { .. } => "greenhouse",
            Self::Lever { .. } => "lever",
            Self::Ashby { .. } => "ashby",
            Self::Recruitee { .. } => "recruitee",
        }
    }
}

#[derive(Debug)]
pub(crate) enum FetchError {
    Unsafe(String),
    Transport(String),
    DnsTimeout,
    Http(u16),
    TooLarge,
    Invalid(String),
}

impl fmt::Display for FetchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsafe(message) | Self::Transport(message) | Self::Invalid(message) => {
                formatter.write_str(message)
            }
            Self::DnsTimeout => formatter.write_str("DNS resolution timed out"),
            Self::Http(status) => write!(formatter, "HTTP {status}"),
            Self::TooLarge => formatter.write_str("response exceeds the allowed size"),
        }
    }
}

impl std::error::Error for FetchError {}

#[derive(Debug)]
enum ProviderError {
    Fetch(FetchError),
    Missing,
}

impl From<FetchError> for ProviderError {
    fn from(error: FetchError) -> Self {
        Self::Fetch(error)
    }
}

#[derive(Debug)]
struct ValidatedTarget {
    url: Url,
    domain: Option<String>,
    addresses: Vec<SocketAddr>,
}

struct DnsResolverPool {
    sender: mpsc::SyncSender<DnsResolutionRequest>,
}

struct DnsResolutionRequest {
    domain: String,
    port: u16,
    response: mpsc::Sender<Result<Vec<SocketAddr>, String>>,
}

#[cfg(test)]
pub fn is_safe_public_url(value: &str) -> bool {
    validate_target(value).is_ok()
}

pub fn page_indicates_closed(html: &str) -> bool {
    let document = Html::parse_document(html);
    let headings_selector = Selector::parse("title, h1, h2").expect("static selector is valid");
    let headings = normalize_text(
        &document
            .select(&headings_selector)
            .flat_map(|element| element.text())
            .collect::<Vec<_>>()
            .join(" "),
    );
    if !headings.is_empty() {
        return CLOSED_MARKERS
            .iter()
            .any(|marker| headings.contains(marker));
    }

    let body_selector = Selector::parse("body").expect("static selector is valid");
    let text = document
        .select(&body_selector)
        .next()
        .map(|body| body.text().collect::<Vec<_>>().join(" "))
        .unwrap_or_else(|| document.root_element().text().collect::<Vec<_>>().join(" "));
    let text = normalize_text(&text);
    text.len() <= 280 && CLOSED_MARKERS.iter().any(|marker| text.contains(marker))
}

pub fn verify_job(mut job: Job) -> Job {
    let mut targets = Vec::new();
    for candidate in [job.canonical_url.as_deref(), Some(job.source_url.as_str())]
        .into_iter()
        .flatten()
    {
        if !candidate.trim().is_empty() && !targets.iter().any(|value| value == candidate) {
            targets.push(candidate.to_string());
        }
    }

    let mut status = PageStatus::Unreachable;
    let mut resolved = job.source_url.clone();
    for target in &targets {
        let result = resolve_url(target, 8);
        status = result.status;
        resolved = result.resolved;
        if status != PageStatus::Unreachable {
            break;
        }
    }
    job.verification = status.as_str().into();
    if status == PageStatus::Reachable {
        job.canonical_url = Some(resolved.clone());
    }

    let detected = detect_ats(&resolved).or_else(|| targets.iter().find_map(|url| detect_ats(url)));
    let Some(ats) = detected else {
        return job;
    };
    let provider = ats.provider();
    match verify_ats(&mut job, &ats, &resolved) {
        Ok(()) => {
            job.verification = "verified".into();
            job.verification_source = Some(provider.into());
        }
        Err(ProviderError::Missing) => {
            job.verification = "closed".into();
            job.verification_source = Some(provider.into());
        }
        Err(ProviderError::Fetch(FetchError::Http(status))) if status == 404 || status == 410 => {
            job.verification = "closed".into();
            job.verification_source = Some(provider.into());
        }
        Err(ProviderError::Fetch(_)) => {}
    }
    job
}

pub fn verify_jobs(jobs: Vec<Job>, workers: usize) -> Vec<Job> {
    if jobs.len() <= 1 || workers <= 1 {
        return jobs.into_iter().map(verify_job).collect();
    }
    let worker_count = workers.max(1).min(jobs.len());
    let queue = Arc::new(Mutex::new(VecDeque::from_iter(
        jobs.into_iter().enumerate(),
    )));
    let (sender, receiver) = mpsc::channel();
    thread::scope(|scope| {
        for _ in 0..worker_count {
            let queue = Arc::clone(&queue);
            let sender = sender.clone();
            scope.spawn(move || {
                loop {
                    let item = queue
                        .lock()
                        .expect("verification queue poisoned")
                        .pop_front();
                    let Some((index, job)) = item else {
                        break;
                    };
                    if sender.send((index, verify_job(job))).is_err() {
                        break;
                    }
                }
            });
        }
        drop(sender);
    });
    let mut results = receiver.into_iter().collect::<Vec<_>>();
    results.sort_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, job)| job).collect()
}

fn resolve_url(value: &str, timeout_seconds: u64) -> ResolveResult {
    let original = value.to_string();
    let Ok(mut current) = Url::parse(value) else {
        return ResolveResult {
            status: PageStatus::Unreachable,
            resolved: original,
        };
    };

    for _ in 0..=MAX_REDIRECTS {
        let target =
            match validate_parsed_target(current.clone(), Duration::from_secs(timeout_seconds)) {
                Ok(target) => target,
                Err(_) => {
                    return ResolveResult {
                        status: PageStatus::Unreachable,
                        resolved: original,
                    };
                }
            };
        current = target.url.clone();
        let response = match send(&target, timeout_seconds, None) {
            Ok(response) => response,
            Err(_) => {
                return ResolveResult {
                    status: PageStatus::Unreachable,
                    resolved: original,
                };
            }
        };
        let status = response.status();
        if status.is_redirection() {
            let Some(location) = header_string(response.headers().get(LOCATION)) else {
                return ResolveResult {
                    status: PageStatus::Unreachable,
                    resolved: original,
                };
            };
            match current.join(&location) {
                Ok(next) => {
                    current = next;
                    continue;
                }
                Err(_) => {
                    return ResolveResult {
                        status: PageStatus::Unreachable,
                        resolved: original,
                    };
                }
            }
        }
        if matches!(status, StatusCode::NOT_FOUND | StatusCode::GONE) {
            return ResolveResult {
                status: PageStatus::Closed,
                resolved: current.to_string(),
            };
        }
        if !status.is_success() {
            return ResolveResult {
                status: PageStatus::Unreachable,
                resolved: original,
            };
        }

        let content_type = header_string(response.headers().get(CONTENT_TYPE)).unwrap_or_default();
        let is_html = content_type
            .split(';')
            .next()
            .is_some_and(|value| matches!(value.trim(), "text/html" | "application/xhtml+xml"));
        if !is_html {
            return ResolveResult {
                status: PageStatus::Reachable,
                resolved: current.to_string(),
            };
        }
        let body = match read_limited(response, MAX_HTML_BYTES) {
            Ok(body) => body,
            Err(_) => {
                return ResolveResult {
                    status: PageStatus::Unreachable,
                    resolved: original,
                };
            }
        };
        let html = String::from_utf8_lossy(&body);
        return ResolveResult {
            status: if page_indicates_closed(&html) {
                PageStatus::Closed
            } else {
                PageStatus::Reachable
            },
            resolved: current.to_string(),
        };
    }
    ResolveResult {
        status: PageStatus::Unreachable,
        resolved: original,
    }
}

fn request_json(mut current: Url, timeout_seconds: u64) -> Result<Value, FetchError> {
    for _ in 0..=MAX_REDIRECTS {
        let target = validate_parsed_target(current, Duration::from_secs(timeout_seconds))?;
        current = target.url.clone();
        let response = send(&target, timeout_seconds, Some("application/json"))?;
        let status = response.status();
        if status.is_redirection() {
            let location = header_string(response.headers().get(LOCATION))
                .ok_or_else(|| FetchError::Invalid("redirect is missing Location".into()))?;
            current = current
                .join(&location)
                .map_err(|error| FetchError::Invalid(error.to_string()))?;
            continue;
        }
        if !status.is_success() {
            return Err(FetchError::Http(status.as_u16()));
        }
        let body = read_limited(response, MAX_JSON_BYTES)?;
        return serde_json::from_slice(&body)
            .map_err(|error| FetchError::Invalid(format!("invalid JSON response: {error}")));
    }
    Err(FetchError::Invalid("too many redirects".into()))
}

pub(crate) fn request_json_for_provider(url: Url) -> Result<Value, FetchError> {
    request_json(url, 20)
}

#[cfg(test)]
fn validate_target(value: &str) -> Result<ValidatedTarget, FetchError> {
    let parsed = Url::parse(value).map_err(|error| FetchError::Unsafe(error.to_string()))?;
    validate_parsed_target(parsed, Duration::from_secs(8))
}

fn validate_parsed_target(
    mut url: Url,
    dns_timeout: Duration,
) -> Result<ValidatedTarget, FetchError> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(FetchError::Unsafe("target is not HTTP(S)".into()));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(FetchError::Unsafe("URL credentials are not allowed".into()));
    }
    let port = url
        .port_or_known_default()
        .ok_or_else(|| FetchError::Unsafe("URL has no usable port".into()))?;
    let host = url
        .host()
        .ok_or_else(|| FetchError::Unsafe("URL has no host".into()))?;
    match host {
        Host::Ipv4(address) => {
            if !is_public_ip(IpAddr::V4(address)) {
                return Err(FetchError::Unsafe("target IP is not public".into()));
            }
            Ok(ValidatedTarget {
                url,
                domain: None,
                addresses: Vec::new(),
            })
        }
        Host::Ipv6(address) => {
            if !is_public_ip(IpAddr::V6(address)) {
                return Err(FetchError::Unsafe("target IP is not public".into()));
            }
            Ok(ValidatedTarget {
                url,
                domain: None,
                addresses: Vec::new(),
            })
        }
        Host::Domain(domain) => {
            let domain = domain.trim_end_matches('.').to_ascii_lowercase();
            if domain == "localhost" || domain.ends_with(".localhost") || domain.is_empty() {
                return Err(FetchError::Unsafe(
                    "localhost is not a public target".into(),
                ));
            }
            if url.host_str().is_some_and(|host| host.ends_with('.')) {
                url.set_host(Some(&domain))
                    .map_err(|_| FetchError::Unsafe("invalid normalized host".into()))?;
            }
            let mut addresses = resolve_host_with_deadline(&domain, port, dns_timeout)?;
            addresses.sort_unstable();
            addresses.dedup();
            if addresses.is_empty() || addresses.iter().any(|address| !is_public_ip(address.ip())) {
                return Err(FetchError::Unsafe(
                    "hostname resolves to a non-public address".into(),
                ));
            }
            Ok(ValidatedTarget {
                url,
                domain: Some(domain),
                addresses,
            })
        }
    }
}

fn resolve_host_with_deadline(
    domain: &str,
    port: u16,
    timeout: Duration,
) -> Result<Vec<SocketAddr>, FetchError> {
    let pool = dns_resolver_pool()?;
    let (sender, receiver) = mpsc::channel();
    let request = DnsResolutionRequest {
        domain: domain.to_owned(),
        port,
        response: sender,
    };
    pool.sender.try_send(request).map_err(|error| match error {
        mpsc::TrySendError::Full(_) => FetchError::Transport("DNS resolver pool is busy".into()),
        mpsc::TrySendError::Disconnected(_) => {
            FetchError::Transport("DNS resolver pool is unavailable".into())
        }
    })?;
    receive_dns_result(receiver, timeout)
}

fn receive_dns_result(
    receiver: mpsc::Receiver<Result<Vec<SocketAddr>, String>>,
    timeout: Duration,
) -> Result<Vec<SocketAddr>, FetchError> {
    match receiver.recv_timeout(timeout) {
        Ok(Ok(addresses)) => Ok(addresses),
        Ok(Err(error)) => Err(FetchError::Unsafe(format!(
            "DNS resolution failed: {error}"
        ))),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(FetchError::DnsTimeout),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err(FetchError::Transport("DNS resolver worker stopped".into()))
        }
    }
}

fn dns_resolver_pool() -> Result<&'static DnsResolverPool, FetchError> {
    static POOL: OnceLock<Result<DnsResolverPool, String>> = OnceLock::new();

    POOL.get_or_init(|| {
        let (sender, receiver) = mpsc::sync_channel::<DnsResolutionRequest>(DNS_RESOLVER_QUEUE);
        let receiver = Arc::new(Mutex::new(receiver));
        for index in 0..DNS_RESOLVER_WORKERS {
            let receiver = Arc::clone(&receiver);
            thread::Builder::new()
                .name(format!("jobscout-dns-{index}"))
                .spawn(move || {
                    loop {
                        let request = match receiver.lock() {
                            Ok(receiver) => receiver.recv(),
                            Err(_) => return,
                        };
                        let Ok(request) = request else {
                            return;
                        };
                        let result = (request.domain.as_str(), request.port)
                            .to_socket_addrs()
                            .map(|addresses| addresses.collect::<Vec<_>>())
                            .map_err(|error| error.to_string());
                        let _ = request.response.send(result);
                    }
                })
                .map_err(|error| format!("failed to start DNS resolver worker: {error}"))?;
        }
        Ok(DnsResolverPool { sender })
    })
    .as_ref()
    .map_err(|error| FetchError::Transport(error.clone()))
}

fn is_public_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_public_ipv4(address),
        IpAddr::V6(address) => {
            let segments = address.segments();
            if address.is_unspecified()
                || address.is_loopback()
                || address.is_multicast()
                || segments[0] & 0xfe00 == 0xfc00
                || segments[0] & 0xffc0 == 0xfe80
                || segments[0] & 0xffc0 == 0xfec0
                || (segments[0] == 0x2001 && segments[1] == 0x0db8)
                || (segments[0] == 0x2001 && segments[1] == 0x0002 && segments[2] == 0)
                || (segments[0] == 0x0100
                    && segments[1] == 0
                    && segments[2] == 0
                    && segments[3] == 0)
                // Reject the well-known NAT64 prefix. Otherwise a translated
                // private IPv4 destination could pass the IPv6 classifier.
                || (segments[0] == 0x0064 && segments[1] == 0xff9b)
            {
                return false;
            }
            // Reject IPv4-mapped/compatible literals; ordinary public IPv4 is accepted directly.
            if segments[..5] == [0, 0, 0, 0, 0] && (segments[5] == 0 || segments[5] == 0xffff) {
                return false;
            }
            true
        }
    }
}

fn is_public_ipv4(address: Ipv4Addr) -> bool {
    let [a, b, c, d] = address.octets();
    if a == 0
        || a == 10
        || a == 127
        || a >= 224
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 168)
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 0 && c == 2)
        || (a == 192 && b == 88 && c == 99)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
        || (a == 255 && b == 255 && c == 255 && d == 255)
    {
        return false;
    }
    true
}

fn send(
    target: &ValidatedTarget,
    timeout_seconds: u64,
    accept: Option<&str>,
) -> Result<Response, FetchError> {
    let mut builder = Client::builder()
        .redirect(Policy::none())
        .no_proxy()
        .timeout(Duration::from_secs(timeout_seconds))
        .connect_timeout(Duration::from_secs(timeout_seconds))
        .user_agent(USER_AGENT);
    if let Some(domain) = &target.domain {
        builder = builder.resolve_to_addrs(domain, &target.addresses);
    }
    let client = builder
        .build()
        .map_err(|error| FetchError::Transport(error.to_string()))?;
    let mut request = client.get(target.url.clone());
    if let Some(accept) = accept {
        request = request.header(ACCEPT, accept);
    }
    request
        .send()
        .map_err(|error| FetchError::Transport(error.to_string()))
}

fn read_limited(mut response: Response, maximum: usize) -> Result<Vec<u8>, FetchError> {
    if let Some(length) = header_string(response.headers().get(CONTENT_LENGTH))
        .and_then(|value| value.parse::<u64>().ok())
        && length > maximum as u64
    {
        return Err(FetchError::TooLarge);
    }
    let mut body = Vec::with_capacity(maximum.min(64 * 1024));
    response
        .by_ref()
        .take(maximum as u64 + 1)
        .read_to_end(&mut body)
        .map_err(|error| FetchError::Transport(error.to_string()))?;
    if body.len() > maximum {
        return Err(FetchError::TooLarge);
    }
    Ok(body)
}

fn header_string(value: Option<&reqwest::header::HeaderValue>) -> Option<String> {
    value?.to_str().ok().map(str::to_string)
}

fn detect_ats(value: &str) -> Option<Ats> {
    let url = Url::parse(value).ok()?;
    let host = url.host_str()?.trim_end_matches('.').to_ascii_lowercase();
    let segments = decoded_segments(&url);
    if host == "greenhouse.io" || host.ends_with(".greenhouse.io") {
        let query_id = url
            .query_pairs()
            .find(|(key, _)| key == "gh_jid")
            .map(|(_, value)| value.into_owned());
        if let (Some(job_id), Some(board)) = (query_id, segments.first()) {
            return Some(Ats::Greenhouse {
                board: board.clone(),
                job_id,
            });
        }
        if segments.len() >= 3 && segments[segments.len() - 2] == "jobs" {
            return Some(Ats::Greenhouse {
                board: segments[segments.len() - 3].clone(),
                job_id: segments.last()?.clone(),
            });
        }
    }
    if matches!(host.as_str(), "jobs.lever.co" | "jobs.eu.lever.co") && segments.len() >= 2 {
        return Some(Ats::Lever {
            region: if host.starts_with("jobs.eu") {
                LeverRegion::Eu
            } else {
                LeverRegion::Global
            },
            site: segments[0].clone(),
            posting: segments[1].clone(),
        });
    }
    if host == "jobs.ashbyhq.com" && segments.len() >= 2 {
        return Some(Ats::Ashby {
            board: segments[0].clone(),
            posting: segments[1].clone(),
        });
    }
    if let Some(company) = host.strip_suffix(".recruitee.com")
        && !company.is_empty()
        && segments.len() >= 2
        && segments[segments.len() - 2] == "o"
    {
        return Some(Ats::Recruitee {
            company: company.into(),
            slug: segments.last()?.clone(),
        });
    }
    None
}

fn decoded_segments(url: &Url) -> Vec<String> {
    url.path_segments()
        .into_iter()
        .flatten()
        .filter(|segment| !segment.is_empty())
        .map(|segment| percent_decode_str(segment).decode_utf8_lossy().into_owned())
        .collect()
}

fn verify_ats(job: &mut Job, ats: &Ats, resolved: &str) -> Result<(), ProviderError> {
    match ats {
        Ats::Greenhouse { board, job_id } => {
            let mut api = build_api_url(
                "https://boards-api.greenhouse.io/v1/boards",
                &[board, "jobs", job_id],
            )?;
            api.query_pairs_mut()
                .append_pair("pay_transparency", "true");
            let payload = request_json(api, 10)?;
            job.canonical_url =
                string_field(&payload, "absolute_url").or_else(|| Some(resolved.into()));
            enrich_greenhouse(job, &payload);
        }
        Ats::Lever {
            region,
            site,
            posting,
        } => {
            let host = match region {
                LeverRegion::Eu => "https://api.eu.lever.co/v0/postings",
                LeverRegion::Global => "https://api.lever.co/v0/postings",
            };
            let api = build_api_url(host, &[site, posting])?;
            let payload = request_json(api, 10)?;
            job.canonical_url = string_field(&payload, "applyUrl")
                .or_else(|| string_field(&payload, "hostedUrl"))
                .or_else(|| Some(resolved.into()));
            enrich_lever(job, &payload);
        }
        Ats::Recruitee { company, slug } => {
            let api = Url::parse(&format!("https://{company}.recruitee.com/api/offers/"))
                .map_err(|error| FetchError::Invalid(error.to_string()))?;
            let payload = request_json(api, 10)?;
            let Some(matched) = recruitee_offer(&payload, slug) else {
                suggest_recruitee_replacement(job, &payload, slug);
                return Err(ProviderError::Missing);
            };
            job.canonical_url = string_field(matched, "careers_url")
                .or_else(|| string_field(matched, "url"))
                .or_else(|| Some(resolved.into()));
            if let Some(remote) = matched.get("remote").and_then(Value::as_bool) {
                job.remote = Some(remote);
                if remote {
                    job.work_mode = WorkMode::Remote;
                }
            }
        }
        Ats::Ashby { board, posting } => {
            let mut api = build_api_url("https://api.ashbyhq.com/posting-api/job-board", &[board])?;
            api.query_pairs_mut()
                .append_pair("includeCompensation", "true");
            let payload = request_json(api, 10)?;
            let matched = payload
                .get("jobs")
                .and_then(Value::as_array)
                .and_then(|jobs| {
                    jobs.iter()
                        .find(|item| ashby_job_matches(item, board, posting))
                });
            let Some(matched) = matched else {
                suggest_ashby_replacement(job, &payload, board, posting);
                return Err(ProviderError::Missing);
            };
            job.canonical_url = string_field(matched, "applyUrl")
                .or_else(|| string_field(matched, "jobUrl"))
                .or_else(|| Some(resolved.into()));
            enrich_ashby(job, matched);
        }
    }
    Ok(())
}

fn build_api_url(base: &str, segments: &[&str]) -> Result<Url, FetchError> {
    let mut url = Url::parse(base).map_err(|error| FetchError::Invalid(error.to_string()))?;
    {
        let mut path = url
            .path_segments_mut()
            .map_err(|_| FetchError::Invalid("API URL cannot contain path segments".into()))?;
        path.pop_if_empty();
        for segment in segments {
            path.push(segment);
        }
    }
    Ok(url)
}

fn enrich_greenhouse(job: &mut Job, payload: &Value) {
    let Some(range) = payload
        .get("pay_input_ranges")
        .and_then(Value::as_array)
        .and_then(|ranges| ranges.first())
    else {
        return;
    };
    let context = normalize_text(&format!(
        "{} {}",
        string_field(range, "title").unwrap_or_default(),
        string_field(range, "blurb").unwrap_or_default()
    ));
    if ["annual", "per year", "yearly", "/year"]
        .iter()
        .any(|signal| context.contains(signal))
    {
        set_salary(
            job,
            range.get("min_cents"),
            range.get("max_cents"),
            range.get("currency_type"),
            "greenhouse",
            true,
        );
    }
}

fn enrich_lever(job: &mut Job, payload: &Value) {
    if let Some(salary) = payload.get("salaryRange")
        && salary.get("interval").and_then(Value::as_str) == Some("per-year-salary")
    {
        set_salary(
            job,
            salary.get("min"),
            salary.get("max"),
            salary.get("currency"),
            "lever",
            false,
        );
    }
    if let Some(workplace) = string_field(payload, "workplaceType") {
        apply_workplace(job, &workplace, true);
    }
}

fn enrich_ashby(job: &mut Job, item: &Value) {
    if let Some(workplace) = string_field(item, "workplaceType") {
        apply_workplace(job, &workplace, false);
    }
    if let Some(remote) = item.get("isRemote").and_then(Value::as_bool) {
        job.remote = Some(remote);
        if job.work_mode == WorkMode::Unknown {
            job.work_mode = if remote {
                WorkMode::Remote
            } else {
                WorkMode::Onsite
            };
        }
    }
    if let Some(description) = string_field(item, "descriptionPlain")
        && description.len() > job.description.len()
    {
        job.description = description;
    }
    if let Some(published) = item.get("publishedAt")
        && !published.is_null()
    {
        job.posted = value_string(published);
    }
}

fn apply_workplace(job: &mut Job, workplace: &str, update_remote: bool) {
    let workplace = normalize_text(workplace);
    job.work_mode = match workplace.as_str() {
        "remote" => WorkMode::Remote,
        "hybrid" => WorkMode::Hybrid,
        "onsite" | "on site" => WorkMode::Onsite,
        _ => return,
    };
    if update_remote {
        job.remote = Some(job.work_mode == WorkMode::Remote);
    }
}

fn set_salary(
    job: &mut Job,
    minimum: Option<&Value>,
    maximum: Option<&Value>,
    currency: Option<&Value>,
    source: &str,
    cents: bool,
) {
    let divisor = if cents { 100.0 } else { 1.0 };
    let low = minimum.and_then(number_value);
    let high = maximum.and_then(number_value);
    job.salary_min = low
        .filter(|value| *value >= 0.0)
        .map(|value| value / divisor);
    job.salary_max = high
        .filter(|value| *value >= 0.0)
        .map(|value| value / divisor);
    if let Some(currency) = currency.and_then(value_nonempty_string) {
        job.currency = Some(currency);
    }
    job.salary_source = Some(source.into());
}

fn number_value(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_str()?.parse().ok())
}

fn value_nonempty_string(value: &Value) -> Option<String> {
    let value = value_string(value);
    (!value.is_empty()).then_some(value)
}

fn value_string(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn string_field(value: &Value, field: &str) -> Option<String> {
    value.get(field).and_then(value_nonempty_string)
}

fn ashby_job_matches(item: &Value, board: &str, posting: &str) -> bool {
    let Some(object) = item.as_object() else {
        return false;
    };
    for field in ["jobUrl", "applyUrl"] {
        let Some(value) = object.get(field).and_then(Value::as_str) else {
            continue;
        };
        let Ok(url) = Url::parse(value) else {
            continue;
        };
        let segments = decoded_segments(&url);
        if url
            .host_str()
            .is_some_and(|host| host.eq_ignore_ascii_case("jobs.ashbyhq.com"))
            && matches!(segments.len(), 2 | 3)
            && segments[0] == board
            && segments[1] == posting
            && (segments.len() == 2 || segments[2] == "application")
        {
            return true;
        }
    }
    false
}

fn suggest_ashby_replacement(job: &mut Job, payload: &Value, board: &str, posting: &str) {
    let Some(items) = payload.get("jobs").and_then(Value::as_array) else {
        return;
    };
    let mut candidates = items.iter().filter(|item| {
        item.as_object().is_some()
            && item
                .get("isListed")
                .and_then(Value::as_bool)
                .unwrap_or(true)
            && item
                .get("title")
                .map(value_string)
                .is_some_and(|title| normalize_text(&title) == normalize_text(&job.title))
            && !ashby_job_matches(item, board, posting)
    });
    let Some(candidate) = candidates.next() else {
        return;
    };
    if candidates.next().is_some() {
        return;
    }
    if let Some(replacement) =
        string_field(candidate, "jobUrl").or_else(|| string_field(candidate, "applyUrl"))
    {
        job.replacement_url = Some(replacement);
        job.replacement_title =
            string_field(candidate, "title").or_else(|| Some(job.title.clone()));
    }
}

fn recruitee_offer<'a>(payload: &'a Value, slug: &str) -> Option<&'a Value> {
    payload
        .get("offers")?
        .as_array()?
        .iter()
        .find(|offer| offer.get("slug").map(value_string).as_deref() == Some(slug))
}

fn suggest_recruitee_replacement(job: &mut Job, payload: &Value, slug: &str) {
    let Some(offers) = payload.get("offers").and_then(Value::as_array) else {
        return;
    };
    let normalized_title = normalize_text(&job.title);
    let mut candidates = offers.iter().filter(|offer| {
        offer.as_object().is_some()
            && offer.get("slug").map(value_string).as_deref() != Some(slug)
            && offer
                .get("title")
                .map(value_string)
                .is_some_and(|title| normalize_text(&title) == normalized_title)
    });
    let Some(candidate) = candidates.next() else {
        return;
    };
    if candidates.next().is_some() {
        return;
    }
    if let Some(replacement) =
        string_field(candidate, "careers_url").or_else(|| string_field(candidate, "url"))
    {
        job.replacement_url = Some(replacement);
        job.replacement_title =
            string_field(candidate, "title").or_else(|| Some(job.title.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::demo_jobs;

    #[test]
    fn private_and_special_ipv4_ranges_are_rejected() {
        for value in [
            "http://127.0.0.1/",
            "http://10.0.0.1/",
            "http://169.254.169.254/",
            "http://100.64.0.1/",
            "http://192.0.2.1/",
            "http://198.18.0.1/",
        ] {
            assert!(!is_safe_public_url(value), "{value} should be rejected");
        }
        assert!(is_safe_public_url("https://8.8.8.8/"));
    }

    #[test]
    fn private_and_special_ipv6_ranges_are_rejected() {
        for value in [
            "http://[::1]/",
            "http://[fd00::1]/",
            "http://[fe80::1]/",
            "http://[2001:db8::1]/",
            "http://[64:ff9b::c000:0201]/",
        ] {
            assert!(!is_safe_public_url(value), "{value} should be rejected");
        }
        assert!(is_safe_public_url("https://[2606:4700:4700::1111]/"));
    }

    #[test]
    fn dns_resolution_wait_is_bounded() {
        use std::time::Instant;

        let (_sender, receiver) = mpsc::channel::<Result<Vec<SocketAddr>, String>>();
        let started = Instant::now();
        let result = receive_dns_result(receiver, Duration::from_millis(20));
        assert!(matches!(result, Err(FetchError::DnsTimeout)));
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn credentials_and_localhost_are_rejected() {
        assert!(!is_safe_public_url("https://user:pass@example.com/job"));
        assert!(!is_safe_public_url("http://localhost/job"));
        assert!(!is_safe_public_url("http://foo.localhost/job"));
    }

    #[test]
    fn ats_detection_matches_supported_providers() {
        assert!(matches!(
            detect_ats("https://boards.greenhouse.io/acme/jobs/123"),
            Some(Ats::Greenhouse { .. })
        ));
        assert!(matches!(
            detect_ats("https://jobs.eu.lever.co/acme/abc"),
            Some(Ats::Lever {
                region: LeverRegion::Eu,
                ..
            })
        ));
        assert!(matches!(
            detect_ats("https://jobs.ashbyhq.com/acme/abc"),
            Some(Ats::Ashby { .. })
        ));
        assert!(matches!(
            detect_ats("https://acme.recruitee.com/o/backend-engineer"),
            Some(Ats::Recruitee { .. })
        ));
    }

    #[test]
    fn closed_marker_must_be_page_level_when_headings_exist() {
        let incidental = "<html><head><title>Backend Engineer</title></head><body><p>FAQ: an old job is no longer available.</p></body></html>";
        assert!(!page_indicates_closed(incidental));
        let closed =
            "<html><head><title>Job is no longer available</title></head><body></body></html>";
        assert!(page_indicates_closed(closed));
    }

    #[test]
    fn ashby_identity_is_exact_not_substring_based() {
        let exact = serde_json::json!({
            "jobUrl": "https://jobs.ashbyhq.com/acme/post-123"
        });
        let similar = serde_json::json!({
            "jobUrl": "https://jobs.ashbyhq.com/acme/post-1234"
        });
        assert!(ashby_job_matches(&exact, "acme", "post-123"));
        assert!(!ashby_job_matches(&similar, "acme", "post-123"));
    }

    #[test]
    fn lever_enrichment_updates_salary_and_work_mode() {
        let mut job = demo_jobs().remove(0);
        let payload = serde_json::json!({
            "salaryRange": {"interval":"per-year-salary","min":50000,"max":70000,"currency":"EUR"},
            "workplaceType":"hybrid"
        });
        enrich_lever(&mut job, &payload);
        assert_eq!(job.salary_min, Some(50_000.0));
        assert_eq!(job.salary_max, Some(70_000.0));
        assert_eq!(job.work_mode, WorkMode::Hybrid);
        assert_eq!(job.remote, Some(false));
    }
}
