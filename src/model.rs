use std::{fmt, str::FromStr};

use ratatui::style::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApplicationStatus {
    New,
    Reviewed,
    Applied,
    Interview,
    Rejected,
    Offer,
    Closed,
    Stale,
}

impl ApplicationStatus {
    pub const ALL: [Self; 8] = [
        Self::New,
        Self::Reviewed,
        Self::Applied,
        Self::Interview,
        Self::Rejected,
        Self::Offer,
        Self::Closed,
        Self::Stale,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Reviewed => "reviewed",
            Self::Applied => "applied",
            Self::Interview => "interview",
            Self::Rejected => "rejected",
            Self::Offer => "offer",
            Self::Closed => "closed",
            Self::Stale => "stale",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::New => "New",
            Self::Reviewed => "Reviewed",
            Self::Applied => "Applied",
            Self::Interview => "Interview",
            Self::Rejected => "Rejected",
            Self::Offer => "Offer",
            Self::Closed => "Closed",
            Self::Stale => "Stale",
        }
    }

    pub const fn color(self) -> Color {
        match self {
            Self::New => Color::LightCyan,
            Self::Reviewed => Color::LightBlue,
            Self::Applied => Color::LightMagenta,
            Self::Interview => Color::Yellow,
            Self::Rejected => Color::Red,
            Self::Offer => Color::Green,
            Self::Closed => Color::DarkGray,
            Self::Stale => Color::Gray,
        }
    }
}

impl FromStr for ApplicationStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "new" => Ok(Self::New),
            "reviewed" => Ok(Self::Reviewed),
            "applied" => Ok(Self::Applied),
            "interview" => Ok(Self::Interview),
            "rejected" => Ok(Self::Rejected),
            "offer" => Ok(Self::Offer),
            "closed" => Ok(Self::Closed),
            "stale" => Ok(Self::Stale),
            other => Err(format!("invalid application status: {other}")),
        }
    }
}

impl fmt::Display for ApplicationStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkMode {
    Remote,
    Hybrid,
    Onsite,
    Unknown,
}

impl WorkMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Remote => "remote",
            Self::Hybrid => "hybrid",
            Self::Onsite => "onsite",
            Self::Unknown => "unknown",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Remote => "Remote",
            Self::Hybrid => "Hybrid",
            Self::Onsite => "On-site",
            Self::Unknown => "Unknown",
        }
    }
}

impl FromStr for WorkMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "remote" => Ok(Self::Remote),
            "hybrid" => Ok(Self::Hybrid),
            "onsite" | "on-site" => Ok(Self::Onsite),
            "unknown" | "" => Ok(Self::Unknown),
            other => Err(format!("invalid work mode: {other}")),
        }
    }
}

impl fmt::Display for WorkMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct Job {
    pub id: String,
    pub title: String,
    pub company: String,
    pub location: String,
    pub remote: Option<bool>,
    pub work_mode: WorkMode,
    pub employment_type: Option<String>,
    pub status: ApplicationStatus,
    pub score: f64,
    pub salary_min: Option<f64>,
    pub salary_max: Option<f64>,
    pub currency: Option<String>,
    pub salary_source: Option<String>,
    pub source: String,
    pub source_url: String,
    pub canonical_url: Option<String>,
    pub verification: String,
    pub verification_source: Option<String>,
    pub replacement_url: Option<String>,
    pub replacement_title: Option<String>,
    pub posted: String,
    pub first_seen: String,
    pub last_seen: String,
    pub status_updated_at: Option<String>,
    pub status_manually_set: bool,
    pub reasons: Vec<String>,
    pub concerns: Vec<String>,
    pub description: String,
    pub notes: String,
}

impl Job {
    pub fn search_blob(&self) -> String {
        format!(
            "{} {} {} {} {} {} {} {}",
            self.title,
            self.company,
            self.location,
            self.source,
            self.reasons.join(" "),
            self.concerns.join(" "),
            self.description,
            self.notes,
        )
        .to_lowercase()
    }

