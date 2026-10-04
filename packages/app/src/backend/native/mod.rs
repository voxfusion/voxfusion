//! The services behind the interface on a real machine: recording,
//! transcription, models, the database, permissions and hotkeys.

mod apps;
mod audio;
mod browser;
mod db;
mod hotkeys;
mod media;
mod models;
mod parakeet;
pub mod permissions;
mod sites;
mod text;
mod whisper;

use std::path::PathBuf;
use std::sync::Arc;

use crate::backend::{
    AppDictionary, AppInstruction, AudioDevice, Backend, CommandResult, DictionaryWord,
    EventSender, FrontmostApp, InstalledApp, ModelInfo, PermissionState, SiteDictionary, SiteStyle,
    Transcription, TranscriptionPage, TranscriptionResult,
};

pub use hotkeys::resynchronize_system_keys;
pub use media::{restore_media_on_exit, run_permission_request_if_asked};

pub struct NativeBackend {
    events: EventSender,
    db: db::DbState,
    active_model: models::ActiveModel,
    shortcuts: hotkeys::GlobalShortcuts,
    /// Drives the model downloads, the only calls that are asynchronous
    /// inside.
    runtime: tokio::runtime::Runtime,
}

impl NativeBackend {
    /// Opens the database, loads the model selection and starts reporting
    /// audio device and Accessibility changes. One per process: the watchers
    /// and the global shortcuts report to the first one made.
    pub fn new(events: EventSender) -> Result<Arc<Self>, String> {
        let db = db::init_db()?;
        log::info!(target: "runtime", "database_initialized");

        let active_model = models::init_active_model();
        log::info!(target: "runtime", "active_model_initialized");

        let runtime = tokio::runtime::Builder::new_multi_thread()
            // A download runs on the thread that asked for it; the workers
            // only carry the HTTP client's connection tasks.
            .worker_threads(2)
            .thread_name("backend-runtime")
            .enable_all()
            .build()
            .map_err(|err| format!("Failed to start the backend runtime: {err}"))?;

        #[cfg(target_os = "macos")]
        {
            crate::platform::accessibility_watcher::setup(&events);
            media::watch_audio_devices(&events);
            log::info!(target: "runtime", "macos_listeners_setup");
        }

        let shortcuts = hotkeys::GlobalShortcuts::new(events.clone());

        Ok(Arc::new(Self {
            events,
            db,
            active_model,
            shortcuts,
            runtime,
        }))
    }
}

impl Backend for NativeBackend {
    fn type_text(&self, text: &str) -> CommandResult<()> {
        text::type_text(text)
    }

    fn check_accessibility(&self) -> bool {
        permissions::check_accessibility()
    }

    fn request_accessibility(&self) {
        permissions::request_accessibility();
    }

    fn microphone_permission(&self) -> PermissionState {
        permissions::microphone_permission()
    }

    fn request_microphone_permission(&self) -> bool {
        permissions::request_microphone_permission()
    }

    fn start_system_key_watcher(&self) -> CommandResult<()> {
        hotkeys::start_system_key_watcher(&self.events)
    }

    fn list_audio_devices(&self) -> CommandResult<Vec<AudioDevice>> {
        audio::list_audio_devices()
    }

    fn start_recording(&self, device_name: Option<String>) -> CommandResult<()> {
        audio::start_recording_with_device(&self.events, device_name)
    }

    fn stop_recording(&self) -> CommandResult<PathBuf> {
        audio::stop_recording_with_device()
    }

    fn mute_media_for_recording(&self) -> CommandResult<()> {
        media::mute_media_for_recording()
    }

    fn muffle_media_for_recording(&self) {
        media::muffle_media_for_recording();
    }

    fn restore_media_after_recording(&self) -> CommandResult<()> {
        media::restore_media_after_recording()
    }

    fn request_muffle_permission(&self) {
        media::request_muffle_permission();
    }

    fn transcribe_audio(
        &self,
        audio_path: PathBuf,
        bundle_id: Option<String>,
        domain: Option<String>,
        fallback_style: String,
    ) -> CommandResult<TranscriptionResult> {
        whisper::transcribe_audio(
            &self.db,
            &self.active_model,
            &audio_path,
            bundle_id.as_deref(),
            domain.as_deref(),
            &fallback_style,
        )
    }

    fn save_transcription(&self, result: &TranscriptionResult) -> CommandResult<Transcription> {
        db::save_transcription(&self.db, result)
    }

    fn list_transcriptions(
        &self,
        limit: i64,
        cursor: Option<String>,
    ) -> CommandResult<TranscriptionPage> {
        db::list_transcriptions(&self.db, limit, cursor)
    }

