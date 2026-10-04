//! Runs the interface against canned data instead of the native services.
//!
//! `VOXFUSION_FIXTURE` names a scenario file: the settings and data to start
//! with, which window to show and at what size. Scripted events arrive as
//! JSON lines on stdin. This is how the interface is screenshotted and
//! checked on machines without a microphone, models or macOS.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::backend::*;

#[derive(Debug, Clone, Deserialize)]
pub struct Scenario {
    /// The system appearance the scenario was recorded with: `light` or `dark`.
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default)]
    pub window: ScenarioWindow,
    #[serde(default = "default_route")]
    pub route: String,
    pub size: (f32, f32),
    pub fixture: FixtureData,
}

fn default_route() -> String {
    "/".into()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScenarioWindow {
    #[default]
    Main,
    VoiceControl,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixtureData {
    pub now: Option<DateTime<Utc>>,
    pub app_version: String,
    pub settings: Map<String, Value>,
    pub permissions: FixturePermissions,
    #[serde(default)]
    pub model_ready: bool,
    #[serde(default)]
    pub models: Vec<ModelInfo>,
    #[serde(default)]
    pub audio_devices: Vec<AudioDevice>,
    #[serde(default)]
    pub transcriptions: Vec<Transcription>,
    #[serde(default)]
    pub dictionary_words: Vec<DictionaryWord>,
    #[serde(default)]
    pub app_dictionaries: Vec<AppDictionary>,
    #[serde(default)]
    pub site_dictionaries: Vec<SiteDictionary>,
    #[serde(default)]
    pub app_instructions: Vec<AppInstruction>,
    #[serde(default)]
    pub site_styles: Vec<SiteStyle>,
    #[serde(default)]
    pub installed_apps: Vec<InstalledApp>,
    #[serde(default)]
    pub frontmost_app: Option<FrontmostApp>,
    #[serde(default)]
    pub update: Option<UpdateInfo>,
    /// The step the voice overlay's wave is held at.
    #[serde(default)]
    pub wave_offset: usize,
    /// Commands that never answer, to hold a loading state.
    #[serde(default)]
    pub pending: HashMap<String, bool>,
    /// Commands that fail, with their error.
    #[serde(default)]
    pub failures: HashMap<String, String>,
    /// Commands that take a while to answer, with the delay in milliseconds.
    #[serde(default)]
    pub latency: HashMap<String, u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixturePermissions {
    /// `granted`, `prompt`, `denied`, or `checking` to never answer.
    pub microphone: String,
    /// What asking for the microphone leads to: `granted`, `denied`, `pending`.
    #[serde(default = "granted")]
    pub microphone_request: String,
    pub accessibility: bool,
}

fn granted() -> String {
    "granted".into()
}

pub fn load_scenario() -> Option<Scenario> {
    let path = std::env::var_os("VOXFUSION_FIXTURE")?;
    let contents = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read fixture {path:?}: {error}"));

    Some(
        serde_json::from_str(&contents)
            .unwrap_or_else(|error| panic!("cannot parse fixture {path:?}: {error}")),
    )
}

/// The clock scenarios run on: the scenario's start time, advancing normally.
static CLOCK: Mutex<Option<(DateTime<Utc>, Instant)>> = Mutex::new(None);

pub fn set_clock(now: DateTime<Utc>) {
    *CLOCK.lock().unwrap() = Some((now, Instant::now()));
}

pub fn now() -> Option<DateTime<Utc>> {
    let (start, started_at) = (*CLOCK.lock().unwrap())?;
    Some(start + chrono::Duration::from_std(started_at.elapsed()).ok()?)
}

pub struct FixtureBackend {
    data: Mutex<FixtureData>,
    next_id: Mutex<u32>,
    /// Counts the cancellations asked for; downloads wait for the next one.
    download_cancellations: (Mutex<u32>, Condvar),
}

fn block_forever() -> ! {
    loop {
        std::thread::park();
    }
}

impl FixtureBackend {
    pub fn new(data: FixtureData) -> Self {
        Self {
            data: Mutex::new(data),
            next_id: Mutex::new(1000),
            download_cancellations: (Mutex::new(0), Condvar::new()),
        }
    }

    /// Applies the scenario's `latency`, `pending` and `failures` for
    /// `command`.
    fn gate(&self, command: &str) -> CommandResult<()> {
        let (latency, pending, failure) = {
            let data = self.data.lock().unwrap();
            (
                data.latency.get(command).copied(),
                data.pending.get(command).copied().unwrap_or(false),
                data.failures.get(command).cloned(),
            )
        };

        if let Some(milliseconds) = latency {
            std::thread::sleep(Duration::from_millis(milliseconds));
        }
        if let Some(failure) = failure {
            return Err(failure);
        }
        if pending {
            block_forever();
        }

        Ok(())
    }

    fn new_word(&self, word: &str) -> DictionaryWord {
        let mut next_id = self.next_id.lock().unwrap();
        *next_id += 1;
        let now = Utc::now().to_rfc3339();

        DictionaryWord {
            id: format!("mock-{}", *next_id),
            word: word.to_string(),
            created_at: now.clone(),
            updated_at: now,
        }
    }

    fn with<T>(&self, change: impl FnOnce(&mut FixtureData) -> T) -> T {
        change(&mut self.data.lock().unwrap())
    }
}

impl Backend for FixtureBackend {
    fn type_text(&self, _text: &str) -> CommandResult<()> {
        self.gate("type_text")
    }

    fn check_accessibility(&self) -> bool {
        // A probe that fails counts as "not granted", as it did in the
        // interface this was ported from.
        if self.gate("check_accessibility_probe").is_err() {
            return false;
        }

        self.with(|data| data.permissions.accessibility)
    }

    fn request_accessibility(&self) {}

    fn microphone_permission(&self) -> PermissionState {
        match self
            .with(|data| data.permissions.microphone.clone())
            .as_str()
        {
            "granted" => PermissionState::Granted,
            "denied" => PermissionState::Denied,
            "checking" => block_forever(),
            _ => PermissionState::Prompt,
        }
    }

    fn request_microphone_permission(&self) -> bool {
        match self
            .with(|data| data.permissions.microphone_request.clone())
            .as_str()
        {
            "granted" => {
                self.with(|data| data.permissions.microphone = "granted".into());
                true
            }
            "pending" => block_forever(),
            _ => {
                self.with(|data| data.permissions.microphone = "denied".into());
                false
            }
        }
    }

    fn start_system_key_watcher(&self) -> CommandResult<()> {
        self.gate("start_system_key_watcher")
    }

    fn list_audio_devices(&self) -> CommandResult<Vec<AudioDevice>> {
        self.gate("list_audio_devices")?;
        Ok(self.with(|data| data.audio_devices.clone()))
    }

    fn start_recording(&self, _device_name: Option<String>) -> CommandResult<()> {
        self.gate("start_recording_with_device")
    }

    fn stop_recording(&self) -> CommandResult<PathBuf> {
        self.gate("stop_recording_with_device")?;
        Ok(PathBuf::from("/tmp/mock-recording.wav"))
    }

    fn mute_media_for_recording(&self) -> CommandResult<()> {
        Ok(())
    }

    fn muffle_media_for_recording(&self) {}

    fn restore_media_after_recording(&self) -> CommandResult<()> {
        Ok(())
    }

    fn request_muffle_permission(&self) {}

    fn transcribe_audio(
        &self,
        _audio_path: PathBuf,
        _bundle_id: Option<String>,
        _domain: Option<String>,
        _fallback_style: String,
    ) -> CommandResult<TranscriptionResult> {
        self.gate("transcribe_audio")?;
        Ok(TranscriptionResult {
            text: "Mock transcription".into(),
            word_count: 2,
            processing_time_ms: 420,
            audio_duration_ms: Some(1800),
        })
    }

    fn save_transcription(&self, result: &TranscriptionResult) -> CommandResult<Transcription> {
        let word = self.new_word("");
        Ok(Transcription {
            id: word.id,
            text: result.text.clone(),
            word_count: result.word_count,
            processing_time_ms: result.processing_time_ms,
            audio_duration_ms: result.audio_duration_ms,
            created_at: word.created_at,
        })
    }

    fn list_transcriptions(
        &self,
        limit: i64,
        cursor: Option<String>,
    ) -> CommandResult<TranscriptionPage> {
        self.gate("list_transcriptions")?;
        Ok(self.with(|data| {
            let items: Vec<_> = data
                .transcriptions
                .iter()
                .filter(|item| {
                    cursor
                        .as_ref()
                        .is_none_or(|cursor| &item.created_at < cursor)
                })
                .cloned()
                .collect();
            let limit = limit.max(0) as usize;

            TranscriptionPage {
                has_more: items.len() > limit,
                transcriptions: items.into_iter().take(limit).collect(),
            }
        }))
    }

    fn check_model_status(&self) -> CommandResult<bool> {
        self.gate("check_model_status")?;
        Ok(self.with(|data| data.model_ready))
    }

    fn list_models(&self) -> CommandResult<Vec<ModelInfo>> {
        self.gate("list_models")?;
        Ok(self.with(|data| data.models.clone()))
    }

    fn set_active_model(&self, model_id: &str) -> CommandResult<()> {
        self.gate("set_active_model")?;
        self.with(|data| {
            for model in &mut data.models {
                model.active = model.id == model_id;
            }
        });
        Ok(())
    }

    fn download_model(&self, model_id: &str) -> CommandResult<()> {
        self.gate("download_model")?;
        if model_id == DEFAULT_MODEL_ID {
            self.gate("download_whisper_model")?;
        }
        // Progress is scripted through events; a download ends only by being
        // cancelled.
        let (cancellations, cancelled) = &self.download_cancellations;
        let mut count = cancellations.lock().unwrap();
        let seen = *count;
        while *count == seen {
            count = cancelled.wait(count).unwrap();
        }

        Err(DOWNLOAD_CANCELLED_ERROR.into())
    }

    fn cancel_model_download(&self, _model_id: &str) -> CommandResult<()> {
        self.gate("cancel_model_download")?;

        let (cancellations, cancelled) = &self.download_cancellations;
        *cancellations.lock().unwrap() += 1;
        cancelled.notify_all();
        Ok(())
    }

    fn list_dictionary_words(&self) -> CommandResult<Vec<DictionaryWord>> {
        self.gate("list_dictionary_words")?;
        Ok(self.with(|data| data.dictionary_words.clone()))
    }

    fn add_dictionary_word(&self, word: &str) -> CommandResult<DictionaryWord> {
        self.gate("add_dictionary_word")?;
        let word = self.new_word(word);
        self.with(|data| data.dictionary_words.insert(0, word.clone()));
        Ok(word)
    }

    fn update_dictionary_word(&self, id: &str, word: &str) -> CommandResult<DictionaryWord> {
        self.gate("update_dictionary_word")?;
        self.with(|data| {
            let entry = data
                .dictionary_words
                .iter_mut()
                .find(|entry| entry.id == id)
                .ok_or("Word not found")?;
            entry.word = word.to_string();
            Ok(entry.clone())
        })
    }

    fn delete_dictionary_word(&self, id: &str) -> CommandResult<()> {
        self.gate("delete_dictionary_word")?;
        self.with(|data| data.dictionary_words.retain(|entry| entry.id != id));
        Ok(())
    }

    fn list_app_dictionaries(&self) -> CommandResult<Vec<AppDictionary>> {
        self.gate("list_app_dictionaries")?;
        Ok(self.with(|data| data.app_dictionaries.clone()))
    }

    fn add_app_dictionary_word(
        &self,
        bundle_id: &str,
        app_name: &str,
        word: &str,
    ) -> CommandResult<DictionaryWord> {
        self.gate("add_app_dictionary_word")?;
        let word = self.new_word(word);
        self.with(|data| {
            let position = data
                .app_dictionaries
                .iter()
                .position(|group| group.bundle_id == bundle_id)
                .unwrap_or_else(|| {
                    data.app_dictionaries.push(AppDictionary {
                        bundle_id: bundle_id.to_string(),
                        app_name: app_name.to_string(),
                        words: Vec::new(),
                    });
                    data.app_dictionaries.len() - 1
                });
            data.app_dictionaries[position].words.push(word.clone());
        });
        Ok(word)
    }

    fn update_app_dictionary_word(&self, id: &str, word: &str) -> CommandResult<DictionaryWord> {
        self.gate("update_app_dictionary_word")?;
        self.with(|data| {
            let entry = data
                .app_dictionaries
                .iter_mut()
                .flat_map(|group| group.words.iter_mut())
                .find(|entry| entry.id == id)
                .ok_or("Word not found")?;
            entry.word = word.to_string();
            Ok(entry.clone())
        })
    }

    fn delete_app_dictionary_word(&self, id: &str) -> CommandResult<()> {
        self.gate("delete_app_dictionary_word")?;
        self.with(|data| {
            for group in &mut data.app_dictionaries {
                group.words.retain(|entry| entry.id != id);
            }
        });
        Ok(())
    }

    fn delete_app_dictionary(&self, bundle_id: &str) -> CommandResult<()> {
        self.gate("delete_app_dictionary")?;
        self.with(|data| {
            data.app_dictionaries
                .retain(|group| group.bundle_id != bundle_id)
        });
        Ok(())
    }

    fn list_site_dictionaries(&self) -> CommandResult<Vec<SiteDictionary>> {
        self.gate("list_site_dictionaries")?;
        Ok(self.with(|data| data.site_dictionaries.clone()))
    }

    fn add_site_dictionary_word(&self, domain: &str, word: &str) -> CommandResult<DictionaryWord> {
        self.gate("add_site_dictionary_word")?;
        let word = self.new_word(word);
        self.with(|data| {
            let position = data
                .site_dictionaries
                .iter()
                .position(|group| group.domain == domain)
                .unwrap_or_else(|| {
                    data.site_dictionaries.push(SiteDictionary {
                        domain: domain.to_string(),
                        words: Vec::new(),
                    });
                    data.site_dictionaries.len() - 1
                });
            data.site_dictionaries[position].words.push(word.clone());
        });
        Ok(word)
    }

    fn update_site_dictionary_word(&self, id: &str, word: &str) -> CommandResult<DictionaryWord> {
        self.gate("update_site_dictionary_word")?;
        self.with(|data| {
            let entry = data
                .site_dictionaries
                .iter_mut()
                .flat_map(|group| group.words.iter_mut())
                .find(|entry| entry.id == id)
                .ok_or("Word not found")?;
            entry.word = word.to_string();
            Ok(entry.clone())
        })
    }

    fn delete_site_dictionary_word(&self, id: &str) -> CommandResult<()> {
        self.gate("delete_site_dictionary_word")?;
        self.with(|data| {
            for group in &mut data.site_dictionaries {
                group.words.retain(|entry| entry.id != id);
            }
        });
        Ok(())
    }

    fn delete_site_dictionary(&self, domain: &str) -> CommandResult<()> {
        self.gate("delete_site_dictionary")?;
        self.with(|data| {
            data.site_dictionaries
                .retain(|group| group.domain != domain)
        });
        Ok(())
    }

    fn list_installed_apps(&self) -> CommandResult<Vec<InstalledApp>> {
        self.gate("list_installed_apps")?;
        Ok(self.with(|data| data.installed_apps.clone()))
    }

    fn get_frontmost_app(&self) -> CommandResult<Option<FrontmostApp>> {
        self.gate("get_frontmost_app")?;
        Ok(self.with(|data| data.frontmost_app.clone()))
    }

    fn list_app_instructions(&self) -> CommandResult<Vec<AppInstruction>> {
        self.gate("list_app_instructions")?;
        Ok(self.with(|data| data.app_instructions.clone()))
    }

    fn set_app_instruction(
        &self,
        bundle_id: &str,
        app_name: &str,
        style: &str,
    ) -> CommandResult<AppInstruction> {
        self.gate("set_app_instruction")?;
        let fresh = self.new_word("");
        Ok(self.with(|data| {
            if let Some(existing) = data
                .app_instructions
                .iter_mut()
                .find(|instruction| instruction.bundle_id == bundle_id)
            {
                existing.style = style.to_string();
                return existing.clone();
            }

            let instruction = AppInstruction {
                id: fresh.id,
                bundle_id: bundle_id.to_string(),
                app_name: app_name.to_string(),
                style: style.to_string(),
                created_at: fresh.created_at,
                updated_at: fresh.updated_at,
            };
            data.app_instructions.push(instruction.clone());
            instruction
        }))
    }

    fn delete_app_instruction(&self, id: &str) -> CommandResult<()> {
        self.gate("delete_app_instruction")?;
        self.with(|data| {
            data.app_instructions
                .retain(|instruction| instruction.id != id)
        });
        Ok(())
    }

    fn list_site_styles(&self) -> CommandResult<Vec<SiteStyle>> {
        self.gate("list_site_styles")?;
        Ok(self.with(|data| data.site_styles.clone()))
    }

    fn set_site_style(&self, domain: &str, style: &str) -> CommandResult<SiteStyle> {
        self.gate("set_site_style")?;
        let fresh = self.new_word("");
        Ok(self.with(|data| {
            if let Some(existing) = data
                .site_styles
                .iter_mut()
                .find(|site| site.domain == domain)
            {
                existing.style = style.to_string();
                return existing.clone();
            }

            let site = SiteStyle {
                id: fresh.id,
                domain: domain.to_string(),
                style: style.to_string(),
                created_at: fresh.created_at,
                updated_at: fresh.updated_at,
            };
            data.site_styles.push(site.clone());
            site
        }))
    }

    fn delete_site_style(&self, id: &str) -> CommandResult<()> {
        self.gate("delete_site_style")?;
        self.with(|data| data.site_styles.retain(|site| site.id != id));
        Ok(())
    }

    fn register_shortcut(&self, _shortcut: &str) -> CommandResult<()> {
        Ok(())
    }

    fn unregister_shortcut(&self, _shortcut: &str) -> CommandResult<()> {
        Ok(())
    }
}

/// An updater whose download is scripted from stdin.
pub struct FixtureUpdater {
    update: Option<UpdateInfo>,
    download_events: async_channel::Receiver<UpdateDownloadEvent>,
}

impl Updater for FixtureUpdater {
    fn check(&self) -> CommandResult<Option<UpdateInfo>> {
        Ok(self.update.clone())
    }

    fn download_and_install(
        &self,
        _update: &UpdateInfo,
        on_event: &mut dyn FnMut(UpdateDownloadEvent),
    ) -> CommandResult<()> {
        while let Ok(event) = self.download_events.recv_blocking() {
            on_event(event);
        }
        block_forever()
    }

    fn relaunch(&self) {}
}

/// One line of the script on stdin.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Command {
    Emit {
        event: String,
        #[serde(default)]
        payload: Value,
    },
    UpdateEvent {
        message: UpdateDownloadEvent,
    },
    Shortcut {
        shortcut: String,
        state: ShortcutState,
    },
    Wheel(Wheel),
}

/// A scroll by an exact distance, which a virtual display's wheel clicks
/// cannot express.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Wheel {
    pub x: f32,
    pub y: f32,
    /// Positive scrolls the content up, revealing what is below.
    pub dy: f32,
}

fn parse_event(event: String, payload: Value) -> Result<AppEvent, serde_json::Error> {
    let mut wire = Map::new();
    wire.insert("event".into(), Value::String(event));
    if !payload.is_null() {
        wire.insert("payload".into(), payload);
    }

    serde_json::from_value(Value::Object(wire))
}

/// Builds the fixture services and starts reading the script from stdin.
pub fn services(
    data: FixtureData,
    events: EventSender,
) -> (SharedBackend, SharedUpdater, async_channel::Receiver<Wheel>) {
    let (download_sender, download_events) = async_channel::unbounded();
    let (wheel_sender, wheels) = async_channel::unbounded();
    let updater = FixtureUpdater {
        update: data.update.clone(),
        download_events,
    };
    let backend = FixtureBackend::new(data);

    std::thread::spawn(move || {
        for line in std::io::stdin().lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }

            match serde_json::from_str::<Command>(&line) {
                Ok(Command::Emit { event, payload }) => match parse_event(event, payload) {
                    Ok(event) => events.emit(event),
                    Err(error) => eprintln!("fixture: unknown event in {line}: {error}"),
                },
                Ok(Command::UpdateEvent { message }) => {
                    let _ = download_sender.send_blocking(message);
                }
                Ok(Command::Shortcut { shortcut, state }) => {
                    events.emit(AppEvent::GlobalShortcut { shortcut, state });
                }
                Ok(Command::Wheel(wheel)) => {
                    let _ = wheel_sender.send_blocking(wheel);
                }
                Err(error) => eprintln!("fixture: cannot parse {line}: {error}"),
            }
        }
    });

    (
        std::sync::Arc::new(backend),
        std::sync::Arc::new(updater),
        wheels,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn script_events_use_the_tauri_event_names() {
        assert_eq!(
            parse_event(
                "model-download-progress".into(),
                json!({ "modelId": "m", "progress": 42, "downloadedBytes": 10, "totalBytes": 20 })
            )
            .unwrap(),
            AppEvent::ModelDownloadProgress(ModelDownloadProgress {
                model_id: "m".into(),
                progress: 42,
                downloaded_bytes: 10,
                total_bytes: 20,
            })
        );
        assert_eq!(
            parse_event(
                "system-key-pressed".into(),
                json!({ "key": "leftControl", "pressedKeys": ["leftControl", "leftOption"] })
            )
            .unwrap(),
            AppEvent::SystemKeyPressed {
                key: SystemKey::LeftControl,
                pressed_keys: vec![SystemKey::LeftControl, SystemKey::LeftOption],
            }
        );
        assert_eq!(
            parse_event("transcription-created".into(), Value::Null).unwrap(),
            AppEvent::TranscriptionCreated
        );
        assert_eq!(
            parse_event("audio-level".into(), json!(0.5)).unwrap(),
            AppEvent::AudioLevel(0.5)
        );
        assert_eq!(
            parse_event("hotkey-recorder-active".into(), json!({ "active": true })).unwrap(),
            AppEvent::HotkeyRecorderActive { active: true }
        );
    }
}
