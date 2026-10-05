//! User settings, kept in `settings.json` in the app's data directory. The
//! file keeps the layout the Tauri store wrote, so settings carry over.

use gpui_kit::{App, AppContext as _, Context, Entity, Global};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::PathBuf;

use crate::i18n::Locale;

pub const DEFAULT_HOTKEY: &str = "LeftControl+LeftOption";
#[cfg(target_os = "macos")]
pub const DEFAULT_HOLD_TO_SPEAK_HOTKEY: &str = "RightCommand";
/// Right Command is the right Super key on a PC keyboard, which most laptops
/// do not have. Nearly all have a right Control.
#[cfg(not(target_os = "macos"))]
pub const DEFAULT_HOLD_TO_SPEAK_HOTKEY: &str = "RightControl";

pub const ONBOARDING_STEP_COUNT: u32 = 8;
pub const CURRENT_ONBOARDING_VERSION: u32 = 4;
pub const MODEL_DOWNLOAD_STEP: u32 = 6;

/// The transcription styles, in the order the interface lists them.
pub const STYLE_LIST: [&str; 4] = ["professional", "casual", "agents", "default"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Dark,
    Light,
    #[default]
    System,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub theme: ThemeMode,
    pub hotkey: String,
    pub hold_to_speak_hotkey: String,
    pub selected_microphone_id: Option<String>,
    pub language: Locale,
    pub mute_media_while_recording: bool,
    pub muffle_media_while_recording: bool,
    pub recording_sounds_enabled: bool,
    pub default_style: String,
    pub analytics_enabled: bool,
    pub onboarding_complete: bool,
    pub onboarding_step: u32,
    pub onboarding_version: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::System,
            hotkey: DEFAULT_HOTKEY.into(),
            hold_to_speak_hotkey: DEFAULT_HOLD_TO_SPEAK_HOTKEY.into(),
            selected_microphone_id: None,
            language: Locale::En,
            mute_media_while_recording: false,
            muffle_media_while_recording: false,
            recording_sounds_enabled: false,
            default_style: "default".into(),
            analytics_enabled: true,
            onboarding_complete: false,
            onboarding_step: 1,
            onboarding_version: CURRENT_ONBOARDING_VERSION,
        }
    }
}

pub fn normalize_onboarding_step(step: u32, onboarding_complete: bool, version: u32) -> u32 {
    if onboarding_complete || version < CURRENT_ONBOARDING_VERSION {
        return 1;
    }

    step.clamp(1, ONBOARDING_STEP_COUNT)
}

impl Settings {
    /// Reads settings from stored values, tolerating missing and malformed
    /// entries one by one so a single bad value does not reset the rest.
    fn from_stored(stored: &Map<String, Value>) -> Self {
        let defaults = Settings::default();
        let get = |key: &str| stored.get(key).filter(|value| !value.is_null());

        fn parse<T: serde::de::DeserializeOwned>(value: Option<&Value>) -> Option<T> {
            value.and_then(|value| serde_json::from_value(value.clone()).ok())
        }

        let onboarding_complete =
            parse(get("onboardingComplete")).unwrap_or(defaults.onboarding_complete);
        // Files written before onboarding was versioned are version 1.
        let stored_version = parse(get("onboardingVersion")).unwrap_or(1);
        let default_style = parse::<String>(get("defaultStyle"))
            .filter(|style| STYLE_LIST.contains(&style.as_str()))
            .unwrap_or(defaults.default_style);

        Settings {
            theme: parse(get("theme")).unwrap_or(defaults.theme),
            hotkey: parse(get("hotkey")).unwrap_or(defaults.hotkey),
            hold_to_speak_hotkey: parse(get("holdToSpeakHotkey"))
                .unwrap_or(defaults.hold_to_speak_hotkey),
            selected_microphone_id: parse(get("selectedMicrophoneId")),
            language: parse(get("language")).unwrap_or(defaults.language),
            mute_media_while_recording: parse(get("muteMediaWhileRecording"))
                .unwrap_or(defaults.mute_media_while_recording),
            muffle_media_while_recording: parse(get("muffleMediaWhileRecording"))
                .unwrap_or(defaults.muffle_media_while_recording),
            recording_sounds_enabled: parse(get("recordingSoundsEnabled"))
                .unwrap_or(defaults.recording_sounds_enabled),
            default_style,
            analytics_enabled: parse(get("analyticsEnabled")).unwrap_or(defaults.analytics_enabled),
            onboarding_complete,
            onboarding_step: normalize_onboarding_step(
                parse(get("onboardingStep")).unwrap_or(defaults.onboarding_step),
                onboarding_complete,
                stored_version,
            ),
            onboarding_version: CURRENT_ONBOARDING_VERSION,
        }
    }
}

