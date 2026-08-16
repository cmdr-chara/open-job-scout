use std::{
    collections::HashMap,
    fs,
    path::Path,
    sync::OnceLock,
    time::Duration as StdDuration,
};

use anyhow::{Context, Result};
use csv::{ReaderBuilder, StringRecord};
use regex::Regex;
use rusqlite::{Connection, OptionalExtension, params};
use scraper::Html;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use url::Url;

use crate::{
    identity::{job_dedup_key, job_fingerprint},
    model::{ApplicationStatus, Job, WorkMode},
    storage::Storage,
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

pub fn save_jobs(storage: &Storage, jobs: &[Job]) -> Result<usize> {
    let mut connection = Connection::open(storage.path())
        .with_context(|| format!("failed to open {}", storage.path().display()))?;
    connection.busy_timeout(StdDuration::from_secs(5))?;
    connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")?;
    let transaction = connection.transaction()?;
    let now = OffsetDateTime::now_utc().format(&Rfc3339)?;
    let mut count = 0;

    for job in jobs {
        let before: Option<(String, String)> = transaction
            .query_row(
                "SELECT status, verification_status FROM jobs WHERE fingerprint=?",
                [&job.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let remote = job.remote.map(i64::from);
        let reasons = serde_json::to_string(&job.reasons)?;
        let concerns = serde_json::to_string(&job.concerns)?;
        let initial_status = if job.verification == "closed" {
            "closed"
        } else {
            "new"
        };

        transaction.execute(
            "INSERT INTO jobs (
                fingerprint,title,company,location,remote,work_mode,employment_type,
                salary_min,salary_max,currency,salary_source,description,posted_at,
                source,source_url,canonical_url,score,reasons,concerns,
                verification_status,verification_source,replacement_url,replacement_title,
                first_seen_at,last_seen_at,status,status_manually_set
             ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
             ON CONFLICT(fingerprint) DO UPDATE SET
                title=excluded.title,
                company=excluded.company,
                location=excluded.location,
                remote=excluded.remote,
                work_mode=excluded.work_mode,
                employment_type=excluded.employment_type,
                salary_min=COALESCE(excluded.salary_min,jobs.salary_min),
                salary_max=COALESCE(excluded.salary_max,jobs.salary_max),
                currency=COALESCE(excluded.currency,jobs.currency),
                salary_source=COALESCE(excluded.salary_source,jobs.salary_source),
                description=excluded.description,
                posted_at=excluded.posted_at,
                source=excluded.source,
                source_url=excluded.source_url,
                canonical_url=COALESCE(excluded.canonical_url,jobs.canonical_url),
                score=excluded.score,
                reasons=excluded.reasons,
                concerns=excluded.concerns,
                verification_status=excluded.verification_status,
                verification_source=excluded.verification_source,
                replacement_url=excluded.replacement_url,
                replacement_title=excluded.replacement_title,
                last_seen_at=excluded.last_seen_at,
                status=CASE
                    WHEN jobs.status_manually_set=1 THEN jobs.status
                    WHEN excluded.verification_status='closed' THEN 'closed'
                    WHEN jobs.status IN ('closed','stale') THEN 'new'
                    ELSE jobs.status
                END,
                status_updated_at=CASE
                    WHEN jobs.status_manually_set=1 THEN jobs.status_updated_at
                    WHEN excluded.verification_status='closed' AND jobs.status<>'closed'
                    THEN excluded.last_seen_at
                    WHEN excluded.verification_status<>'closed'
                         AND jobs.status IN ('closed','stale')
                    THEN excluded.last_seen_at
                    ELSE jobs.status_updated_at
                END",
            params![
                &job.id,
                &job.title,
                &job.company,
                &job.location,
                remote,
                job.work_mode.as_str(),
                job.employment_type.as_deref(),
                job.salary_min,
                job.salary_max,
                job.currency.as_deref(),
                job.salary_source.as_deref(),
                &job.description,
                &job.posted,
                &job.source,
                &job.source_url,
                job.canonical_url.as_deref(),
                job.score,
                reasons,
                concerns,
                &job.verification,
                job.verification_source.as_deref(),
                job.replacement_url.as_deref(),
                job.replacement_title.as_deref(),
                &now,
                &now,
                initial_status,
                0,
            ],
        )?;

        let after: (String, String) = transaction.query_row(
            "SELECT status, verification_status FROM jobs WHERE fingerprint=?",
            [&job.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        match before {
            None => insert_event(
                &transaction,
                &job.id,
                "discovered",
                None,
                Some(&after.0),
                Some(&format!(
                    "source={}; verification={}",
                    job.source, job.verification
                )),
                &now,
            )?,
            Some((old_status, old_verification)) => {
                if old_verification != after.1 {
                    insert_event(
                        &transaction,
                        &job.id,
                        "verification",
                        Some(&old_verification),
                        Some(&after.1),
                        None,
                        &now,
                    )?;
                }
                if old_status != after.0 {
                    insert_event(
                        &transaction,
                        &job.id,
                        "status",
                        Some(&old_status),
                        Some(&after.0),
                        Some("automatic discovery refresh"),
                        &now,
                    )?;
                }
            }
        }
        count += 1;
    }
    transaction.commit()?;
    Ok(count)
}

fn row_to_job(headers: &StringRecord, record: &StringRecord) -> Result<Job> {
    let mut row = HashMap::new();
    for (key, value) in headers.iter().zip(record.iter()) {
        row.insert(key.to_ascii_lowercase(), value);
    }
    let source_url = first_url(
        &row,
        &["job_url", "source_url", "job_url_direct", "canonical_url"],
    )
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
    let normalized = whitespace_regex()
        .replace_all(value, " ")
        .trim()
        .to_string();
    if is_missing(&normalized) {
        String::new()
    } else {
        normalized
    }
}

fn plain_description(value: &str) -> String {
    let truncated = value
        .chars()
        .take(MAX_DESCRIPTION_CHARS)
        .collect::<String>();
    let mut source = truncated;
    for regex in ignored_element_regexes() {
        source = regex.replace_all(&source, " ").into_owned();
    }
    let document = Html::parse_fragment(&source);
    whitespace_regex()
        .replace_all(
            &document.root_element().text().collect::<Vec<_>>().join(" "),
            " ",
        )
        .trim()
        .to_string()
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

fn insert_event(
    connection: &Connection,
    fingerprint: &str,
    event_type: &str,
    old_value: Option<&str>,
    new_value: Option<&str>,
    note: Option<&str>,
    created_at: &str,
) -> Result<()> {
    connection.execute(
        "INSERT INTO job_events (job_fingerprint,event_type,old_value,new_value,note,created_at)
         VALUES (?,?,?,?,?,?)",
        params![fingerprint, event_type, old_value, new_value, note, created_at],
    )?;
    Ok(())
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
    use std::time::{SystemTime, UNIX_EPOCH};

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
            &[
                "title",
                "company",
                "job_url",
                "job_url_direct",
                "is_remote",
                "min_amount",
            ],
            &[
                "Backend Engineer",
                "Example",
                "https://linkedin.test/job",
                "https://employer.test/job",
                "true",
                "50000",
            ],
        );
        assert_eq!(job.source_url, "https://linkedin.test/job");
        assert_eq!(
            job.canonical_url.as_deref(),
            Some("https://employer.test/job")
        );
        assert_eq!(job.remote, Some(true));
        assert_eq!(job.salary_min, Some(50_000.0));
    }

    #[test]
    fn html_description_discards_script_and_style_content_without_lowercasing() {
        let job = row(
            &["title", "company", "job_url", "description"],
            &[
                "Backend",
                "Example",
                "https://example.test/job",
                "<p>Hello <b>World</b></p><script>evil()</script><style>.x{}</style>",
            ],
        );
        assert_eq!(job.description, "Hello World");
    }

    #[test]
    fn missing_placeholders_and_invalid_urls_are_cleaned() {
        let job = row(
            &[
                "title",
                "company",
                "job_url",
                "job_url_direct",
                "currency",
            ],
            &[
                "Backend",
                "Example",
                "not a url",
                "https://example.test/job",
                "NaN",
            ],
        );
        assert_eq!(job.source_url, "https://example.test/job");
        assert!(job.currency.is_none());
    }

    #[test]
    fn dedup_prefers_richer_direct_listing() {
        let left = row(
            &[
                "title",
                "company",
                "job_url",
                "job_url_direct",
                "description",
            ],
            &[
                "Backend",
                "Example",
                "https://board-a.test/job",
                "https://employer.test/job",
                "short",
            ],
        );
        let right = row(
            &[
                "title",
                "company",
                "job_url",
                "job_url_direct",
                "description",
            ],
            &[
                "Backend",
                "Example",
                "https://board-b.test/job",
                "https://employer.test/job",
                "a much richer description",
            ],
        );
        let jobs = deduplicate(vec![left, right]);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].description, "a much richer description");
    }

    #[test]
    fn save_jobs_matches_discovery_status_ownership_rules() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("openjobscout-import-{unique}.sqlite3"));
        let storage = Storage::open(&path).unwrap();
        let job = row(
            &["title", "company", "job_url", "description"],
            &["Backend", "Example", "https://example.test/job", "Python"],
        );
        save_jobs(&storage, &[job.clone()]).unwrap();
        storage
            .mark_job(&job.id, ApplicationStatus::Applied, None)
            .unwrap();
        let mut refreshed = job.clone();
        refreshed.verification = "closed".into();
        save_jobs(&storage, &[refreshed]).unwrap();
        assert_eq!(
            storage.find_job(&job.id).unwrap().status,
            ApplicationStatus::Applied
        );
        let _ = fs::remove_file(path);
    }
}
