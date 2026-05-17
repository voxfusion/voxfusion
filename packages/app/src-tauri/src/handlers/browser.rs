const SAFARI_BUNDLES: &[&str] = &["com.apple.Safari", "com.apple.SafariTechnologyPreview"];

const CHROMIUM_BUNDLES: &[&str] = &[
    "com.google.Chrome",
    "com.google.Chrome.canary",
    "com.google.Chrome.beta",
    "com.google.Chrome.dev",
    "com.brave.Browser",
    "com.brave.Browser.nightly",
    "com.brave.Browser.beta",
    "com.microsoft.edgemac",
    "com.microsoft.edgemac.Beta",
    "com.microsoft.edgemac.Dev",
    "com.microsoft.edgemac.Canary",
    "com.vivaldi.Vivaldi",
    "com.operasoftware.Opera",
    "com.operasoftware.OperaGX",
    "company.thebrowser.Browser",
    "company.thebrowser.dia",
    "app.zen-browser.zen",
    "com.duckduckgo.macos.browser",
];

pub fn is_known_browser(bundle_id: &str) -> bool {
    SAFARI_BUNDLES
        .iter()
        .chain(CHROMIUM_BUNDLES.iter())
        .any(|b| b.eq_ignore_ascii_case(bundle_id))
}

fn applescript_for_bundle(bundle_id: &str) -> Option<String> {
    if SAFARI_BUNDLES
        .iter()
        .any(|b| b.eq_ignore_ascii_case(bundle_id))
    {
        return Some(format!(
            r#"tell application id "{}" to return URL of front document"#,
            bundle_id
        ));
    }
    if CHROMIUM_BUNDLES
        .iter()
        .any(|b| b.eq_ignore_ascii_case(bundle_id))
    {
        return Some(format!(
            r#"tell application id "{}" to return URL of active tab of front window"#,
            bundle_id
        ));
    }
    None
}

pub fn normalize_domain(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    let without_scheme = match trimmed.find("://") {
        Some(idx) => &trimmed[idx + 3..],
        None => trimmed,
    };

    let host_part = without_scheme
        .split('/')
        .next()
        .unwrap_or(without_scheme)
        .split('?')
        .next()
        .unwrap_or(without_scheme)
        .split('#')
        .next()
        .unwrap_or(without_scheme);

    let host_part = host_part.rsplit('@').next().unwrap_or(host_part);
    let host_part = host_part.split(':').next().unwrap_or(host_part);

    let host = host_part.trim_matches('.').to_lowercase();
    if host.is_empty() {
        return None;
    }

    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
    if host.is_empty() { None } else { Some(host) }
}

#[cfg(target_os = "macos")]
pub fn get_frontmost_browser_url(bundle_id: &str, _pid: i32) -> Option<String> {
    use std::process::Command;
    use std::time::Duration;

    let script = applescript_for_bundle(bundle_id)?;

    let mut child = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(&script)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null())
        .spawn()
        .ok()?;

    // Bound how long we'll wait so a hung browser can't stall recording start.
    let deadline = std::time::Instant::now() + Duration::from_millis(800);
    let exit_status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    eprintln!("[browser] osascript timed out for bundle={}", bundle_id);
                    return None;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => {
                eprintln!("[browser] osascript wait error: {}", e);
                return None;
            }
        }
    };

    let mut stdout_buf = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        use std::io::Read;
        let _ = stdout.read_to_string(&mut stdout_buf);
    }

    if !exit_status.success() {
        let mut stderr_buf = String::new();
        if let Some(mut stderr) = child.stderr.take() {
            use std::io::Read;
            let _ = stderr.read_to_string(&mut stderr_buf);
        }
        eprintln!(
            "[browser] osascript failed for bundle={} stderr={}",
            bundle_id,
            stderr_buf.trim()
        );
        return None;
    }

    let trimmed = stdout_buf.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("missing value") {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(not(target_os = "macos"))]
pub fn get_frontmost_browser_url(_bundle_id: &str, _pid: i32) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_basic_url() {
        assert_eq!(
            normalize_domain("https://www.GitHub.com/user/repo"),
            Some("github.com".to_string())
        );
    }

    #[test]
    fn keeps_subdomains() {
        assert_eq!(
            normalize_domain("https://mail.google.com/inbox"),
            Some("mail.google.com".to_string())
        );
    }

    #[test]
    fn handles_no_scheme() {
        assert_eq!(
            normalize_domain("example.com:8080/path"),
            Some("example.com".to_string())
        );
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(normalize_domain(""), None);
        assert_eq!(normalize_domain("   "), None);
    }

    #[test]
    fn known_browsers() {
        assert!(is_known_browser("com.google.Chrome"));
        assert!(is_known_browser("com.apple.Safari"));
        assert!(is_known_browser("company.thebrowser.Browser"));
        assert!(!is_known_browser("com.apple.finder"));
    }

    #[test]
    fn applescript_dispatch() {
        let safari = applescript_for_bundle("com.apple.Safari").unwrap();
        assert!(safari.contains("front document"));
        assert!(safari.contains("com.apple.Safari"));

        let chrome = applescript_for_bundle("com.google.Chrome").unwrap();
        assert!(chrome.contains("active tab"));

        let arc = applescript_for_bundle("company.thebrowser.Browser").unwrap();
        assert!(arc.contains("active tab"));
        assert!(arc.contains("company.thebrowser.Browser"));

        assert!(applescript_for_bundle("com.apple.finder").is_none());
    }
}
