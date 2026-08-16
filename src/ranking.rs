use std::sync::OnceLock;

use regex::Regex;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    config::Config,
    model::{Job, WorkMode},
};

const REQUIRED_SIGNALS: &[&str] = &[
    "required",
    "requires",
    "requirement",
    "mandatory",
    "must have",
    "minimum",
    "at least",
    "richiest",
    "obbligatori",
    "necessari",
    "almeno",
];
const PREFERENCE_SIGNALS: &[&str] = &[
    "preferred",
    "nice to have",
    "a plus",
    "bonus",
    "desirable",
    "preferibil",
    "gradit",
];
const HYBRID_SIGNALS: &[&str] = &[
    "hybrid",
    "ibrido",
    "partly remote",
    "days per week in the office",
    "giorni a settimana in ufficio",
];
const ONSITE_SIGNALS: &[&str] = &[
    "on-site",
    "onsite",
    "office-based",
    "in office",
    "in sede",
    "in ufficio",
    "not remote",
    "no remote",
    "non remoto",
    "no smart working",
];
const REMOTE_SIGNALS: &[&str] = &[
    "fully remote",
    "full remote",
    "remote-first",
    "remote within",
    "remote from",
    "work from home",
    "da remoto",
];

#[derive(Debug, Clone, PartialEq)]
pub struct FilterDecision {
    pub allowed: bool,
    pub reason: Option<String>,
}

impl FilterDecision {
    fn allow() -> Self {
        Self {
            allowed: true,
            reason: None,
        }
    }

    fn reject(reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            reason: Some(reason.into()),
        }
    }
}

pub fn normalize_text(value: &str) -> String {
    whitespace_regex()
        .replace_all(value.trim().to_lowercase().as_str(), " ")
        .into_owned()
}

pub fn classify_work_mode(job: &Job) -> WorkMode {
    let text = normalize_text(&format!(
        "{} {} {}",
        job.title, job.location, job.description
    ));
    if HYBRID_SIGNALS.iter().any(|signal| text.contains(signal)) {
        return WorkMode::Hybrid;
    }
    if ONSITE_SIGNALS.iter().any(|signal| text.contains(signal)) {
        return WorkMode::Onsite;
    }
    if job.work_mode != WorkMode::Unknown {
        return job.work_mode;
    }
    if job.remote == Some(true) || REMOTE_SIGNALS.iter().any(|signal| text.contains(signal)) {
        return WorkMode::Remote;
    }
    if job.remote == Some(false) {
        return WorkMode::Onsite;
    }
    WorkMode::Unknown
}

pub fn age_days(value: &str) -> Option<i64> {
    if value.trim().is_empty() {
        return None;
    }
    let parsed = OffsetDateTime::parse(&value.replace('Z', "+00:00"), &Rfc3339)
        .ok()
        .map(|value| value.date())
        .or_else(|| {
            let prefix = value.get(..10)?;
            let synthesized = format!("{prefix}T00:00:00+00:00");
            OffsetDateTime::parse(&synthesized, &Rfc3339)
                .ok()
                .map(|value| value.date())
        })?;
    Some((OffsetDateTime::now_utc().date() - parsed).whole_days())
}

pub fn required_years(text: &str) -> Option<f64> {
    let mut requirements: Vec<f64> = Vec::new();
    for clause in requirement_clauses(text) {
        let matches = experience_regex()
            .captures_iter(&clause)
            .filter_map(|capture| capture.get(1)?.as_str().parse::<f64>().ok())
            .collect::<Vec<_>>();
        if matches.is_empty() {
            continue;
        }
        let lowered = normalize_text(&clause);
        let preferred = PREFERENCE_SIGNALS
            .iter()
            .any(|signal| lowered.contains(signal));
        let explicit_requirement = REQUIRED_SIGNALS
            .iter()
            .any(|signal| lowered.contains(signal));
        if preferred && !explicit_requirement {
            continue;
        }
        requirements.extend(matches);
    }
    requirements.into_iter().reduce(f64::max)
}

pub fn contains_term(text: &str, term: &str) -> bool {
    let normalized_text = normalize_text(text);
    let normalized_term = normalize_text(term);
    if normalized_term.is_empty() {
        return false;
    }
    let body = normalized_term
        .split_whitespace()
        .map(regex::escape)
        .collect::<Vec<_>>()
        .join(r"\s+");
    Regex::new(&format!(
        r"(?:^|[^\p{{L}}\p{{N}}_]){body}(?:[^\p{{L}}\p{{N}}_]|$)"
    ))
    .is_ok_and(|regex| regex.is_match(&normalized_text))
}

pub fn degree_required(text: &str) -> bool {
    degree_regex().is_match(&normalize_text(text))
}

