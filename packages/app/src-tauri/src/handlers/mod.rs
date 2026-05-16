pub mod apps;
pub mod audio;
pub mod audio_processing;
pub mod db;
pub mod media;
pub mod text;
pub mod whisper;

pub use apps::{
    add_app_dictionary_word, delete_app_dictionary, delete_app_dictionary_word,
    delete_app_instruction, get_frontmost_app, list_app_dictionaries, list_app_instructions,
    list_installed_apps, set_app_instruction, update_app_dictionary_word,
};
pub use audio::{list_audio_devices, start_recording_with_device, stop_recording_with_device};
pub use audio_processing::process_audio_file;
pub use db::{
    add_dictionary_word, delete_dictionary_word, get_dictionary_prompt, list_dictionary_words,
    list_transcriptions, save_transcription, update_dictionary_word,
};
pub use media::{mute_media_for_recording, restore_media_after_recording};
pub use text::{check_accessibility_probe, read_audio_file, type_text};
pub use whisper::{check_model_status, download_whisper_model, transcribe_audio};
