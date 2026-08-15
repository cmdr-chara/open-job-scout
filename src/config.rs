use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ConfigFile {
    storage: Option<StorageConfig>,
}

#[derive(Debug, Deserialize)]
struct StorageConfig {
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

pub fn resolve_database_path(
    explicit_database: Option<&Path>,
    config_path: Option<&Path>,
) -> Result<PathBuf> {
    if let Some(path) = explicit_database {
        return expand_path(path);
    }

    let selected_config = match config_path {
        Some(path) => expand_path(path)?,
        None => default_config_path()?,
    };

    if !selected_config.exists() {
        if config_path.is_some() {
            bail!("config not found: {}", selected_config.display());
        }
        return default_database_path();
    }

    let source = fs::read_to_string(&selected_config)
        .with_context(|| format!("failed to read config {}", selected_config.display()))?;
    let config: ConfigFile = toml::from_str(&source)
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
    if let Some(rest) = text.strip_prefix("~/").or_else(|| text.strip_prefix("~\\")) {
        return Ok(home_dir()?.join(rest));
    }
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    Ok(env::current_dir()?.join(path))
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

    #[test]
    fn absolute_paths_are_preserved() {
        let path = if cfg!(windows) {
            PathBuf::from(r"C:\jobs\tracker.sqlite3")
        } else {
            PathBuf::from("/tmp/tracker.sqlite3")
        };
        assert_eq!(expand_path(&path).unwrap(), path);
    }
}