pub fn filter_job(job: &mut Job, config: &Config) -> FilterDecision {
    let title = normalize_text(&job.title);
    let body = normalize_text(&job.description);
    job.work_mode = classify_work_mode(job);

    if config.filters.require_remote && job.work_mode != WorkMode::Remote {
        return FilterDecision::reject(format!(
            "fully remote work not confirmed ({})",
            job.work_mode.as_str()
        ));
    }
    if config
        .filters
        .blocked_title_terms
        .iter()
        .any(|term| contains_term(&title, term))
    {
        return FilterDecision::reject("blocked seniority or title");
    }
    if config
        .filters
        .blocked_description_terms
        .iter()
        .any(|term| contains_term(&body, term))
    {
        return FilterDecision::reject("blocked condition in description");
    }
    if let Some(years) = required_years(&body)
        && years > config.filters.max_required_years as f64
    {
        return FilterDecision::reject(format!(
            "requires {} years of experience",
            compact_number(years)
        ));
    }
    if !config.profile.has_degree
        && config.profile.degree_policy == "filter"
        && degree_required(&body)
    {
        return FilterDecision::reject("degree required");
    }

    let employment = normalize_text(job.employment_type.as_deref().unwrap_or(""));
    let allowed = config
        .filters
        .allowed_employment_types
        .iter()
        .map(|value| normalize_text(value))
        .collect::<Vec<_>>();
    if !allowed.is_empty() && !allowed.contains(&employment) {
        return FilterDecision::reject(format!("employment type not allowed: {employment}"));
    }
    if let Some(days) = age_days(&job.posted)
        && days > config.search.max_age_days
    {
        return FilterDecision::reject(format!("listing is {days} days old"));
    }

    let known_high = job.salary_max.or(job.salary_min);
    if let Some(known_high) = known_high
        && known_high < config.salary.minimum_annual
    {
        return FilterDecision::reject(format!(
            "published salary below {}",
            compact_number(config.salary.minimum_annual)
        ));
    }
    if known_high.is_none() && config.salary.unknown_policy == "filter" {
        return FilterDecision::reject("salary not published");
    }
    FilterDecision::allow()
}

pub fn rank_job(job: &mut Job, config: &Config) {
    let text = normalize_text(&format!("{} {}", job.title, job.description));
    let title = normalize_text(&job.title);
    job.work_mode = classify_work_mode(job);

    let skills = config
        .ranking
        .preferred_skills
        .iter()
        .filter(|value| contains_term(&text, value))
        .cloned()
        .collect::<Vec<_>>();
    let title_hits = config
        .ranking
        .preferred_title_terms
        .iter()
        .filter(|value| contains_term(&title, value))
        .cloned()
        .collect::<Vec<_>>();
    let junior = config
        .ranking
        .junior_signals
        .iter()
        .filter(|value| contains_term(&text, value))
        .cloned()
        .collect::<Vec<_>>();
    let mut concerns = config
        .ranking
        .concern_signals
        .iter()
        .filter(|value| contains_term(&text, value))
        .cloned()
        .collect::<Vec<_>>();

    let mut score =
        skills.len() as f64 * 5.0 + title_hits.len() as f64 * 12.0 + junior.len() as f64 * 7.0;
    if job.work_mode == WorkMode::Remote {
        score += 8.0;
    }
    if job.verification == "verified" {
        score += 5.0;
    }
    score -= concerns.len() as f64 * 8.0;
    if job.verification == "closed" {
        concerns.push("listing closed".into());
        score -= 100.0;
    } else if job.verification == "unreachable" {
        concerns.push("listing could not be verified".into());
        score -= 15.0;
    }

    let known_salary = job.salary_max.or(job.salary_min);
    if let Some(known_salary) = known_salary {
        if config.salary.preferred_annual > 0.0 && known_salary >= config.salary.preferred_annual {
            score += config.salary.preferred_bonus;
        }
    } else {
        score -= config.salary.unknown_penalty;
    }

    match job.work_mode {
        WorkMode::Hybrid => concerns.push("hybrid work".into()),
        WorkMode::Onsite => concerns.push("on-site work".into()),
        WorkMode::Unknown => concerns.push("work mode unconfirmed".into()),
        WorkMode::Remote => {}
    }

    if !config.profile.has_degree
        && config.profile.degree_policy == "penalize"
        && degree_required(&text)
    {
        concerns.push("degree required".into());
        score -= config.profile.degree_penalty;
    }

    job.score = ((score.clamp(0.0, 100.0) * 10.0).round_ties_even()) / 10.0;
    job.reasons.clear();
    if !title_hits.is_empty() {
        job.reasons
            .push(format!("title: {}", title_hits.join(", ")));
    }
    if !skills.is_empty() {
        job.reasons.push(format!("skills: {}", skills.join(", ")));
    }
    if !junior.is_empty() {
        job.reasons
            .push(format!("early-career signals: {}", junior.join(", ")));
    }
    if job.work_mode == WorkMode::Remote {
        job.reasons.push("fully remote".into());
    }
    if let Some(known_salary) = known_salary {
        let reason = format!(
            "published salary: {} {}",
            compact_number(known_salary),
            job.currency.as_deref().unwrap_or("")
        );
        job.reasons.push(reason.trim().to_string());
    } else if config.salary.unknown_penalty != 0.0 {
        concerns.push("salary not published".into());
    }
    job.concerns = concerns;
}

