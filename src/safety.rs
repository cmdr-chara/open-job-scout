#[cfg(unix)]
use std::fs;
use std::path::Path;

use anyhow::Result;
use url::Url;

/// Remove terminal control characters from data that may have come from a
/// provider, an imported CSV, or the tracker database.
pub fn terminal_text(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() && !matches!(character, '\n' | '\t') {
                '\u{FFFD}'
            } else {
                character
            }
        })
        .collect()
}

/// Accept only URL values that are safe to hand to a browser or emit as a
/// Markdown link destination.
pub fn safe_http_url(value: &str) -> Option<String> {
    let parsed = Url::parse(value.trim()).ok()?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return None;
    }
    Some(parsed.to_string())
}

pub(crate) fn secure_private_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(path, permissions)?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

pub(crate) fn secure_private_file(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(path, permissions)?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_text_replaces_control_characters() {
        assert_eq!(terminal_text("safe\u{1b}[31m\u{7} text"), "safe�[31m� text");
        assert_eq!(terminal_text("line\nnext\tfield"), "line\nnext\tfield");
    }

    #[test]
    fn safe_http_url_rejects_non_web_and_credential_urls() {
        assert_eq!(
            safe_http_url("https://example.com/jobs/1"),
            Some("https://example.com/jobs/1".into())
        );
        assert!(safe_http_url("javascript:alert(1)").is_none());
        assert!(safe_http_url("https://user:pass@example.com/job").is_none());
        assert!(safe_http_url("not a URL").is_none());
    }

    #[cfg(unix)]
    #[test]
    fn private_files_are_restricted_to_the_current_user() {
        use std::{
            fs,
            os::unix::fs::PermissionsExt,
            time::{SystemTime, UNIX_EPOCH},
        };

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("openjobscout-private-{unique}"));
        fs::write(&path, "private").unwrap();
        secure_private_file(&path).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let _ = fs::remove_file(path);
    }
}
