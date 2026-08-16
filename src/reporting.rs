use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use regex::Regex;
use time::{OffsetDateTime, format_description};

use crate::model::Job;

pub fn write_markdown(jobs: &[Job], output: &Path) -> Result<PathBuf> {
    if let Some(parent) = output.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let timestamp = OffsetDateTime::now_local()
        .unwrap_or_else(|_| OffsetDateTime::now_utc())
        .format(&format_description::parse_borrowed::<3>(
            "[year]-[month]-[day] [hour]:[minute]",
        )?)?;
    let mut lines = vec![
        format!("# OpenJobScout report - {timestamp}"),
        String::new(),
        format!("Jobs: **{}**", jobs.len()),
        String::new(),
    ];
    for (index, job) in jobs.iter().enumerate() {
        let salary = if job.salary_min.is_none() && job.salary_max.is_none() {
            "not published".into()
        } else {
            let mut value = format!(
                "{}-{} {}",
                amount(job.salary_min),
                amount(job.salary_max),
                inline(job.currency.as_deref().unwrap_or(""))
            )
            .trim()
            .to_string();
            if let Some(source) = job
                .salary_source
                .as_deref()
                .filter(|value| !value.is_empty())
            {
                value.push_str(&format!(" (source: {})", inline(source)));
            }
            value
        };
        let canonical = job
            .canonical_url
            .as_deref()
            .filter(|value| !value.is_empty());
        let preferred = canonical.unwrap_or(&job.source_url);
        lines.extend([
            format!(
                "## {}. {} - {}",
                index + 1,
                inline(&job.title),
                inline(&job.company)
            ),
            String::new(),
            format!("- ID: `{}`", job.short_id()),
            format!("- Score: **{:.1}/100**", job.score),
            format!("- Status: `{}`", job.status.as_str()),
            format!(
                "- Location: {}",
                nonempty_inline(&job.location).unwrap_or_else(|| "not provided".into())
            ),
            format!("- Remote: {}", remote_label(job.remote)),
            format!(
                "- Employment: {}",
                job.employment_type
                    .as_deref()
                    .and_then(nonempty_inline)
                    .unwrap_or_else(|| "not provided".into())
            ),
            format!("- Work mode: {}", inline(job.work_mode.as_str())),
            format!("- Salary: {salary}"),
            format!(
                "- Posted: {}",
                nonempty_inline(&job.posted).unwrap_or_else(|| "not provided".into())
            ),
            format!(
                "- Source: {}",
                nonempty_inline(&job.source).unwrap_or_else(|| "not provided".into())
            ),
            format!("- Verification: {}", job.verification),
            format!("- Reasons: {}", list_inline(&job.reasons)),
            format!("- Concerns: {}", list_inline(&job.concerns)),
            format!("- URL: <{}>", safe_url(preferred)),
        ]);
        if canonical.is_some_and(|value| value != job.source_url) {
            lines.push(format!("- Source URL: <{}>", safe_url(&job.source_url)));
        }
        if !job.notes.is_empty() {
            lines.push(format!("- Notes: {}", inline(&job.notes)));
        }
        if let Some(replacement) = job
            .replacement_url
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            let title = job
                .replacement_title
                .as_deref()
                .filter(|value| !value.is_empty())
                .unwrap_or("open role");
            lines.push(format!(
                "- Suggested successor: [{}](<{}>)",
                inline(title),
                safe_url(replacement)
            ));
        }
        lines.push(String::new());
    }
    fs::write(output, lines.join("\n"))
        .with_context(|| format!("failed to write {}", output.display()))?;
    Ok(output.to_path_buf())
}

fn remote_label(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "yes",
        Some(false) => "no",
        None => "unknown",
    }
}

fn inline(value: &str) -> String {
    let text = whitespace_regex().replace_all(value, " ");
    markdown_regex()
        .replace_all(text.trim(), r"\$1")
        .into_owned()
}

fn nonempty_inline(value: &str) -> Option<String> {
    let value = inline(value);
    (!value.is_empty()).then_some(value)
}

fn safe_url(value: &str) -> String {
    value.replace('<', "%3C").replace('>', "%3E")
}

fn amount(value: Option<f64>) -> String {
    match value {
        Some(value) if value.is_finite() && value.fract() == 0.0 => format!("{value:.0}"),
        Some(value) if value.is_finite() => format!("{value:.2}"),
        _ => "?".into(),
    }
}

fn list_inline(values: &[String]) -> String {
    if values.is_empty() {
        "none".into()
    } else {
        inline(&values.join(", "))
    }
}

fn whitespace_regex() -> &'static Regex {
    static REGEX: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\s+").expect("valid whitespace regex"))
}

fn markdown_regex() -> &'static Regex {
    static REGEX: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"([\\`*_{}\[\]<>()#+!|])").expect("valid markdown escape regex")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::demo_jobs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn markdown_escapes_job_control_characters() {
        assert_eq!(inline("A [role] | remote"), r"A \[role\] \| remote");
    }

    #[test]
    fn report_contains_tracker_fields() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("openjobscout-report-{unique}.md"));
        write_markdown(&demo_jobs()[..1], &path).unwrap();
        let report = fs::read_to_string(&path).unwrap();
        assert!(report.contains("OpenJobScout report"));
        assert!(report.contains("Verification:"));
        assert!(report.contains("Reasons:"));
        let _ = fs::remove_file(path);
    }
}
