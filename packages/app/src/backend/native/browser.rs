#[cfg(any(target_os = "macos", test))]
const SAFARI_BUNDLES: &[&str] = &["com.apple.Safari", "com.apple.SafariTechnologyPreview"];

#[cfg(any(target_os = "macos", test))]
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

#[cfg(any(target_os = "windows", test))]
const WINDOWS_BROWSERS: &[&str] = &[
    "msedge.exe",
    "chrome.exe",
    "brave.exe",
    "firefox.exe",
    "vivaldi.exe",
    "opera.exe",
    "arc.exe",
    "zen.exe",
    "chromium.exe",
    "waterfox.exe",
    "librewolf.exe",
    "floorp.exe",
    "thorium.exe",
];

#[cfg(any(target_os = "macos", target_os = "windows", test))]
pub fn is_known_browser(bundle_id: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        return SAFARI_BUNDLES
            .iter()
            .chain(CHROMIUM_BUNDLES.iter())
            .any(|b| b.eq_ignore_ascii_case(bundle_id));
    }

    #[cfg(target_os = "windows")]
    {
        if WINDOWS_BROWSERS
            .iter()
            .any(|b| b.eq_ignore_ascii_case(bundle_id))
        {
            return true;
        }
        #[cfg(test)]
        {
            return SAFARI_BUNDLES
                .iter()
                .chain(CHROMIUM_BUNDLES.iter())
                .any(|b| b.eq_ignore_ascii_case(bundle_id));
        }
        #[cfg(not(test))]
        return false;
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        SAFARI_BUNDLES
            .iter()
            .chain(CHROMIUM_BUNDLES.iter())
            .chain(WINDOWS_BROWSERS.iter())
            .any(|b| b.eq_ignore_ascii_case(bundle_id))
    }
}

#[cfg(any(target_os = "macos", test))]
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

#[cfg(test)]
use crate::backend::normalize_domain;

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

#[cfg(target_os = "windows")]
pub fn get_frontmost_browser_url(bundle_id: &str, hwnd_raw: isize) -> Option<String> {
    use std::sync::mpsc;
    use std::time::Duration;

    if !is_known_browser(bundle_id) || hwnd_raw == 0 {
        return None;
    }

    let bundle_owned = bundle_id.to_string();
    let (tx, rx) = mpsc::channel();
    let _ = std::thread::Builder::new()
        .name("browser-url-uia".into())
        .spawn(move || {
            let url = query_browser_url_uia(&bundle_owned, hwnd_raw);
            let _ = tx.send(url);
        });

    // Bound wait so an unresponsive browser never stalls recording start.
    rx.recv_timeout(Duration::from_millis(800)).ok().flatten()
}

#[cfg(target_os = "windows")]
fn read_uia_element_url(elem: &windows::Win32::UI::Accessibility::IUIAutomationElement) -> Option<String> {
    use windows::Win32::UI::Accessibility::{IUIAutomationValuePattern, UIA_ValuePatternId};
    use windows::core::Interface;

    let pattern_unk = unsafe { elem.GetCurrentPattern(UIA_ValuePatternId) }.ok()?;
    let value_pattern = pattern_unk.cast::<IUIAutomationValuePattern>().ok()?;
    let value_bstr = unsafe { value_pattern.CurrentValue() }.ok()?;
    let value = value_bstr.to_string();
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_whitespace) {
        return None;
    }
    Some(trimmed.to_string())
}

#[cfg(target_os = "windows")]
fn query_browser_url_uia(bundle_id: &str, hwnd_raw: isize) -> Option<String> {
    use std::collections::VecDeque;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, UIA_ComboBoxControlTypeId, UIA_DocumentControlTypeId,
        UIA_EditControlTypeId,
    };

    const MAX_CHROME_NODES: usize = 384;

    struct ComGuard(bool);
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    let com_ok = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let _com = ComGuard(com_ok);

    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }.ok()?;
    let root = unsafe { automation.ElementFromHandle(HWND(hwnd_raw as *mut _)) }.ok()?;
    let walker = unsafe { automation.ControlViewWalker() }.ok()?;

    let is_gecko = matches!(
        bundle_id.to_ascii_lowercase().as_str(),
        "firefox.exe" | "zen.exe" | "waterfox.exe" | "librewolf.exe" | "floorp.exe"
    );

    // Traverse browser chrome via BFS while pruning `Document` subtrees so web
    // page form inputs can never spoof the browser address bar or stall the walk.
    let mut queue = VecDeque::new();
    queue.push_back(root);
    let mut visited = 0usize;

    while let Some(elem) = queue.pop_front() {
        visited += 1;
        if visited > MAX_CHROME_NODES {
            break;
        }

        let ctrl_type = unsafe { elem.CurrentControlType() }.ok();
        if ctrl_type == Some(UIA_DocumentControlTypeId) {
            continue;
        }

        let is_candidate = ctrl_type == Some(UIA_EditControlTypeId)
            || (is_gecko && ctrl_type == Some(UIA_ComboBoxControlTypeId));

        if is_candidate {
            let automation_id = unsafe { elem.CurrentAutomationId() }
                .map(|b| b.to_string())
                .unwrap_or_default();
            let name = unsafe { elem.CurrentName() }
                .map(|b| b.to_string())
                .unwrap_or_default();
            let name_lower = name.to_ascii_lowercase();

            let is_address_bar = if is_gecko {
                automation_id == "urlbar-input"
                    || name_lower.contains("address")
                    || name_lower.contains("search with")
            } else {
                automation_id == "view_1022"
                    || name_lower.contains("address")
                    || name_lower.contains("url")
                    || name_lower.contains("omnibox")
                    || name_lower.contains("location")
            };

            if is_address_bar
                && let Some(trimmed) = read_uia_element_url(&elem)
                && crate::backend::browser_domain(&trimmed).is_some()
            {
                return Some(trimmed);
            }
        }

        let Ok(mut child) = (unsafe { walker.GetFirstChildElement(&elem) }) else {
            continue;
        };
        loop {
            let next = unsafe { walker.GetNextSiblingElement(&child) }.ok();
            queue.push_back(child);
            match next {
                Some(sibling) => child = sibling,
                None => break,
            }
        }
    }

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
