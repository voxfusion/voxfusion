//! Where the app keeps its files. These are the locations the Tauri app
//! used, so settings, history and models carry over.

use std::path::PathBuf;

pub const APP_IDENTIFIER: &str = "io.voxfusion.app";

/// The identifier that names the app's directories. `VOXFUSION_APP_ID` gives
/// a separate profile, for testing without touching personal data.
pub fn app_identifier() -> String {
    std::env::var("VOXFUSION_APP_ID").unwrap_or_else(|_| APP_IDENTIFIER.to_string())
}

fn home_dir() -> PathBuf {
    std::env::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// Settings, the database, models and recordings.
pub fn data_dir() -> PathBuf {
    let base = if cfg!(target_os = "macos") {
        home_dir().join("Library/Application Support")
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or_else(|| home_dir().join(".local/share"))
    };

    base.join(app_identifier())
}

pub fn log_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        home_dir().join("Library/Logs").join(app_identifier())
    } else {
        data_dir().join("logs")
    }
}

pub fn settings_file() -> PathBuf {
    data_dir().join("settings.json")
}

/// The `Contents/Resources` directory of the app bundle, when running from one.
pub fn resource_dir() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let contents = executable.parent()?.parent()?;

    (contents.file_name()? == "Contents").then(|| contents.join("Resources"))
}
