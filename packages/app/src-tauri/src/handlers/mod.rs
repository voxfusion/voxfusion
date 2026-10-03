pub mod apps;
pub mod audio;
pub mod browser;
pub mod db;
pub mod hotkeys;
pub mod media;
pub mod models;
pub mod parakeet;
pub mod sites;
pub mod text;
pub mod whisper;

pub use apps::{
    add_app_dictionary_word, delete_app_dictionary, delete_app_dictionary_word,
    delete_app_instruction, get_frontmost_app, list_app_dictionaries, list_app_instructions,
    list_installed_apps, set_app_instruction, update_app_dictionary_word,
};
pub use audio::{list_audio_devices, start_recording_with_device, stop_recording_with_device};
pub use db::{
    add_dictionary_word, delete_dictionary_word, list_dictionary_words, list_transcriptions,
    save_transcription, update_dictionary_word,
};
pub use hotkeys::start_system_key_watcher;
pub use media::{
    muffle_media_for_recording, mute_media_for_recording, request_muffle_permission,
    restore_media_after_recording,
};
pub use models::{cancel_model_download, download_model, list_models, set_active_model};
pub use sites::{
    add_site_dictionary_word, delete_site_dictionary, delete_site_dictionary_word,
    delete_site_style, list_site_dictionaries, list_site_styles, set_site_style,
    update_site_dictionary_word,
};
pub use text::{check_accessibility_probe, type_text};
pub use whisper::{check_model_status, download_whisper_model, transcribe_audio};
