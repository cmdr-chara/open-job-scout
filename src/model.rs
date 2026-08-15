use std::fmt;

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

impl fmt::Display for ApplicationStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
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
    pub const fn label(self) -> &'static str {
        match self {
            Self::Remote => "Remote",
            Self::Hybrid => "Hybrid",
            Self::Onsite => "On-site",
            Self::Unknown => "Unknown",
        }
    }
}

impl fmt::Display for WorkMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

#[derive(Debug, Clone)]
pub struct Job {
    pub id: String,
    pub title: String,
    pub company: String,
    pub location: String,
    pub work_mode: WorkMode,
    pub status: ApplicationStatus,
    pub score: u8,
    pub salary: Option<String>,
    pub source: String,
    pub verification: String,
    pub posted: String,
    pub skills: Vec<String>,
    pub concerns: Vec<String>,
    pub description: String,
    pub url: String,
}

impl Job {
    pub fn search_blob(&self) -> String {
        format!(
            "{} {} {} {} {} {}",
            self.title,
            self.company,
            self.location,
            self.source,
            self.skills.join(" "),
            self.description
        )
        .to_lowercase()
    }
}

pub fn demo_jobs() -> Vec<Job> {
    vec![
        Job {
            id: "8f1a9d45e2".into(),
            title: "Junior Backend Engineer".into(),
            company: "Northstar Labs".into(),
            location: "Italy · Remote".into(),
            work_mode: WorkMode::Remote,
            status: ApplicationStatus::New,
            score: 94,
            salary: Some("€48k–€62k".into()),
            source: "Greenhouse".into(),
            verification: "Verified".into(),
            posted: "2h ago".into(),
            skills: vec!["Python".into(), "FastAPI".into(), "PostgreSQL".into(), "Docker".into()],
            concerns: vec!["3 years preferred".into()],
            description: "Join a small product team building APIs used by thousands of customers. The role emphasizes pragmatic backend engineering, mentorship, testing, and gradual ownership. Strong Python fundamentals matter more than having worked with every tool in the stack.".into(),
            url: "https://example.test/northstar/backend".into(),
        },
        Job {
            id: "d25b72c8b4".into(),
            title: "Graduate Software Engineer".into(),
            company: "Tandem Cloud".into(),
            location: "Milan · Hybrid".into(),
            work_mode: WorkMode::Hybrid,
            status: ApplicationStatus::New,
            score: 91,
            salary: Some("€38k–€46k".into()),
            source: "Lever".into(),
            verification: "Verified".into(),
            posted: "5h ago".into(),
            skills: vec!["Go".into(), "APIs".into(), "Kubernetes".into(), "SQL".into()],
            concerns: vec![],
            description: "A structured graduate position with pairing, a dedicated mentor, and rotations through platform and product teams. You will ship production code from the first month while learning cloud infrastructure and service ownership.".into(),
            url: "https://example.test/tandem/graduate".into(),
        },
        Job {
            id: "4b6c019aa7".into(),
            title: "Python Developer".into(),
            company: "Orbit Finance".into(),
            location: "Europe · Remote".into(),
            work_mode: WorkMode::Remote,
            status: ApplicationStatus::Reviewed,
            score: 89,
            salary: Some("€52k–€68k".into()),
            source: "Ashby".into(),
            verification: "Verified".into(),
            posted: "1d ago".into(),
            skills: vec!["Python".into(), "Django".into(), "PostgreSQL".into(), "Redis".into()],
            concerns: vec!["Fintech experience preferred".into()],
            description: "Work on internal and customer-facing financial services with a strong emphasis on correctness, observability, and maintainable Python. The team runs a modern Django stack and encourages engineers to propose product improvements.".into(),
            url: "https://example.test/orbit/python".into(),
        },
        Job {
            id: "be7209a33c".into(),
            title: "API Engineer".into(),
            company: "Luma Health".into(),
            location: "Italy · Remote".into(),
            work_mode: WorkMode::Remote,
            status: ApplicationStatus::New,
            score: 87,
            salary: None,
            source: "Google".into(),
            verification: "Reachable".into(),
            posted: "1d ago".into(),
            skills: vec!["REST".into(), "Python".into(), "Testing".into(), "AWS".into()],
            concerns: vec!["Salary not published".into()],
            description: "Build and maintain healthcare APIs, integrations, and background services. The team values clear interfaces, automated tests, code review, and thoughtful incident response.".into(),
            url: "https://example.test/luma/api".into(),
        },
        Job {
            id: "a9031148cd".into(),
            title: "Software Engineer I".into(),
            company: "Canvas Works".into(),
            location: "Turin · Hybrid".into(),
            work_mode: WorkMode::Hybrid,
            status: ApplicationStatus::Applied,
            score: 86,
            salary: Some("€42k–€50k".into()),
            source: "Greenhouse".into(),
            verification: "Verified".into(),
            posted: "2d ago".into(),
            skills: vec!["TypeScript".into(), "Node.js".into(), "PostgreSQL".into()],
            concerns: vec![],
            description: "A junior product-engineering role spanning backend services and lightweight frontend work. Engineers collaborate closely with design and customer teams and receive regular mentorship.".into(),
            url: "https://example.test/canvas/swe1".into(),
        },
        Job {
            id: "88f901c26e".into(),
            title: "Backend Software Engineer".into(),
            company: "Keystone".into(),
            location: "Europe · Remote".into(),
            work_mode: WorkMode::Remote,
            status: ApplicationStatus::Interview,
            score: 84,
            salary: Some("€55k–€72k".into()),
            source: "Lever".into(),
            verification: "Verified".into(),
            posted: "3d ago".into(),
            skills: vec!["Rust".into(), "PostgreSQL".into(), "Distributed systems".into()],
            concerns: vec!["Rust experience preferred".into()],
            description: "Build backend services for a distributed collaboration platform. The team uses Rust heavily but is open to strong backend engineers from other ecosystems who can demonstrate systems fundamentals.".into(),
            url: "https://example.test/keystone/backend".into(),
        },
        Job {
            id: "5cd684a12f".into(),
            title: "Junior Platform Engineer".into(),
            company: "Vela Systems".into(),
            location: "Rome · Hybrid".into(),
            work_mode: WorkMode::Hybrid,
            status: ApplicationStatus::New,
            score: 82,
            salary: Some("€40k–€49k".into()),
            source: "Recruitee".into(),
            verification: "Verified".into(),
            posted: "3d ago".into(),
            skills: vec!["Linux".into(), "Docker".into(), "Terraform".into(), "Python".into()],
            concerns: vec![],
            description: "Support developer tooling, CI, containers, and cloud infrastructure in a platform team that explicitly welcomes early-career engineers. The first months focus on pairing and operational fundamentals.".into(),
            url: "https://example.test/vela/platform".into(),
        },
        Job {
            id: "19e3acd688".into(),
            title: "Associate Software Engineer".into(),
            company: "Mosaic Travel".into(),
            location: "Italy · Remote".into(),
            work_mode: WorkMode::Remote,
            status: ApplicationStatus::Offer,
            score: 80,
            salary: Some("€44k–€54k".into()),
            source: "Ashby".into(),
            verification: "Verified".into(),
            posted: "4d ago".into(),
            skills: vec!["Java".into(), "Spring".into(), "SQL".into(), "AWS".into()],
            concerns: vec![],
            description: "Join a travel-tech team working on booking and inventory services. The associate track includes structured onboarding, code review, and gradual ownership of production services.".into(),
            url: "https://example.test/mosaic/associate".into(),
        },
        Job {
            id: "3e4cb8901d".into(),
            title: "Backend Engineer".into(),
            company: "Signal Foundry".into(),
            location: "Berlin · Remote EU".into(),
            work_mode: WorkMode::Remote,
            status: ApplicationStatus::Rejected,
            score: 78,
            salary: Some("€58k–€70k".into()),
            source: "Greenhouse".into(),
            verification: "Verified".into(),
            posted: "5d ago".into(),
            skills: vec!["Go".into(), "PostgreSQL".into(), "Kafka".into()],
            concerns: vec!["German timezone overlap".into()],
            description: "Backend role on a real-time event platform. The team values simple service design, observability, and operational ownership.".into(),
            url: "https://example.test/signal/backend".into(),
        },
        Job {
            id: "f06d5ce822".into(),
            title: "Entry-Level Developer".into(),
            company: "Juniper Studio".into(),
            location: "Florence · On-site".into(),
            work_mode: WorkMode::Onsite,
            status: ApplicationStatus::Closed,
            score: 72,
            salary: Some("€32k–€38k".into()),
            source: "Google".into(),
            verification: "Closed".into(),
            posted: "8d ago".into(),
            skills: vec!["JavaScript".into(), "React".into(), "Node.js".into()],
            concerns: vec!["On-site only".into()],
            description: "Entry-level generalist role working across web applications and internal tools. This listing is retained for history even though verification shows it is no longer accepting applications.".into(),
            url: "https://example.test/juniper/developer".into(),
        },
        Job {
            id: "40be507d66".into(),
            title: "Junior Data Engineer".into(),
            company: "Blue Atlas".into(),
            location: "Europe · Remote".into(),
            work_mode: WorkMode::Remote,
            status: ApplicationStatus::Stale,
            score: 76,
            salary: None,
            source: "LinkedIn".into(),
            verification: "Reachable".into(),
            posted: "21d ago".into(),
            skills: vec!["Python".into(), "SQL".into(), "dbt".into(), "Airflow".into()],
            concerns: vec!["Not rediscovered recently".into()],
            description: "Early-career data engineering role focused on ingestion pipelines, analytics infrastructure, and data quality. The job remains reachable but has not been rediscovered inside the configured freshness window.".into(),
            url: "https://example.test/atlas/data".into(),
        },
        Job {
            id: "d991fca04a".into(),
            title: "Software Engineer".into(),
            company: "Helio Robotics".into(),
            location: "Bologna · On-site".into(),
            work_mode: WorkMode::Onsite,
            status: ApplicationStatus::Reviewed,
            score: 74,
            salary: Some("€43k–€55k".into()),
            source: "Lever".into(),
            verification: "Verified".into(),
            posted: "6d ago".into(),
            skills: vec!["C++".into(), "Python".into(), "Linux".into()],
            concerns: vec!["Mostly on-site".into()],
            description: "Develop software for robotics systems with a mix of C++ and Python. The role includes hardware integration, simulation, and close collaboration with controls engineers.".into(),
            url: "https://example.test/helio/software".into(),
        },
        Job {
            id: "bc1f8239a1".into(),
            title: "Cloud Support Engineer".into(),
            company: "Nimbus Grid".into(),
            location: "Italy · Remote".into(),
            work_mode: WorkMode::Unknown,
            status: ApplicationStatus::New,
            score: 70,
            salary: Some("€36k–€44k".into()),
            source: "Google".into(),
            verification: "Reachable".into(),
            posted: "7d ago".into(),
            skills: vec!["Linux".into(), "Networking".into(), "AWS".into(), "Python".into()],
            concerns: vec!["Work arrangement unclear".into()],
            description: "Customer-facing technical role troubleshooting cloud infrastructure and automation. Good fit for someone who likes debugging and wants a path into platform engineering.".into(),
            url: "https://example.test/nimbus/support".into(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_data_covers_every_application_status() {
        let jobs = demo_jobs();
        for status in ApplicationStatus::ALL {
            assert!(jobs.iter().any(|job| job.status == status));
        }
    }

    #[test]
    fn job_search_blob_contains_useful_fields() {
        let job = &demo_jobs()[0];
        let blob = job.search_blob();
        assert!(blob.contains("junior backend engineer"));
        assert!(blob.contains("northstar labs"));
        assert!(blob.contains("fastapi"));
    }
}