    fn check_model_status(&self) -> CommandResult<bool> {
        whisper::check_model_status(&self.active_model)
    }

    fn list_models(&self) -> CommandResult<Vec<ModelInfo>> {
        Ok(models::list_models(&self.active_model))
    }

    fn set_active_model(&self, model_id: &str) -> CommandResult<()> {
        models::set_active_model(&self.active_model, model_id)
    }

    fn download_model(&self, model_id: &str) -> CommandResult<()> {
        self.runtime
            .block_on(models::download_model(&self.events, model_id))
    }

    fn cancel_model_download(&self, model_id: &str) -> CommandResult<()> {
        models::cancel_model_download(model_id)
    }

    fn list_dictionary_words(&self) -> CommandResult<Vec<DictionaryWord>> {
        db::list_dictionary_words(&self.db)
    }

    fn add_dictionary_word(&self, word: &str) -> CommandResult<DictionaryWord> {
        db::add_dictionary_word(&self.db, word)
    }

    fn update_dictionary_word(&self, id: &str, word: &str) -> CommandResult<DictionaryWord> {
        db::update_dictionary_word(&self.db, id, word)
    }

    fn delete_dictionary_word(&self, id: &str) -> CommandResult<()> {
        db::delete_dictionary_word(&self.db, id)
    }

    fn list_app_dictionaries(&self) -> CommandResult<Vec<AppDictionary>> {
        apps::list_app_dictionaries(&self.db)
    }

    fn add_app_dictionary_word(
        &self,
        bundle_id: &str,
        app_name: &str,
        word: &str,
    ) -> CommandResult<DictionaryWord> {
        apps::add_app_dictionary_word(&self.db, bundle_id, app_name, word)
    }

    fn update_app_dictionary_word(&self, id: &str, word: &str) -> CommandResult<DictionaryWord> {
        apps::update_app_dictionary_word(&self.db, id, word)
    }

    fn delete_app_dictionary_word(&self, id: &str) -> CommandResult<()> {
        apps::delete_app_dictionary_word(&self.db, id)
    }

    fn delete_app_dictionary(&self, bundle_id: &str) -> CommandResult<()> {
        apps::delete_app_dictionary(&self.db, bundle_id)
    }

    fn list_site_dictionaries(&self) -> CommandResult<Vec<SiteDictionary>> {
        sites::list_site_dictionaries(&self.db)
    }

    fn add_site_dictionary_word(&self, domain: &str, word: &str) -> CommandResult<DictionaryWord> {
        sites::add_site_dictionary_word(&self.db, domain, word)
    }

    fn update_site_dictionary_word(&self, id: &str, word: &str) -> CommandResult<DictionaryWord> {
        sites::update_site_dictionary_word(&self.db, id, word)
    }

    fn delete_site_dictionary_word(&self, id: &str) -> CommandResult<()> {
        sites::delete_site_dictionary_word(&self.db, id)
    }

    fn delete_site_dictionary(&self, domain: &str) -> CommandResult<()> {
        sites::delete_site_dictionary(&self.db, domain)
    }

    fn list_installed_apps(&self) -> CommandResult<Vec<InstalledApp>> {
        Ok(apps::list_installed_apps())
    }

    fn get_frontmost_app(&self) -> CommandResult<Option<FrontmostApp>> {
        Ok(apps::get_frontmost_app())
    }

    fn list_app_instructions(&self) -> CommandResult<Vec<AppInstruction>> {
        apps::list_app_instructions(&self.db)
    }

    fn set_app_instruction(
        &self,
        bundle_id: &str,
        app_name: &str,
        style: &str,
    ) -> CommandResult<AppInstruction> {
        apps::set_app_instruction(&self.db, bundle_id, app_name, style)
    }

    fn delete_app_instruction(&self, id: &str) -> CommandResult<()> {
        apps::delete_app_instruction(&self.db, id)
    }

    fn list_site_styles(&self) -> CommandResult<Vec<SiteStyle>> {
        sites::list_site_styles(&self.db)
    }

    fn set_site_style(&self, domain: &str, style: &str) -> CommandResult<SiteStyle> {
        sites::set_site_style(&self.db, domain, style)
    }

    fn delete_site_style(&self, id: &str) -> CommandResult<()> {
        sites::delete_site_style(&self.db, id)
    }

    fn register_shortcut(&self, shortcut: &str) -> CommandResult<()> {
        self.shortcuts.register(shortcut)
    }

    fn unregister_shortcut(&self, shortcut: &str) -> CommandResult<()> {
        self.shortcuts.unregister(shortcut)
    }
}
