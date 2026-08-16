#[cfg(unix)]
use std::fs;
use std::path::Path;

use rusqlite::Connection;
use serde::Serialize;

use crate::discovery;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub level: &'static str,
    pub check: &'static str,
    pub message: String,
}

pub fn run(config_path: &Path, database_path: &Path) -> Vec<Diagnostic> {
    let mut checks = Vec::new();
    if config_path.exists() {
        checks.push(Diagnostic {
            level: "ok",
            check: "configuration",
            message: format!("Config found: {}", config_path.display()),
        });
        if let Some(check) = permission_check(config_path, "config permissions") {
            checks.push(check);
        }
        match discovery::firecrawl_status(config_path) {
            Ok((false, _)) => checks.push(Diagnostic {
                level: "ok",
                check: "Firecrawl",
                message: "Optional Firecrawl discovery is disabled.".into(),
            }),
            Ok((true, true)) => checks.push(Diagnostic {
                level: "ok",
                check: "Firecrawl",
                message: "Enabled and FIRECRAWL_API_KEY is present in the environment.".into(),
            }),
            Ok((true, false)) => checks.push(Diagnostic {
                level: "error",
                check: "Firecrawl",
                message: "Enabled but FIRECRAWL_API_KEY is not set in the environment.".into(),
            }),
            Err(error) => checks.push(Diagnostic {
                level: "error",
                check: "Firecrawl",
                message: format!("Invalid Firecrawl configuration: {error:#}"),
            }),
        }
    } else {
        checks.push(Diagnostic {
            level: "warn",
            check: "configuration",
            message: format!(
                "Config not found: {}. The tracker can still use --database or the default path.",
                config_path.display()
            ),
        });
    }

    if !database_path.exists() {
        checks.push(Diagnostic {
            level: "warn",
            check: "database",
            message: format!("Database does not exist yet: {}", database_path.display()),
        });
        return checks;
    }

    match Connection::open(database_path) {
        Ok(connection) => {
            match connection.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0)) {
                Ok(3) => checks.push(Diagnostic {
                    level: "ok",
                    check: "database schema",
                    message: "Schema 3 is current and Python-compatible.".into(),
                }),
                Ok(version) if version > 3 => checks.push(Diagnostic {
                    level: "error",
                    check: "database schema",
                    message: format!("Schema {version} is newer than supported schema 3."),
                }),
                Ok(version) => checks.push(Diagnostic {
                    level: "warn",
                    check: "database schema",
                    message: format!("Schema {version} will migrate to schema 3 when opened."),
                }),
                Err(error) => checks.push(Diagnostic {
                    level: "error",
                    check: "database schema",
                    message: error.to_string(),
                }),
            }
            match connection.query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0)) {
                Ok(value) if value == "ok" => checks.push(Diagnostic {
                    level: "ok",
                    check: "database integrity",
                    message: "SQLite quick_check: ok".into(),
                }),
                Ok(value) => checks.push(Diagnostic {
                    level: "error",
                    check: "database integrity",
                    message: value,
                }),
                Err(error) => checks.push(Diagnostic {
                    level: "error",
                    check: "database integrity",
                    message: error.to_string(),
                }),
            }
        }
        Err(error) => checks.push(Diagnostic {
            level: "error",
            check: "database",
            message: format!("Cannot inspect {}: {error}", database_path.display()),
        }),
    }
    if let Some(check) = permission_check(database_path, "database permissions") {
        checks.push(check);
    }
    checks
}

#[cfg(unix)]
fn permission_check(path: &Path, check: &'static str) -> Option<Diagnostic> {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(path).ok()?.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        Some(Diagnostic {
            level: "warn",
            check,
            message: format!(
                "{} is mode {mode:04o}; consider restricting it to 0600.",
                path.display()
            ),
        })
    } else {
        Some(Diagnostic {
            level: "ok",
            check,
            message: format!("{} is restricted to the current user.", path.display()),
        })
    }
}

#[cfg(not(unix))]
fn permission_check(_path: &Path, _check: &'static str) -> Option<Diagnostic> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_paths_produce_actionable_warnings() {
        let checks = run(
            Path::new("/definitely/missing/openjobscout/config.toml"),
            Path::new("/definitely/missing/openjobscout/jobs.sqlite3"),
        );
        assert!(checks.iter().any(|check| check.check == "configuration"));
        assert!(checks.iter().any(|check| check.check == "database"));
    }
}
