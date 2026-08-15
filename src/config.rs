use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub search: SearchConfig,
    #[serde(default)]
    pub profile: ProfileConfig,
    pub filters: FiltersConfig,
    pub ranking: RankingConfig,
    #[serde(default)]
    pub salary: SalaryConfig,
    pub storage: StorageConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchConfig {
    pub terms: Vec<String>,
    pub sites: Vec<String>,
    pub location: String,
    #[serde(default = "default_country_indeed")]
    pub country_indeed: String,
    pub results_per_term: i64,
    pub max_age_days: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProfileConfig {
    #[serde(default = "default_true")]
    pub has_degree: bool,
    #[serde(default = "default_degree_policy")]
    pub degree_policy: String,
    #[serde(default = "default_degree_penalty")]
    pub degree_penalty: f64,
}

impl Default for ProfileConfig {
    fn default() -> Self {
        Self {
            has_degree: true,
            degree_policy: default_degree_policy(),
            degree_penalty: default_degree_penalty(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct FiltersConfig {
    pub require_remote: bool,
    pub allowed_employment_types: Vec<String>,
    pub blocked_title_terms: Vec<String>,
    pub blocked_description_terms: Vec<String>,
    pub max_required_years: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RankingConfig {
    pub preferred_title_terms: Vec<String>,
    pub preferred_skills: Vec<String>,
    pub junior_signals: Vec<String>,
    pub concern_signals: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SalaryConfig {
    #[serde(default)]
    pub minimum_annual: f64,
    #[serde(default)]
    pub preferred_annual: f64,
    #[serde(default = "default_unknown_policy")]
    pub unknown_policy: String,
    #[serde(default)]
    pub unknown_penalty: f64,
    #[serde(default = "default_preferred_bonus")]
    pub preferred_bonus: f64,
}

impl Default for SalaryConfig {
    fn default() -> Self {
        Self {
            minimum_annual: 0.0,
            preferred_annual: 0.0,
            unknown_policy: default_unknown_policy(),
            unknown_penalty: 0.0,
            preferred_bonus: default_preferred_bonus(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    pub database: String,
    pub report_directory: String,
    #[serde(default = "default_stale_after_days")]
    pub stale_after_days: i64,
}

#[derive(Debug, Deserialize)]
struct PathConfig {
    storage: Option<PathStorageConfig>,
}

#[derive(Debug, Deserialize)]
struct PathStorageConfig {
    database: Option<String>,
}

pub fn default_app_dir() -> Result<PathBuf> {
    Ok(home_dir()?.join(".openjobscout"))
}

pub fn default_config_path() -> Result<PathBuf> {
    Ok(default_app_dir()?.join("config.toml"))
}

pub fn default_database_path() -> Result<PathBuf> {
    Ok(default_app_dir()?.join("jobs.sqlite3"))
}

pub fn selected_config_path(config_path: Option<&Path>) -> Result<PathBuf> {
    match config_path {
        Some(path) => expand_path(path),
        None => default_config_path(),
    }
}

pub fn load_config(path: &Path) -> Result<Config> {
    let path = expand_path(path)?;
    if !path.exists() {
        bail!("config not found: {}", path.display());
    }
    let source = fs::read_to_string(&path)
        .with_context(|| format!("failed to read config {}", path.display()))?;
    let config: Config = toml::from_str(&source)
        .with_context(|| format!("failed to parse config {}", path.display()))?;
    validate_config(config)
        .with_context(|| format!("invalid config {}", path.display()))
}

pub fn resolve_database_path(
    explicit_database: Option<&Path>,
    config_path: Option<&Path>,
) -> Result<PathBuf> {
    if let Some(path) = explicit_database {
        return expand_path(path);
    }

    let selected_config = selected_config_path(config_path)?;
    if !selected_config.exists() {
        if config_path.is_some() {
            bail!("config not found: {}", selected_config.display());
        }
        return default_database_path();
    }

    let source = fs::read_to_string(&selected_config)
        .with_context(|| format!("failed to read config {}", selected_config.display()))?;
    let config: PathConfig = toml::from_str(&source)
        .with_context(|| format!("failed to parse config {}", selected_config.display()))?;
    let Some(database) = config.storage.and_then(|storage| storage.database) else {
        return default_database_path();
    };
    expand_path(Path::new(&database))
}

pub fn expand_path(path: &Path) -> Result<PathBuf> {
    let text = path.to_string_lossy();
    if text == "~" {
        return home_dir();
    }
    if let Some(rest) = text
        .strip_prefix("~/")
        .or_else(|| text.strip_prefix("~\\"))
    {
        return Ok(home_dir()?.join(rest));
    }
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    Ok(env::current_dir()?.join(path))
}

fn validate_config(config: Config) -> Result<Config> {
    validate_string_list(&config.search.terms, "[search].terms", false, false)?;
    validate_string_list(&config.search.sites, "[search].sites", false, false)?;
    if config.search.results_per_term < 1 {
        bail!("config value [search].results_per_term must be an integer >= 1");
    }
    if config.search.max_age_days < 0 {
        bail!("config value [search].max_age_days must be an integer >= 0");
    }
    if config.search.country_indeed.trim().is_empty() {
        bail!("config value [search].country_indeed must not be blank");
    }

    validate_string_list(
        &config.filters.allowed_employment_types,
        "[filters].allowed_employment_types",
        true,
        true,
    )?;
    validate_string_list(
        &config.filters.blocked_title_terms,
        "[filters].blocked_title_terms",
        true,
        false,
    )?;
    validate_string_list(
        &config.filters.blocked_description_terms,
        "[filters].blocked_description_terms",
        true,
        false,
    )?;
    if config.filters.max_required_years < 0 {
        bail!("config value [filters].max_required_years must be an integer >= 0");
    }

    for (values, key) in [
        (
            &config.ranking.preferred_title_terms,
            "[ranking].preferred_title_terms",
        ),
        (&config.ranking.preferred_skills, "[ranking].preferred_skills"),
        (&config.ranking.junior_signals, "[ranking].junior_signals"),
        (&config.ranking.concern_signals, "[ranking].concern_signals"),
    ] {
        validate_string_list(values, key, true, false)?;
    }

    for (value, key) in [
        (config.salary.minimum_annual, "[salary].minimum_annual"),
        (config.salary.preferred_annual, "[salary].preferred_annual"),
        (config.salary.unknown_penalty, "[salary].unknown_penalty"),
        (config.salary.preferred_bonus, "[salary].preferred_bonus"),
        (config.profile.degree_penalty, "[profile].degree_penalty"),
    ] {
        if !value.is_finite() || value < 0.0 {
            bail!("config value {key} must be a number >= 0");
        }
    }
    if !matches!(config.salary.unknown_policy.as_str(), "allow" | "filter") {
        bail!("config value [salary].unknown_policy must be allow or filter");
    }
    if config.salary.preferred_annual > 0.0
        && config.salary.minimum_annual > config.salary.preferred_annual
    {
        bail!("config value [salary].minimum_annual must not exceed preferred_annual");
    }
    if !matches!(
        config.profile.degree_policy.as_str(),
        "ignore" | "penalize" | "filter"
    ) {
        bail!("config value [profile].degree_policy must be ignore, penalize, or filter");
    }

    if config.storage.database.trim().is_empty() {
        bail!("config value [storage].database must be a non-empty path string");
    }
    if config.storage.report_directory.trim().is_empty() {
        bail!("config value [storage].report_directory must be a non-empty path string");
    }
    if config.storage.stale_after_days < 1 {
        bail!("config value [storage].stale_after_days must be an integer >= 1");
    }
    Ok(config)
}

fn validate_string_list(
    values: &[String],
    key: &str,
    allow_empty: bool,
    allow_blank: bool,
) -> Result<()> {
    if !allow_empty && values.is_empty() {
        bail!("config value {key} must not be empty");
    }
    if !allow_blank && values.iter().any(|value| value.trim().is_empty()) {
        bail!("config value {key} contains a blank item");
    }
    Ok(())
}

fn default_true() -> bool {
    true
}

fn default_country_indeed() -> String {
    "USA".into()
}

fn default_degree_policy() -> String {
    "ignore".into()
}

fn default_degree_penalty() -> f64 {
    15.0
}

fn default_unknown_policy() -> String {
    "allow".into()
}

fn default_preferred_bonus() -> f64 {
    10.0
}

fn default_stale_after_days() -> i64 {
    30
}

fn home_dir() -> Result<PathBuf> {
    if let Some(value) = env::var_os("HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(value));
    }
    if let Some(value) = env::var_os("USERPROFILE").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(value));
    }
    bail!("could not determine the current user's home directory")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_config(source: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!("openjobscout-config-{unique}.toml"));
        fs::write(&path, source).unwrap();
        path
    }

    #[test]
    fn absolute_paths_are_preserved() {
        let path = if cfg!(windows) {
            PathBuf::from(r"C:\jobs\tracker.sqlite3")
        } else {
            PathBuf::from("/tmp/tracker.sqlite3")
        };
        assert_eq!(expand_path(&path).unwrap(), path);
    }

    #[test]
    fn full_python_style_config_loads() {
        let path = temp_config(
            r#"
[search]
terms=["junior software engineer"]
sites=["linkedin"]
location="Italy"
country_indeed="Italy"
results_per_term=20
max_age_days=14
[profile]
has_degree=true
degree_policy="ignore"
degree_penalty=15
[filters]
require_remote=false
allowed_employment_types=["fulltime",""]
blocked_title_terms=["senior"]
blocked_description_terms=[]
max_required_years=3
[ranking]
preferred_title_terms=["backend"]
preferred_skills=["python"]
junior_signals=["junior"]
concern_signals=[]
[salary]
minimum_annual=0
preferred_annual=50000
unknown_policy="allow"
unknown_penalty=0
preferred_bonus=10
[storage]
database="~/.openjobscout/jobs.sqlite3"
report_directory="~/.openjobscout/reports"
stale_after_days=30
"#,
        );
        let config = load_config(&path).unwrap();
        assert_eq!(config.search.location, "Italy");
        assert_eq!(config.ranking.preferred_skills, vec!["python"]);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn invalid_salary_policy_is_rejected() {
        let path = temp_config(
            r#"
[search]
terms=["junior"]
sites=["linkedin"]
location=""
results_per_term=1
max_age_days=1
[filters]
require_remote=false
allowed_employment_types=[]
blocked_title_terms=[]
blocked_description_terms=[]
max_required_years=3
[ranking]
preferred_title_terms=[]
preferred_skills=[]
junior_signals=[]
concern_signals=[]
[salary]
unknown_policy="guess"
[storage]
database="jobs.db"
report_directory="reports"
"#,
        );
        assert!(load_config(&path).is_err());
        fs::remove_file(path).unwrap();
    }
}
