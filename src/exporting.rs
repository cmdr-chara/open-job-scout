use std::{fs, path::Path};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::model::Job;

pub fn export_jobs(jobs: &[Job], output: &Path, format: &str) -> Result<()> {
    if let Some(parent) = output.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    match format {
        "json" => {
            let records = jobs.iter().map(job_json).collect::<Vec<_>>();
            let mut body = serde_json::to_string_pretty(&records)?;
            body.push('\n');
            fs::write(output, body)
                .with_context(|| format!("failed to write {}", output.display()))?;
        }
        "csv" => {
            let fields = [
                "fingerprint",
                "title",
                "company",
                "location",
                "remote",
                "work_mode",
                "employment_type",
                "salary_min",
                "salary_max",
                "currency",
                "salary_source",
                "description",
                "posted_at",
                "source",
                "source_url",
                "canonical_url",
                "score",
                "status",
                "verification_status",
                "verification_source",
                "replacement_url",
                "replacement_title",
                "first_seen_at",
                "last_seen_at",
                "status_updated_at",
                "notes",
                "reasons",
                "concerns",
            ];
            let mut body = fields.join(",");
            body.push('\n');
            for job in jobs {
                let record = job_json(job);
                let values = fields
                    .iter()
                    .map(|field| csv_value(record.get(*field).unwrap_or(&Value::Null)))
                    .collect::<Vec<_>>();
                body.push_str(&values.join(","));
                body.push('\n');
            }
            fs::write(output, body)
                .with_context(|| format!("failed to write {}", output.display()))?;
        }
        other => bail!("unsupported export format: {other}; expected json or csv"),
    }
    Ok(())
}

pub fn job_json(job: &Job) -> Value {
    json!({
        "fingerprint": job.id,
        "title": job.title,
        "company": job.company,
        "location": job.location,
        "remote": job.remote,
        "work_mode": job.work_mode.as_str(),
        "employment_type": job.employment_type,
        "salary_min": job.salary_min,
        "salary_max": job.salary_max,
        "currency": job.currency,
        "salary_source": job.salary_source,
        "description": job.description,
        "posted_at": job.posted,
        "source": job.source,
        "source_url": job.source_url,
        "canonical_url": job.canonical_url,
        "score": job.score,
        "status": job.status.as_str(),
        "verification_status": job.verification,
        "verification_source": job.verification_source,
        "replacement_url": job.replacement_url,
        "replacement_title": job.replacement_title,
        "first_seen_at": job.first_seen,
        "last_seen_at": job.last_seen,
        "status_updated_at": job.status_updated_at,
        "notes": job.notes,
        "reasons": job.reasons,
        "concerns": job.concerns,
    })
}

fn csv_value(value: &Value) -> String {
    let raw = match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        Value::Array(_) | Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
        _ => value.to_string(),
    };
    if raw.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", raw.replace('"', "\"\""))
    } else {
        raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::demo_jobs;

    #[test]
    fn json_export_uses_python_field_names() {
        let value = job_json(&demo_jobs()[0]);
        assert!(value.get("fingerprint").is_some());
        assert!(value.get("verification_status").is_some());
        assert!(value.get("reasons").unwrap().is_array());
        assert_eq!(value.get("remote").unwrap(), &Value::Bool(true));
    }

    #[test]
    fn csv_escapes_commas_and_quotes() {
        let value = Value::String("one, \"two\"".into());
        assert_eq!(csv_value(&value), "\"one, \"\"two\"\"\"");
    }
}
