use super::apps::{FrontmostApp, InstalledApp};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn desktop_fields(text: &str) -> HashMap<&str, &str> {
    let mut active = false;
    let mut fields = HashMap::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            active = line == "[Desktop Entry]";
            continue;
        }
        if active && !line.starts_with('#') {
            if let Some((key, value)) = line.split_once('=') {
                fields.insert(key.trim(), value.trim());
            }
        }
    }
    fields
}

fn application_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(data) = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
    {
        dirs.push(data.join("applications"));
    }
    let data_dirs =
        std::env::var_os("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    dirs.extend(std::env::split_paths(&data_dirs).map(|dir| dir.join("applications")));
    dirs
}

fn scan(
    dir: &Path,
    base: &Path,
    depth: u8,
    seen: &mut HashSet<String>,
    apps: &mut Vec<InstalledApp>,
) {
    if depth > 8 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            scan(&path, base, depth + 1, seen, apps);
            continue;
        }
        if path.extension().is_none_or(|ext| ext != "desktop") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let fields = desktop_fields(&text);
        let id = path
            .strip_prefix(base)
            .unwrap_or(&path)
            .to_string_lossy()
            .trim_end_matches(".desktop")
            .replace('/', "-");
        // User overrides (including Hidden=true) take precedence over system entries.
        if !seen.insert(id.clone())
            || fields.get("Hidden") == Some(&"true")
            || fields.get("NoDisplay") == Some(&"true")
            || fields.get("Type") != Some(&"Application")
        {
            continue;
        }
        let Some(name) = fields.get("Name") else {
            continue;
        };
        apps.push(InstalledApp {
            name: name.to_string(),
            bundle_id: id,
            path: path.to_string_lossy().into_owned(),
            icon_data_url: None,
        });
    }
}

pub fn installed_apps() -> Vec<InstalledApp> {
    let mut apps = Vec::new();
    let mut seen = HashSet::new();
    for dir in application_dirs() {
        scan(&dir, &dir, 0, &mut seen, &mut apps);
    }
    apps.sort_by_key(|app| app.name.to_lowercase());
    apps
}

pub fn frontmost_app() -> Result<Option<FrontmostApp>, String> {
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none() {
        return Ok(None);
    }
    let output = Command::new("hyprctl")
        .args(["-j", "activewindow"])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let window: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
    let Some(class) = window["class"].as_str().filter(|c| !c.is_empty()) else {
        return Ok(None);
    };
    let app = installed_apps().into_iter().find(|app| {
        if app.bundle_id.eq_ignore_ascii_case(class) {
            return true;
        }
        fs::read_to_string(&app.path).ok().is_some_and(|text| {
            desktop_fields(&text)
                .get("StartupWMClass")
                .is_some_and(|value| value.eq_ignore_ascii_case(class))
        })
    });
    let (name, bundle_id) = app
        .map(|app| (app.name, app.bundle_id))
        .unwrap_or_else(|| (class.to_string(), class.to_string()));
    // Wayland does not expose browser URLs. Never infer a website from a title.
    Ok(Some(FrontmostApp {
        name,
        bundle_id,
        url: None,
        domain: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_entries_ignore_actions_and_preserve_values() {
        let fields = desktop_fields(
            "# comment\n[Desktop Entry]\nType=Application\nName=My App\nStartupWMClass=org.example.App\nExec=app --value=a=b\n[Desktop Action New]\nName=Other\n",
        );
        assert_eq!(fields["Name"], "My App");
        assert_eq!(fields["Exec"], "app --value=a=b");
        assert_eq!(fields["StartupWMClass"], "org.example.App");
    }
}
