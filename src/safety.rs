#[cfg(unix)]
use std::fs;
use std::path::Path;

#[cfg(windows)]
use std::{path::PathBuf, process::Command};

#[cfg(windows)]
use anyhow::Context;
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
    #[cfg(windows)]
    harden_windows(path, true)?;
    #[cfg(not(any(unix, windows)))]
    return Err(anyhow::anyhow!(
        "private directory permissions are unsupported on this platform"
    ));
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
    #[cfg(windows)]
    harden_windows(path, false)?;
    #[cfg(not(any(unix, windows)))]
    return Err(anyhow::anyhow!(
        "private file permissions are unsupported on this platform"
    ));
    Ok(())
}

#[cfg(windows)]
fn harden_windows(path: &Path, directory: bool) -> Result<()> {
    let sid = current_user_sid()?;
    let permission = if directory {
        format!("*{sid}:(OI)(CI)F")
    } else {
        format!("*{sid}:F")
    };
    let output = run_icacls(path, &["/L", "/reset"])?;
    ensure_icacls_success(path, output)?;
    let output = run_icacls(
        path,
        &["/L", "/inheritance:r", "/grant:r", permission.as_str()],
    )?;
    ensure_icacls_success(path, output)
}

#[cfg(windows)]
fn ensure_icacls_success(path: &Path, output: std::process::Output) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }
    let detail = if output.stderr.is_empty() {
        &output.stdout
    } else {
        &output.stderr
    };
    let detail = terminal_text(&String::from_utf8_lossy(detail));
    Err(anyhow::anyhow!(
        "icacls failed for {}: {}",
        path.display(),
        detail.trim()
    ))
}

#[cfg(windows)]
fn run_icacls(path: &Path, arguments: &[&str]) -> Result<std::process::Output> {
    let mut command = Command::new(system_tool("icacls.exe")?);
    command.arg(path);
    command.args(arguments);
    command
        .output()
        .with_context(|| format!("failed to run icacls for {}", path.display()))
}

#[cfg(windows)]
fn system_tool(name: &str) -> Result<PathBuf> {
    let root = std::env::var_os("SystemRoot")
        .ok_or_else(|| anyhow::anyhow!("SystemRoot is not set; cannot harden Windows ACLs"))?;
    let path = PathBuf::from(root).join("System32").join(name);
    if !path.is_file() {
        return Err(anyhow::anyhow!(
            "Windows system tool is missing: {}",
            path.display()
        ));
    }
    Ok(path)
}

#[cfg(windows)]
fn current_user_sid() -> Result<String> {
    let output = Command::new(system_tool("whoami.exe")?)
        .args(["/user", "/fo", "csv", "/nh"])
        .output()
        .context("failed to identify the current Windows user")?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "whoami.exe failed to identify the current user"
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .split(|character: char| character == ',' || character == '"' || character.is_whitespace())
        .find(|candidate| is_sid(candidate))
        .map(str::to_owned)
        .ok_or_else(|| anyhow::anyhow!("whoami.exe returned no usable user SID"))
}

#[cfg(windows)]
fn is_sid(value: &str) -> bool {
    let mut parts = value.split('-');
    if parts.next() != Some("S") || parts.next() != Some("1") {
        return false;
    }
    let Some(authority) = parts.next() else {
        return false;
    };
    !authority.is_empty()
        && authority
            .bytes()
            .all(|character| character.is_ascii_digit())
        && parts.all(|part| {
            !part.is_empty() && part.bytes().all(|character| character.is_ascii_digit())
        })
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

    #[cfg(windows)]
    #[test]
    fn private_files_and_directories_stop_acl_inheritance() {
        use std::{
            fs,
            time::{SystemTime, UNIX_EPOCH},
        };

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("openjobscout-private-{unique}"));
        let file = directory.join("private.txt");
        fs::create_dir(&directory).unwrap();
        fs::write(&file, "private").unwrap();
        run_icacls(&file, &["/grant", "*S-1-1-0:R"]).unwrap();

        harden_windows(&directory, true).unwrap();
        harden_windows(&file, false).unwrap();

        let directory_output = run_icacls(&directory, &[]).unwrap();
        let file_output = run_icacls(&file, &[]).unwrap();
        let directory_acl = String::from_utf8_lossy(&directory_output.stdout);
        let file_acl = String::from_utf8_lossy(&file_output.stdout);
        assert!(!directory_acl.contains("(I)"));
        assert!(directory_acl.contains("(OI)(CI)(F)"));
        assert!(!file_acl.contains("(I)"));
        assert!(file_acl.contains("(F)"));
        assert!(!file_acl.contains("Everyone"));

        let _ = fs::remove_file(file);
        let _ = fs::remove_dir(directory);
    }
}
