//! The interface between the windows and the services behind them: the data
//! they exchange, the events the services raise, and the [`Backend`] trait
//! that both the native services and the fixture stand-in implement.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub type CommandResult<T> = Result<T, String>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transcription {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub word_count: i64,
    pub processing_time_ms: i64,
    #[serde(default)]
    pub audio_duration_ms: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TranscriptionPage {
    pub transcriptions: Vec<Transcription>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResult {
    pub text: String,
    pub word_count: i64,
    pub processing_time_ms: i64,
    pub audio_duration_ms: Option<i64>,
}

/// A custom term. The default, per-app and per-site dictionaries share it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DictionaryWord {
    pub id: String,
    pub word: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

/// Normalizes a site input or URL to a canonical lowercase domain without
/// scheme, credentials, port, path, or leading `www.`.
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

/// The most characters a dictionary word may have. The services refuse a
/// longer one, and the forms say so before it is sent.
pub const MAX_WORD_CHARS: usize = 100;

/// Why a dictionary term is refused before storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordValidationError {
    Empty,
    TooLong { count: usize },
    ControlCharacters,
}

impl fmt::Display for WordValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WordValidationError::Empty | WordValidationError::TooLong { .. } => {
                write!(f, "Word must be between 1 and {MAX_WORD_CHARS} characters")
            }
            WordValidationError::ControlCharacters => {
                write!(f, "Word cannot contain control characters")
            }
        }
    }
}

/// Validates and trims a single dictionary term.
pub fn validate_dictionary_word(input: &str) -> Result<&str, WordValidationError> {
    let word = input.trim();
    if word.is_empty() {
        return Err(WordValidationError::Empty);
    }
    let count = word.chars().count();
    if count > MAX_WORD_CHARS {
        return Err(WordValidationError::TooLong { count });
    }
    if word.chars().any(char::is_control) {
        return Err(WordValidationError::ControlCharacters);
    }
    Ok(word)
}

/// Canonical case-insensitive key for duplicate detection across dictionaries.
pub fn dictionary_word_key(word: &str) -> String {
    word.trim().to_lowercase()
}

/// Validates every term in `words`, then returns the trimmed terms that are
/// neither already in `existing_words` nor repeated earlier in `words`.
pub fn prepare_dictionary_word_batch(
    words: &[String],
    existing_words: impl IntoIterator<Item = String>,
) -> Result<Vec<&str>, String> {
    let mut validated = Vec::with_capacity(words.len());
    for raw in words {
        validated.push(validate_dictionary_word(raw).map_err(|err| err.to_string())?);
    }

    let mut seen: HashSet<String> = existing_words
        .into_iter()
        .map(|word| dictionary_word_key(&word))
        .filter(|key| !key.is_empty())
        .collect();

    let mut to_insert = Vec::with_capacity(validated.len());
    for word in validated {
        if seen.insert(dictionary_word_key(word)) {
            to_insert.push(word);
        }
    }

    Ok(to_insert)
}

/// Why a line in a bulk-entry list cannot be saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulkInvalidReason {
    TooLong { count: usize },
    ControlCharacters,
}

/// Classification of one non-blank line in a bulk-entry list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulkWordStatus {
    /// Valid new term that will be added.
    Valid,
    /// Matches a term already stored in the target dictionary.
    DuplicateExisting,
    /// Repeats an earlier term in the same input.
    DuplicateInInput,
    /// Fails dictionary validation.
    Invalid(BulkInvalidReason),
}

/// One non-blank line parsed from bulk dictionary input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BulkWordEntry {
    /// 1-based line number in the input text.
    pub line: usize,
    /// Trimmed term text.
    pub term: String,
    pub status: BulkWordStatus,
}

/// Result of parsing a multiline bulk-entry list against existing dictionary
/// terms.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BulkWordPreview {
    pub entries: Vec<BulkWordEntry>,
    pub valid_terms: Vec<String>,
    pub duplicate_existing_count: usize,
    pub duplicate_input_count: usize,
    pub invalid_count: usize,
}

