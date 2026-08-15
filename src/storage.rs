use std::{
    fs,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration as StdDuration,
};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, Row, params};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

use crate::model::{ApplicationStatus, Job, JobEvent, WorkMode};

const SCHEMA_VERSION: i64 = 3;
const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS jobs (
    fingerprint TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    company TEXT NOT NULL,
    location TEXT,
    remote INTEGER,
    work_mode TEXT NOT NULL DEFAULT 'unknown',
    employment_type TEXT,
    salary_min REAL,
    salary_max REAL,
    currency TEXT,
    salary_source TEXT,
    description TEXT,
    posted_at TEXT,
    source TEXT,
    source_url TEXT,
    canonical_url TEXT,
    score REAL NOT NULL DEFAULT 0,
    reasons TEXT NOT NULL DEFAULT '[]',
    concerns TEXT NOT NULL DEFAULT '[]',
    verification_status TEXT NOT NULL DEFAULT 'unverified',
    verification_source TEXT,
    replacement_url TEXT,
    replacement_title TEXT,
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'new',
    status_updated_at TEXT,
    status_manually_set INTEGER NOT NULL DEFAULT 0,
    notes TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_jobs_status_score ON jobs(status, score DESC);
CREATE INDEX IF NOT EXISTS idx_jobs_last_seen ON jobs(last_seen_at DESC);
CREATE TABLE IF NOT EXISTS job_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_fingerprint TEXT NOT NULL,
    event_type TEXT NOT NULL,
    old_value TEXT,
    new_value TEXT,
    note TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY(job_fingerprint) REFERENCES jobs(fingerprint)
        ON UPDATE CASCADE ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_job_events_job_created
ON job_events(job_fingerprint, created_at DESC, id DESC);
"#;

#[derive(Debug, Clone)]
pub struct Storage {
    path: PathBuf,
}

impl Storage {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let storage = Self { path: path.into() };
        let connection = storage.connect()?;
        drop(connection);
        Ok(storage)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load_jobs(&self) -> Result<Vec<Job>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT * FROM jobs ORDER BY score DESC, last_seen_at DESC, fingerprint ASC",
        )?;
        let mut rows = statement.query([])?;
        let mut jobs = Vec::new();
        while let Some(row) = rows.next()? {
            jobs.push(job_from_row(row)?);
        }
        Ok(jobs)
    }

    pub fn find_job(&self, identifier: &str) -> Result<Job> {
        let connection = self.connect()?;
        let fingerprints = matching_fingerprints(&connection, identifier)?;
        let fingerprint = unique_fingerprint(identifier, &fingerprints)?;
        connection
            .query_row(
                "SELECT * FROM jobs WHERE fingerprint=?",
                [fingerprint],
                |row| Ok(raw_job(row)),
            )
            .optional()?
            .ok_or_else(|| anyhow::anyhow!("no job matches ID {identifier:?}"))
            .and_then(job_from_raw)
    }

    pub fn mark_job(
        &self,
        identifier: &str,
        status: ApplicationStatus,
        note: Option<&str>,
    ) -> Result<()> {
        let mut connection = self.connect()?;
        let fingerprints = matching_fingerprints(&connection, identifier)?;
        let fingerprint = unique_fingerprint(identifier, &fingerprints)?.to_string();
        let transaction = connection.transaction()?;
        let (old_status, old_notes): (String, String) = transaction.query_row(
            "SELECT status, notes FROM jobs WHERE fingerprint=?",
            [&fingerprint],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let now = now_rfc3339()?;
        let next_notes = match note.map(str::trim).filter(|value| !value.is_empty()) {
            Some(note) => append_note(&old_notes, note, &now),
            None => old_notes,
        };
        transaction.execute(
            "UPDATE jobs SET status=?, status_updated_at=?, status_manually_set=1, notes=? WHERE fingerprint=?",
            params![status.as_str(), now, next_notes, fingerprint],
        )?;
        if old_status != status.as_str() {
            record_event(
                &transaction,
                &fingerprint,
                "status",
                Some(&old_status),
                Some(status.as_str()),
                note,
                &now,
            )?;
        } else if let Some(note) = note.map(str::trim).filter(|value| !value.is_empty()) {
            record_event(
                &transaction,
                &fingerprint,
                "note",
                None,
                None,
                Some(note),
                &now,
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn add_note(&self, identifier: &str, note: &str) -> Result<()> {
        let note = note.trim();
        if note.is_empty() {
            bail!("note must not be blank");
        }
        let mut connection = self.connect()?;
        let fingerprints = matching_fingerprints(&connection, identifier)?;
        let fingerprint = unique_fingerprint(identifier, &fingerprints)?.to_string();
        let transaction = connection.transaction()?;
        let old_notes: String = transaction.query_row(
            "SELECT notes FROM jobs WHERE fingerprint=?",
            [&fingerprint],
            |row| row.get(0),
        )?;
        let now = now_rfc3339()?;
        let next_notes = append_note(&old_notes, note, &now);
        transaction.execute(
            "UPDATE jobs SET notes=? WHERE fingerprint=?",
            params![next_notes, fingerprint],
        )?;
        record_event(
            &transaction,
            &fingerprint,
            "note",
            None,
            None,
            Some(note),
            &now,
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn events(&self, identifier: &str, limit: usize) -> Result<Vec<JobEvent>> {
        if limit == 0 {
            bail!("history limit must be at least 1");
        }
        let connection = self.connect()?;
        let fingerprints = matching_fingerprints(&connection, identifier)?;
        let fingerprint = unique_fingerprint(identifier, &fingerprints)?;
        let mut statement = connection.prepare(
            "SELECT event_type, old_value, new_value, note, created_at
             FROM job_events
             WHERE job_fingerprint=?
             ORDER BY created_at DESC, id DESC
             LIMIT ?",
        )?;
        let rows = statement.query_map(params![fingerprint, limit as i64], |row| {
            Ok(JobEvent {
                event_type: row.get(0)?,
                old_value: row.get(1)?,
                new_value: row.get(2)?,
                note: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn mark_stale_jobs(&self, stale_after_days: i64) -> Result<usize> {
        if stale_after_days < 1 {
            bail!("stale_after_days must be at least 1");
        }
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        let cutoff =
            (OffsetDateTime::now_utc() - Duration::days(stale_after_days)).format(&Rfc3339)?;
        let now = now_rfc3339()?;
        let candidates: Vec<(String, String)> = {
            let mut statement = transaction.prepare(
                "SELECT fingerprint, status FROM jobs
                 WHERE status_manually_set=0
                   AND status IN ('new','reviewed')
                   AND last_seen_at < ?",
            )?;
            let rows = statement.query_map([&cutoff], |row| Ok((row.get(0)?, row.get(1)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for (fingerprint, old_status) in &candidates {
            transaction.execute(
                "UPDATE jobs SET status='stale', status_updated_at=? WHERE fingerprint=?",
                params![now, fingerprint],
            )?;
            record_event(
                &transaction,
                fingerprint,
                "status",
                Some(old_status),
                Some("stale"),
                Some("not seen in configured discovery window"),
                &now,
            )?;
        }
        transaction.commit()?;
        Ok(candidates.len())
    }

    fn connect(&self) -> Result<Connection> {
        if let Some(parent) = self
            .path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let existed = self.path.exists();
        let connection = Connection::open(&self.path)
            .with_context(|| format!("failed to open {}", self.path.display()))?;
        connection.busy_timeout(StdDuration::from_secs(5))?;
        connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")?;
        migrate_schema(&connection)?;
        if !existed {
            secure_database_permissions(&self.path)?;
        }
        Ok(connection)
    }
}

fn migrate_schema(connection: &Connection) -> Result<()> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > SCHEMA_VERSION {
        bail!(
            "database schema {version} is newer than this OpenJobScout build supports ({SCHEMA_VERSION})"
        );
    }

    connection.execute_batch(SCHEMA)?;
    let job_count: i64 = connection.query_row("SELECT COUNT(*) FROM jobs", [], |row| row.get(0))?;
    if version == 0 && job_count > 0 {
        bail!(
            "legacy pre-v1 tracker detected; open it once with OpenJobScout Python v1 to migrate fingerprints safely"
        );
    }
    if version < 1 {
        connection.pragma_update(None, "user_version", 1)?;
    }
    if version < 2 {
        let columns = table_columns(connection, "jobs")?;
        for (name, definition) in [
            ("work_mode", "TEXT NOT NULL DEFAULT 'unknown'"),
            ("replacement_url", "TEXT"),
            ("replacement_title", "TEXT"),
            ("status_manually_set", "INTEGER NOT NULL DEFAULT 0"),
        ] {
            if !columns.iter().any(|column| column == name) {
                connection
                    .execute_batch(&format!("ALTER TABLE jobs ADD COLUMN {name} {definition};"))?;
            }
        }
        connection.execute(
            "UPDATE jobs SET status_manually_set=1 WHERE status IN ('reviewed','applied','interview','rejected','offer')",
            [],
        )?;
        connection.pragma_update(None, "user_version", 2)?;
    }
    if version < 3 {
        connection.execute(
            "INSERT INTO job_events (job_fingerprint, event_type, new_value, note, created_at)
             SELECT fingerprint, 'snapshot', status, 'State recorded during history migration',
                    COALESCE(status_updated_at, first_seen_at)
             FROM jobs
             WHERE NOT EXISTS (
                 SELECT 1 FROM job_events WHERE job_events.job_fingerprint=jobs.fingerprint
             )",
            [],
        )?;
        connection.pragma_update(None, "user_version", 3)?;
    }
    Ok(())
}

fn table_columns(connection: &Connection, table: &str) -> Result<Vec<String>> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = statement.query_map([], |row| row.get(1))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn raw_job(row: &Row<'_>) -> RawJob {
    RawJob {
        id: row.get("fingerprint").unwrap_or_default(),
        title: row.get("title").unwrap_or_default(),
        company: row.get("company").unwrap_or_default(),
        location: row
            .get::<_, Option<String>>("location")
            .unwrap_or_default()
            .unwrap_or_default(),
        work_mode: row.get("work_mode").unwrap_or_else(|_| "unknown".into()),
        employment_type: row.get("employment_type").unwrap_or_default(),
        status: row.get("status").unwrap_or_else(|_| "new".into()),
        score: row.get("score").unwrap_or_default(),
        salary_min: row.get("salary_min").unwrap_or_default(),
        salary_max: row.get("salary_max").unwrap_or_default(),
        currency: row.get("currency").unwrap_or_default(),
        salary_source: row.get("salary_source").unwrap_or_default(),
        source: row
            .get::<_, Option<String>>("source")
            .unwrap_or_default()
            .unwrap_or_default(),
        source_url: row
            .get::<_, Option<String>>("source_url")
            .unwrap_or_default()
            .unwrap_or_default(),
        canonical_url: row.get("canonical_url").unwrap_or_default(),
        verification: row
            .get("verification_status")
            .unwrap_or_else(|_| "unverified".into()),
        verification_source: row.get("verification_source").unwrap_or_default(),
        replacement_url: row.get("replacement_url").unwrap_or_default(),
        replacement_title: row.get("replacement_title").unwrap_or_default(),
        posted: row
            .get::<_, Option<String>>("posted_at")
            .unwrap_or_default()
            .unwrap_or_default(),
        first_seen: row.get("first_seen_at").unwrap_or_default(),
        last_seen: row.get("last_seen_at").unwrap_or_default(),
        status_updated_at: row.get("status_updated_at").unwrap_or_default(),
        status_manually_set: row.get::<_, i64>("status_manually_set").unwrap_or_default() != 0,
        reasons: row.get("reasons").unwrap_or_else(|_| "[]".into()),
        concerns: row.get("concerns").unwrap_or_else(|_| "[]".into()),
        description: row
            .get::<_, Option<String>>("description")
            .unwrap_or_default()
            .unwrap_or_default(),
        notes: row.get("notes").unwrap_or_default(),
    }
}

fn job_from_row(row: &Row<'_>) -> Result<Job> {
    job_from_raw(raw_job(row))
}

fn job_from_raw(raw: RawJob) -> Result<Job> {
    Ok(Job {
        id: raw.id,
        title: raw.title,
        company: raw.company,
        location: raw.location,
        work_mode: WorkMode::from_str(&raw.work_mode).unwrap_or(WorkMode::Unknown),
        employment_type: raw.employment_type,
        status: ApplicationStatus::from_str(&raw.status).map_err(|error| anyhow::anyhow!(error))?,
        score: raw.score,
        salary_min: raw.salary_min,
        salary_max: raw.salary_max,
        currency: raw.currency,
        salary_source: raw.salary_source,
        source: raw.source,
        source_url: raw.source_url,
        canonical_url: raw.canonical_url,
        verification: raw.verification,
        verification_source: raw.verification_source,
        replacement_url: raw.replacement_url,
        replacement_title: raw.replacement_title,
        posted: raw.posted,
        first_seen: raw.first_seen,
        last_seen: raw.last_seen,
        status_updated_at: raw.status_updated_at,
        status_manually_set: raw.status_manually_set,
        reasons: decode_list(&raw.reasons),
        concerns: decode_list(&raw.concerns),
        description: raw.description,
        notes: raw.notes,
    })
}

struct RawJob {
    id: String,
    title: String,
    company: String,
    location: String,
    work_mode: String,
    employment_type: Option<String>,
    status: String,
    score: f64,
    salary_min: Option<f64>,
    salary_max: Option<f64>,
    currency: Option<String>,
    salary_source: Option<String>,
    source: String,
    source_url: String,
    canonical_url: Option<String>,
    verification: String,
    verification_source: Option<String>,
    replacement_url: Option<String>,
    replacement_title: Option<String>,
    posted: String,
    first_seen: String,
    last_seen: String,
    status_updated_at: Option<String>,
    status_manually_set: bool,
    reasons: String,
    concerns: String,
    description: String,
    notes: String,
}

fn decode_list(value: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(value).unwrap_or_default()
}

fn matching_fingerprints(connection: &Connection, identifier: &str) -> Result<Vec<String>> {
    let identifier = identifier.trim();
    if identifier.is_empty() {
        bail!("job ID must not be blank");
    }
    let mut statement = connection.prepare(
        "SELECT fingerprint FROM jobs WHERE fingerprint LIKE ? ORDER BY fingerprint LIMIT 3",
    )?;
    let rows = statement.query_map([format!("{identifier}%")], |row| row.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn unique_fingerprint<'a>(identifier: &str, matches: &'a [String]) -> Result<&'a str> {
    match matches {
        [] => bail!("no job matches ID {identifier:?}"),
        [fingerprint] => Ok(fingerprint),
        _ => bail!("ID {identifier:?} is ambiguous; use more characters"),
    }
}

fn append_note(existing: &str, note: &str, now: &str) -> String {
    if existing.trim().is_empty() {
        return note.to_string();
    }
    let tagged = format!("[{now}] {note}");
    if existing == note || existing.lines().any(|line| line == tagged) {
        return existing.to_string();
    }
    format!("{}\n{tagged}", existing.trim_end())
}

fn record_event(
    connection: &Connection,
    fingerprint: &str,
    event_type: &str,
    old_value: Option<&str>,
    new_value: Option<&str>,
    note: Option<&str>,
    created_at: &str,
) -> Result<()> {
    connection.execute(
        "INSERT INTO job_events (job_fingerprint, event_type, old_value, new_value, note, created_at)
         VALUES (?, ?, ?, ?, ?, ?)",
        params![fingerprint, event_type, old_value, new_value, note, created_at],
    )?;
    Ok(())
}

fn now_rfc3339() -> Result<String> {
    Ok(OffsetDateTime::now_utc().format(&Rfc3339)?)
}

#[cfg(unix)]
fn secure_database_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn secure_database_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_database(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "openjobscout-{name}-{}-{unique}.sqlite3",
            std::process::id()
        ))
    }

    fn insert_job(storage: &Storage) -> String {
        let connection = storage.connect().unwrap();
        let id = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        connection
            .execute(
                "INSERT INTO jobs (
                fingerprint,title,company,location,remote,work_mode,employment_type,
                description,posted_at,source,source_url,canonical_url,score,reasons,concerns,
                verification_status,first_seen_at,last_seen_at,status,status_manually_set,notes
             ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                params![
                    id,
                    "Backend Engineer",
                    "Example Labs",
                    "Italy",
                    1,
                    "remote",
                    "fulltime",
                    "Python APIs",
                    "2026-08-15T12:00:00+00:00",
                    "Greenhouse",
                    "https://source.test/job",
                    "https://employer.test/job",
                    91.0,
                    "[\"Python\",\"Junior-friendly\"]",
                    "[]",
                    "verified",
                    "2026-08-15T12:00:00+00:00",
                    "2026-08-15T12:00:00+00:00",
                    "new",
                    0,
                    ""
                ],
            )
            .unwrap();
        id.into()
    }

    #[test]
    fn new_database_uses_python_schema_version_three() {
        let path = temp_database("schema");
        let storage = Storage::open(&path).unwrap();
        let connection = storage.connect().unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 3);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn status_changes_persist_and_record_history() {
        let path = temp_database("status");
        let storage = Storage::open(&path).unwrap();
        let id = insert_job(&storage);
        storage
            .mark_job(
                &id[..10],
                ApplicationStatus::Applied,
                Some("sent application"),
            )
            .unwrap();
        let job = storage.find_job(&id[..10]).unwrap();
        assert_eq!(job.status, ApplicationStatus::Applied);
        assert!(job.status_manually_set);
        assert!(job.notes.contains("sent application"));
        let events = storage.events(&id[..10], 10).unwrap();
        assert_eq!(events[0].event_type, "status");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn notes_do_not_mutate_status_ownership() {
        let path = temp_database("note");
        let storage = Storage::open(&path).unwrap();
        let id = insert_job(&storage);
        storage.add_note(&id[..10], "interesting team").unwrap();
        let job = storage.find_job(&id[..10]).unwrap();
        assert_eq!(job.status, ApplicationStatus::New);
        assert!(!job.status_manually_set);
        assert_eq!(job.notes, "interesting team");
        assert_eq!(storage.events(&id[..10], 10).unwrap()[0].event_type, "note");
        let _ = fs::remove_file(path);
    }
}