fn requirement_clauses(text: &str) -> Vec<String> {
    let characters = text.chars().collect::<Vec<_>>();
    let mut clauses = Vec::new();
    let mut current = String::new();
    for (index, character) in characters.iter().copied().enumerate() {
        let separator = match character {
            ';' | '\n' | '•' | '▪' => true,
            '.' => {
                let previous_digit = index
                    .checked_sub(1)
                    .and_then(|offset| characters.get(offset))
                    .is_some_and(char::is_ascii_digit);
                let next_digit = characters.get(index + 1).is_some_and(char::is_ascii_digit);
                !(previous_digit && next_digit)
            }
            _ => false,
        };
        if separator {
            if !current.is_empty() {
                clauses.push(std::mem::take(&mut current));
            }
        } else {
            current.push(character);
        }
    }
    if !current.is_empty() {
        clauses.push(current);
    }
    clauses
}

fn compact_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

fn whitespace_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\s+").expect("valid whitespace regex"))
}

fn experience_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)\b(\d{1,2}(?:\.\d)?)\+?\s*(?:years?|ann[oi])(?:\s+(?:of|di)\s+(?:professional(?:e|i)?\s+)?)?(?:experience|esperienza)?",
        )
        .expect("valid experience regex")
    })
}

fn degree_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?:bachelor'?s?|master'?s?|university degree|degree|laurea).{0,80}(?:required|mandatory|must have|requirement|richiest[oaie]?|obbligatori[oaie]?|necessari[oaie]?)|(?:required|mandatory|must have|requirement|richiest[oaie]?|obbligatori[oaie]?|necessari[oaie]?).{0,80}(?:bachelor'?s?|master'?s?|university degree|degree|laurea)",
        )
        .expect("valid degree regex")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{
            Config, FiltersConfig, ProfileConfig, RankingConfig, SalaryConfig, SearchConfig,
            StorageConfig,
        },
        model::{ApplicationStatus, demo_jobs},
    };

    fn config() -> Config {
        Config {
            search: SearchConfig {
                terms: vec!["junior backend".into()],
                sites: vec!["linkedin".into()],
                location: "Italy".into(),
                country_indeed: "Italy".into(),
                results_per_term: 20,
                max_age_days: 30,
            },
            profile: ProfileConfig::default(),
            filters: FiltersConfig {
                require_remote: false,
                allowed_employment_types: vec!["fulltime".into(), "".into()],
                blocked_title_terms: vec!["senior".into()],
                blocked_description_terms: Vec::new(),
                max_required_years: 3,
            },
            ranking: RankingConfig {
                preferred_title_terms: vec!["backend".into(), "software engineer".into()],
                preferred_skills: vec!["python".into(), "postgresql".into()],
                junior_signals: vec!["junior".into(), "graduate".into()],
                concern_signals: vec!["mandatory relocation".into()],
            },
            salary: SalaryConfig {
                preferred_annual: 50_000.0,
                preferred_bonus: 10.0,
                ..SalaryConfig::default()
            },
            storage: StorageConfig {
                database: "jobs.db".into(),
                report_directory: "reports".into(),
                stale_after_days: 30,
            },
        }
    }

    #[test]
    fn preferred_experience_does_not_become_hard_requirement() {
        assert_eq!(
            required_years("5 years preferred; 2 years required"),
            Some(2.0)
        );
        assert_eq!(required_years("5 years preferred"), None);
    }

    #[test]
    fn decimal_experience_is_not_split_at_period() {
        assert_eq!(required_years("Minimum 2.5 years experience."), Some(2.5));
    }

    #[test]
    fn explicit_hybrid_language_beats_remote_flag() {
        let mut job = demo_jobs().remove(0);
        job.remote = Some(true);
        job.work_mode = WorkMode::Unknown;
        job.description = "Hybrid role, two days per week in office".into();
        assert_eq!(classify_work_mode(&job), WorkMode::Hybrid);
    }

    #[test]
    fn blocked_title_terms_respect_word_boundaries() {
        let mut job = demo_jobs().remove(0);
        job.title = "Senior Backend Engineer".into();
        assert!(!filter_job(&mut job, &config()).allowed);
        job.title = "Seniority Platform Engineer".into();
        assert!(filter_job(&mut job, &config()).allowed);
    }

    #[test]
    fn ranking_matches_python_weight_model() {
        let mut job = demo_jobs().remove(0);
        job.title = "Junior Backend Engineer".into();
        job.description = "Python and PostgreSQL. Fully remote.".into();
        job.remote = Some(true);
        job.work_mode = WorkMode::Remote;
        job.verification = "verified".into();
        job.salary_max = Some(60_000.0);
        job.status = ApplicationStatus::New;
        rank_job(&mut job, &config());
        assert_eq!(job.score, 52.0);
        assert!(job.reasons.iter().any(|reason| reason.contains("backend")));
        assert!(job.reasons.iter().any(|reason| reason == "fully remote"));
    }

    #[test]
    fn closed_verification_clamps_score_to_zero() {
        let mut job = demo_jobs().remove(0);
        job.verification = "closed".into();
        rank_job(&mut job, &config());
        assert_eq!(job.score, 0.0);
        assert!(job.concerns.contains(&"listing closed".to_string()));
    }
}