    pub fn salary_label(&self) -> String {
        if self.salary_min.is_none() && self.salary_max.is_none() {
            return "Salary not published".into();
        }
        let amount = |value: Option<f64>| match value {
            Some(value) if value.is_finite() => format!("{value:.0}"),
            _ => "?".into(),
        };
        let mut label = if self.salary_min == self.salary_max && self.salary_min.is_some() {
            amount(self.salary_min)
        } else {
            format!("{}–{}", amount(self.salary_min), amount(self.salary_max))
        };
        if let Some(currency) = self.currency.as_deref().filter(|value| !value.is_empty()) {
            label.push(' ');
            label.push_str(currency);
        }
        if let Some(source) = self
            .salary_source
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            label.push_str(" · ");
            label.push_str(source);
        }
        label
    }

    pub fn preferred_url(&self) -> &str {
        self.canonical_url
            .as_deref()
            .filter(|url| !url.is_empty())
            .unwrap_or(&self.source_url)
    }

    pub fn short_id(&self) -> &str {
        let end = self.id.len().min(10);
        &self.id[..end]
    }
}

#[derive(Debug, Clone)]
pub struct JobEvent {
    pub event_type: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
}

pub fn demo_jobs() -> Vec<Job> {
    ApplicationStatus::ALL
        .iter()
        .enumerate()
        .map(|(index, status)| {
            let remote = index % 2 == 0;
            Job {
                id: format!("demo{index:06}"),
                title: match status {
                    ApplicationStatus::New => "Junior Backend Engineer",
                    ApplicationStatus::Reviewed => "Python Developer",
                    ApplicationStatus::Applied => "Software Engineer I",
                    ApplicationStatus::Interview => "Backend Software Engineer",
                    ApplicationStatus::Rejected => "API Engineer",
                    ApplicationStatus::Offer => "Associate Software Engineer",
                    ApplicationStatus::Closed => "Entry-Level Developer",
                    ApplicationStatus::Stale => "Junior Data Engineer",
                }
                .into(),
                company: [
                    "Northstar Labs",
                    "Orbit Finance",
                    "Canvas Works",
                    "Keystone",
                    "Luma Health",
                    "Mosaic Travel",
                    "Juniper Studio",
                    "Blue Atlas",
                ][index]
                    .into(),
                location: if remote {
                    "Italy · Remote".into()
                } else {
                    "Milan · Hybrid".into()
                },
                remote: Some(remote),
                work_mode: if remote {
                    WorkMode::Remote
                } else {
                    WorkMode::Hybrid
                },
                employment_type: Some("fulltime".into()),
                status: *status,
                score: 94.0 - index as f64 * 3.0,
                salary_min: Some(42_000.0),
                salary_max: Some(58_000.0),
                currency: Some("EUR".into()),
                salary_source: Some("employer".into()),
                source: "Greenhouse".into(),
                source_url: format!("https://example.test/source/{index}"),
                canonical_url: Some(format!("https://example.test/jobs/{index}")),
                verification: if *status == ApplicationStatus::Closed {
                    "closed".into()
                } else {
                    "verified".into()
                },
                verification_source: Some("greenhouse".into()),
                replacement_url: None,
                replacement_title: None,
                posted: "2026-08-15T12:00:00+00:00".into(),
                first_seen: "2026-08-15T12:00:00+00:00".into(),
                last_seen: "2026-08-15T12:00:00+00:00".into(),
                status_updated_at: None,
                status_manually_set: !matches!(
                    status,
                    ApplicationStatus::New | ApplicationStatus::Closed | ApplicationStatus::Stale
                ),
                reasons: vec![
                    "Strong title match".into(),
                    "Python".into(),
                    "Junior-friendly".into(),
                ],
                concerns: if index % 3 == 0 {
                    vec!["3 years preferred".into()]
                } else {
                    vec![]
                },
                description: "Build production software with a small product team, code review, automated tests, and structured mentorship.".into(),
                notes: String::new(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_data_covers_every_application_status() {
        let jobs = demo_jobs();
        assert!(
            ApplicationStatus::ALL
                .iter()
                .all(|status| jobs.iter().any(|job| job.status == *status))
        );
    }

    #[test]
    fn job_search_blob_contains_useful_fields() {
        let job = demo_jobs().remove(0);
        let blob = job.search_blob();
        assert!(blob.contains("northstar"));
        assert!(blob.contains("python"));
    }

    #[test]
    fn status_parser_matches_python_tracker_values() {
        assert_eq!(
            "interview".parse::<ApplicationStatus>().unwrap(),
            ApplicationStatus::Interview
        );
        assert!("unknown".parse::<ApplicationStatus>().is_err());
    }
}
