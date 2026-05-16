mod handlers;
mod listeners;
mod menu;
mod tray;
mod window;

use tauri::Manager;

use handlers::{
    add_dictionary_word, check_accessibility_probe, check_model_status, delete_app_instruction,
    delete_dictionary_word, download_whisper_model, get_dictionary_prompt, get_frontmost_app,
    list_app_instructions, list_audio_devices, list_dictionary_words, list_installed_apps,
    list_transcriptions, mute_media_for_recording, process_audio_file, read_audio_file,
    restore_media_after_recording, save_transcription, set_app_instruction,
    start_recording_with_device, stop_recording_with_device, transcribe_audio, type_text,
    update_dictionary_word,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            type_text,
            read_audio_file,
            process_audio_file,
            check_accessibility_probe,
            list_audio_devices,
            mute_media_for_recording,
            restore_media_after_recording,
            start_recording_with_device,
            stop_recording_with_device,
            save_transcription,
            list_transcriptions,
            add_dictionary_word,
            list_dictionary_words,
            update_dictionary_word,
            delete_dictionary_word,
            get_dictionary_prompt,
            check_model_status,
            download_whisper_model,
            transcribe_audio,
            list_installed_apps,
            get_frontmost_app,
            list_app_instructions,
            set_app_instruction,
            delete_app_instruction,
        ])
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_macos_permissions::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            {
                listeners::accessibility_watcher::setup(app.handle());
                listeners::system_key_watcher::setup(app.handle());
            }

            let db_state = handlers::db::init_db(app.handle())?;
            app.manage(db_state);

            #[cfg(desktop)]
            let _ = app
                .handle()
                .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
                    window::show_or_create_main_window(app);
                }));

            #[cfg(desktop)]
            menu::setup(app)?;

            #[cfg(desktop)]
            tray::setup(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            #[cfg(desktop)]
            if let tauri::RunEvent::Reopen { .. } = event {
                window::show_or_create_main_window(app);
            }
        });
}