/// The stored values at `path`, and whether a file is there but unreadable.
/// A missing file is a first launch, not an error.
fn read_stored(path: Option<&std::path::Path>) -> (Map<String, Value>, bool) {
    let Some(contents) = path.and_then(|path| match std::fs::read_to_string(path) {
        Ok(contents) => Some(Ok(contents)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => Some(Err(error.to_string())),
    }) else {
        return (Map::new(), false);
    };

    match contents.and_then(|contents| {
        serde_json::from_str::<Map<String, Value>>(&contents).map_err(|error| error.to_string())
    }) {
        Ok(stored) => (stored, false),
        Err(error) => {
            log::error!(target: "settings", "load_failed error={error}");
            (Map::new(), true)
        }
    }
}

/// The live settings. Observe the entity to react to changes.
pub struct SettingsStore {
    settings: Settings,
    /// Everything in the file, including keys this version does not know.
    stored: Map<String, Value>,
    path: Option<PathBuf>,
}

struct GlobalSettings(Entity<SettingsStore>);

impl Global for GlobalSettings {}

impl SettingsStore {
    /// Loads the settings at `path`. Without a path nothing is persisted.
    pub fn init(path: Option<PathBuf>, initial: Option<Map<String, Value>>, cx: &mut App) {
        let (stored, unreadable) = match initial {
            Some(initial) => (initial, false),
            None => read_stored(path.as_deref()),
        };

        let mut settings = Settings::from_stored(&stored);
        if unreadable {
            // The file holds the user's analytics choice. When it cannot be
            // read, the choice is unknown, and unknown must not mean yes.
            settings.analytics_enabled = false;
        }

        let store = cx.new(|_| SettingsStore {
            settings,
            stored,
            path,
        });

        // Normalizing may have changed the stored onboarding step or version.
        store.update(cx, |store, _| store.save());
        cx.set_global(GlobalSettings(store));
    }

    pub fn entity(cx: &App) -> Entity<SettingsStore> {
        cx.global::<GlobalSettings>().0.clone()
    }

    pub fn get(cx: &App) -> &Settings {
        &cx.global::<GlobalSettings>().0.read(cx).settings
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Changes the settings, saves them and notifies observers.
    pub fn update(cx: &mut App, change: impl FnOnce(&mut Settings)) {
        Self::entity(cx).update(cx, |store, cx: &mut Context<SettingsStore>| {
            let before = store.settings.clone();
            change(&mut store.settings);

            if store.settings != before {
                store.save();
                cx.notify();
            }
        });
    }

    fn save(&mut self) {
        let Ok(Value::Object(current)) = serde_json::to_value(&self.settings) else {
            return;
        };

        let mut changed = false;
        for (key, value) in current {
            if self.stored.get(&key) != Some(&value) {
                self.stored.insert(key, value);
                changed = true;
            }
        }

        let Some(path) = self.path.as_ref().filter(|_| changed) else {
            return;
        };

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        match serde_json::to_string_pretty(&self.stored) {
            Ok(contents) => {
                if let Err(error) = std::fs::write(path, contents) {
                    log::error!(target: "settings", "save_failed error={error}");
                }
            }
            Err(error) => log::error!(target: "settings", "serialize_failed error={error}"),
        }
    }
}

/// Muting and muffling are alternatives: turning one on turns the other off.
pub fn set_mute_media(cx: &mut App, enabled: bool) {
    SettingsStore::update(cx, |settings| {
        settings.mute_media_while_recording = enabled;
        if enabled {
            settings.muffle_media_while_recording = false;
        }
    });
}

pub fn set_muffle_media(cx: &mut App, enabled: bool) {
    SettingsStore::update(cx, |settings| {
        settings.muffle_media_while_recording = enabled;
        if enabled {
            settings.mute_media_while_recording = false;
        }
    });
}

pub fn mark_onboarding_complete(cx: &mut App) {
    SettingsStore::update(cx, |settings| {
        settings.onboarding_complete = true;
        settings.onboarding_step = 1;
    });
}

pub fn resume_onboarding_at(cx: &mut App, step: u32) {
    SettingsStore::update(cx, |settings| {
        settings.onboarding_complete = false;
        settings.onboarding_step = step.clamp(1, ONBOARDING_STEP_COUNT);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn stored(value: Value) -> Map<String, Value> {
        value.as_object().cloned().unwrap()
    }

    #[test]
    fn reads_the_file_the_tauri_store_wrote() {
        let settings = Settings::from_stored(&stored(json!({
            "theme": "dark",
            "hotkey": "Command+Shift+K",
            "holdToSpeakHotkey": "RightOption",
            "selectedMicrophoneId": null,
            "language": "ru",
            "muteMediaWhileRecording": true,
            "defaultStyle": "casual",
            "analyticsEnabled": false,
            "onboardingComplete": true,
            "onboardingStep": 5,
            "onboardingVersion": 4
        })));

        assert_eq!(settings.theme, ThemeMode::Dark);
        assert_eq!(settings.hotkey, "Command+Shift+K");
        assert_eq!(settings.hold_to_speak_hotkey, "RightOption");
        assert_eq!(settings.selected_microphone_id, None);
        assert_eq!(settings.language, Locale::Ru);
        assert!(settings.mute_media_while_recording);
        assert_eq!(settings.default_style, "casual");
        assert!(!settings.analytics_enabled);
        assert!(settings.onboarding_complete);
        assert_eq!(settings.onboarding_step, 1);
    }

    #[test]
    fn one_bad_value_does_not_reset_the_others() {
        let settings = Settings::from_stored(&stored(json!({
            "theme": 7,
            "defaultStyle": "shouting",
            "hotkey": "Command+K"
        })));

        assert_eq!(settings.theme, ThemeMode::System);
        assert_eq!(settings.default_style, "default");
        assert_eq!(settings.hotkey, "Command+K");
    }

    #[test]
    fn a_missing_file_is_a_first_launch_and_a_broken_one_is_unreadable() {
        let directory = tempfile::tempdir().unwrap();

        let missing = directory.path().join("settings.json");
        assert_eq!(read_stored(Some(&missing)), (Map::new(), false));
        assert_eq!(read_stored(None), (Map::new(), false));

        let broken = directory.path().join("broken.json");
        std::fs::write(&broken, "{ not json").unwrap();
        assert_eq!(read_stored(Some(&broken)), (Map::new(), true));

        let valid = directory.path().join("valid.json");
        std::fs::write(&valid, r#"{ "theme": "dark" }"#).unwrap();
        let (stored, unreadable) = read_stored(Some(&valid));
        assert_eq!(stored.get("theme"), Some(&json!("dark")));
        assert!(!unreadable);
    }

    #[test]
    fn onboarding_restarts_when_its_version_is_older() {
        assert_eq!(normalize_onboarding_step(5, false, 3), 1);
        assert_eq!(normalize_onboarding_step(5, false, 4), 5);
        assert_eq!(normalize_onboarding_step(12, false, 4), 8);
        assert_eq!(normalize_onboarding_step(5, true, 4), 1);
    }
}