/// Splits `input` into lines on `\r\n`, `\n`, or standalone `\r`, preserving
/// exact 1-based line indices and stripping a leading UTF-8 BOM.
fn split_bulk_lines(input: &str) -> Vec<&str> {
    let input = input.strip_prefix('\u{FEFF}').unwrap_or(input);
    let bytes = input.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0;
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                lines.push(&input[start..i]);
                i += 1;
                start = i;
            }
            b'\r' => {
                lines.push(&input[start..i]);
                i += 1;
                if i < bytes.len() && bytes[i] == b'\n' {
                    i += 1;
                }
                start = i;
            }
            _ => i += 1,
        }
    }

    if start < bytes.len() {
        lines.push(&input[start..]);
    }

    lines
}

/// Parses one-term-per-line bulk input against `existing_words`, reporting
/// valid new terms, existing duplicates, in-input duplicates, and invalid
/// entries before saving.
pub fn parse_bulk_dictionary_words<'a>(
    input: &str,
    existing_words: impl IntoIterator<Item = &'a str>,
) -> BulkWordPreview {
    let existing: HashSet<String> = existing_words
        .into_iter()
        .map(dictionary_word_key)
        .filter(|key| !key.is_empty())
        .collect();
    let mut seen_in_input: HashSet<String> = HashSet::new();
    let mut preview = BulkWordPreview::default();

    for (index, raw_line) in split_bulk_lines(input).into_iter().enumerate() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let status = match validate_dictionary_word(trimmed) {
            Err(WordValidationError::TooLong { count }) => {
                preview.invalid_count += 1;
                BulkWordStatus::Invalid(BulkInvalidReason::TooLong { count })
            }
            Err(WordValidationError::ControlCharacters) => {
                preview.invalid_count += 1;
                BulkWordStatus::Invalid(BulkInvalidReason::ControlCharacters)
            }
            Err(WordValidationError::Empty) => continue,
            Ok(valid) => {
                let key = dictionary_word_key(valid);
                if existing.contains(&key) {
                    preview.duplicate_existing_count += 1;
                    BulkWordStatus::DuplicateExisting
                } else if !seen_in_input.insert(key) {
                    preview.duplicate_input_count += 1;
                    BulkWordStatus::DuplicateInInput
                } else {
                    preview.valid_terms.push(valid.to_string());
                    BulkWordStatus::Valid
                }
            }
        };

        preview.entries.push(BulkWordEntry {
            line: index + 1,
            term: trimmed.to_string(),
            status,
        });
    }

    preview
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppDictionary {
    pub bundle_id: String,
    pub app_name: String,
    pub words: Vec<DictionaryWord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiteDictionary {
    pub domain: String,
    pub words: Vec<DictionaryWord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppInstruction {
    pub id: String,
    pub bundle_id: String,
    pub app_name: String,
    pub style: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiteStyle {
    pub id: String,
    pub domain: String,
    pub style: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstalledApp {
    pub name: String,
    pub bundle_id: String,
    pub path: String,
    /// A `data:image/png;base64,` URL of the app's icon.
    pub icon_data_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontmostApp {
    pub name: String,
    pub bundle_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AudioDevice {
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub engine: String,
    pub size_label: String,
    pub languages: String,
    pub experimental: bool,
    pub recommended: bool,
    /// Whether this build has the engine that runs the model. Release builds
    /// bundle Parakeet's; a development build may not have it.
    #[serde(default = "engine_present")]
    pub available: bool,
    pub downloaded: bool,
    pub active: bool,
}

fn engine_present() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDownloadProgress {
    pub model_id: String,
    /// Whole percentage, 0-100.
    pub progress: u32,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
}

pub const DEFAULT_MODEL_ID: &str = "whisper-large-v3-turbo";
pub const DOWNLOAD_IN_PROGRESS_ERROR: &str = "download already in progress";
pub const DOWNLOAD_CANCELLED_ERROR: &str = "Download cancelled";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SystemKey {
    Fn,
    LeftControl,
    RightControl,
    LeftOption,
    RightOption,
    LeftShift,
    RightShift,
    LeftCommand,
    RightCommand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShortcutState {
    Pressed,
    Released,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionState {
    Granted,
    Denied,
    /// macOS has not asked the user yet.
    Prompt,
}

/// Everything the services and windows tell each other. The serialized names
/// are the event names of the Tauri app this was ported from, which the
/// fixture scripts still use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", content = "payload", rename_all = "kebab-case")]
pub enum AppEvent {
    ModelDownloadProgress(ModelDownloadProgress),
    /// A model's file has every byte and is being checked against its
    /// checksum. A `ModelDownloadProgress` at 100 follows if it passes.
    #[serde(rename_all = "camelCase")]
    ModelVerifying {
        model_id: String,
    },
    /// Another model was made the one that transcribes.
    ActiveModelChanged,
    AudioLevel(f32),
    RecordingError {
        message: String,
    },
    AudioDevicesChanged,
    AccessibilityChanged,
    #[serde(rename_all = "camelCase")]
    SystemKeyPressed {
        key: SystemKey,
        pressed_keys: Vec<SystemKey>,
    },
    #[serde(rename_all = "camelCase")]
    SystemKeyReleased {
        key: SystemKey,
        pressed_keys: Vec<SystemKey>,
    },
    SystemKeysReleased,
    #[serde(rename_all = "camelCase")]
    KeyboardKeyPressed {
        key_code: i64,
    },
    TranscriptionCreated,
    /// Every transcription was deleted.
    TranscriptionsCleared,
    Navigate(String),
    SelectMicrophone(String),
    CheckForUpdates,
    AccessibilityPermissionNeeded,
    LearningStepActive(bool),
    HotkeyRecorderActive {
        active: bool,
    },
    /// A registered global shortcut changed state.
    GlobalShortcut {
        shortcut: String,
        state: ShortcutState,
    },
    /// The app was launched again, or the tray asked for the main window.
    ShowMainWindow,
}

/// Where services post [`AppEvent`]s from any thread.
#[derive(Clone)]
pub struct EventSender(pub async_channel::Sender<AppEvent>);

impl EventSender {
    pub fn emit(&self, event: AppEvent) {
        // The receiver lives as long as the app; a closed channel means exit.
        let _ = self.0.try_send(event);
    }
}

/// The services behind the interface. Every method blocks, so the interface
/// calls them off the main thread (see `ui::call`).
pub trait Backend: Send + Sync + 'static {
    // ---- permissions and text insertion ----
    fn type_text(&self, text: &str) -> CommandResult<()>;
    fn check_accessibility(&self) -> bool;
    /// Brings up the system prompt that leads to the Accessibility settings.
    fn request_accessibility(&self);
    fn microphone_permission(&self) -> PermissionState;
    /// Asks for microphone access and waits for the answer.
    fn request_microphone_permission(&self) -> bool;
    fn start_system_key_watcher(&self) -> CommandResult<()>;

    // ---- recording ----
    fn list_audio_devices(&self) -> CommandResult<Vec<AudioDevice>>;
    fn start_recording(&self, device_name: Option<String>) -> CommandResult<()>;
    fn stop_recording(&self) -> CommandResult<PathBuf>;
    fn mute_media_for_recording(&self) -> CommandResult<()>;
    fn muffle_media_for_recording(&self);
    fn restore_media_after_recording(&self) -> CommandResult<()>;
    fn request_muffle_permission(&self);

    // ---- transcription ----
    fn transcribe_audio(
        &self,
        audio_path: PathBuf,
        bundle_id: Option<String>,
        domain: Option<String>,
        fallback_style: String,
    ) -> CommandResult<TranscriptionResult>;
    /// Saves `result` to the history along with the recording at
    /// `audio_path` it was transcribed from, which goes when it is deleted.
    fn save_transcription(
        &self,
        result: &TranscriptionResult,
        audio_path: &Path,
    ) -> CommandResult<Transcription>;
    fn list_transcriptions(
        &self,
        limit: i64,
        cursor: Option<String>,
    ) -> CommandResult<TranscriptionPage>;
    /// Deletes a transcription and its recording.
    fn delete_transcription(&self, id: &str) -> CommandResult<()>;
    /// Deletes every transcription and every recording kept on disk.
    fn clear_transcriptions(&self) -> CommandResult<()>;

    // ---- models ----
    fn check_model_status(&self) -> CommandResult<bool>;
    fn list_models(&self) -> CommandResult<Vec<ModelInfo>>;
    fn set_active_model(&self, model_id: &str) -> CommandResult<()>;
    /// Returns when the download has finished, failed or been cancelled.
    fn download_model(&self, model_id: &str) -> CommandResult<()>;
    fn cancel_model_download(&self, model_id: &str) -> CommandResult<()>;

    // ---- dictionary ----
    fn list_dictionary_words(&self) -> CommandResult<Vec<DictionaryWord>>;
    fn add_dictionary_word(&self, word: &str) -> CommandResult<DictionaryWord>;
    fn add_dictionary_words(&self, words: &[String]) -> CommandResult<Vec<DictionaryWord>>;
    fn update_dictionary_word(&self, id: &str, word: &str) -> CommandResult<DictionaryWord>;
    fn delete_dictionary_word(&self, id: &str) -> CommandResult<()>;

    fn list_app_dictionaries(&self) -> CommandResult<Vec<AppDictionary>>;
    fn add_app_dictionary_word(
        &self,
        bundle_id: &str,
        app_name: &str,
        word: &str,
    ) -> CommandResult<DictionaryWord>;
    fn add_app_dictionary_words(
        &self,
        bundle_id: &str,
        app_name: &str,
        words: &[String],
    ) -> CommandResult<Vec<DictionaryWord>>;
    fn update_app_dictionary_word(&self, id: &str, word: &str) -> CommandResult<DictionaryWord>;
    fn delete_app_dictionary_word(&self, id: &str) -> CommandResult<()>;
    fn delete_app_dictionary(&self, bundle_id: &str) -> CommandResult<()>;

    fn list_site_dictionaries(&self) -> CommandResult<Vec<SiteDictionary>>;
    fn add_site_dictionary_word(&self, domain: &str, word: &str) -> CommandResult<DictionaryWord>;
    fn add_site_dictionary_words(
        &self,
        domain: &str,
        words: &[String],
    ) -> CommandResult<Vec<DictionaryWord>>;
    fn update_site_dictionary_word(&self, id: &str, word: &str) -> CommandResult<DictionaryWord>;
    fn delete_site_dictionary_word(&self, id: &str) -> CommandResult<()>;
    fn delete_site_dictionary(&self, domain: &str) -> CommandResult<()>;

    // ---- styles ----
    fn list_installed_apps(&self) -> CommandResult<Vec<InstalledApp>>;
    fn get_frontmost_app(&self) -> CommandResult<Option<FrontmostApp>>;
    fn list_app_instructions(&self) -> CommandResult<Vec<AppInstruction>>;
    fn set_app_instruction(
        &self,
        bundle_id: &str,
        app_name: &str,
        style: &str,
    ) -> CommandResult<AppInstruction>;
    fn delete_app_instruction(&self, id: &str) -> CommandResult<()>;
    fn list_site_styles(&self) -> CommandResult<Vec<SiteStyle>>;
    fn set_site_style(&self, domain: &str, style: &str) -> CommandResult<SiteStyle>;
    fn delete_site_style(&self, id: &str) -> CommandResult<()>;

    // ---- global shortcuts ----
    /// Registers a key combination such as `Command+Shift+K`. Its presses and
    /// releases arrive as [`AppEvent::GlobalShortcut`].
    fn register_shortcut(&self, shortcut: &str) -> CommandResult<()>;
    fn unregister_shortcut(&self, shortcut: &str) -> CommandResult<()>;
}

pub type SharedBackend = Arc<dyn Backend>;

/// An update that can be downloaded and installed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub version: String,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", content = "data")]
pub enum UpdateDownloadEvent {
    #[serde(rename_all = "camelCase")]
    Started {
        content_length: Option<u64>,
    },
    #[serde(rename_all = "camelCase")]
    Progress {
        chunk_length: u64,
    },
    Finished,
}

pub trait Updater: Send + Sync + 'static {
    fn check(&self) -> CommandResult<Option<UpdateInfo>>;
    /// Downloads and installs `update`, reporting progress, then returns. The
    /// caller relaunches the app.
    fn download_and_install(
        &self,
        update: &UpdateInfo,
        on_event: &mut dyn FnMut(UpdateDownloadEvent),
    ) -> CommandResult<()>;
    fn relaunch(&self);
}

pub type SharedUpdater = Arc<dyn Updater>;
