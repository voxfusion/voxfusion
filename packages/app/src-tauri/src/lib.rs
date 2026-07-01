mod handlers;
mod listeners;
mod menu;
mod tray;
mod window;

use tauri::Manager;

use handlers::{
    add_app_dictionary_word, add_dictionary_word, add_site_dictionary_word,
    check_accessibility_probe, check_model_downloaded, check_model_status, delete_app_dictionary,
    delete_app_dictionary_word, delete_app_instruction, delete_dictionary_word,
    delete_site_dictionary, delete_site_dictionary_word, delete_site_style, download_model,
    download_whisper_model, get_active_model, get_dictionary_prompt, get_frontmost_app,
    list_app_dictionaries, list_app_instructions, list_audio_devices, list_dictionary_words,
    list_installed_apps, list_models, list_site_dictionaries, list_site_styles, list_transcriptions,
    mute_media_for_recording, process_audio_file, read_audio_file, restore_media_after_recording,
    save_transcription, set_active_model, set_app_instruction, set_site_style,
    start_recording_with_device, start_system_key_watcher, stop_recording_with_device,
    transcribe_audio, type_text, update_app_dictionary_word, update_dictionary_word,
    update_site_dictionary_word,
};

fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let payload = panic_info
            .payload()
            .downcast_ref::<&str>()
            .map(|value| (*value).to_string())
            .or_else(|| panic_info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic payload".to_string());

        let location = panic_info
            .location()
            .map(|location| {
                format!(
                    "{}:{}:{}",
                    location.file(),
                    location.line(),
                    location.column()
                )
            })
            .unwrap_or_else(|| "unknown".to_string());

        log::error!(target: "runtime", "rust_panic payload={payload:?} location={location}");

        default_hook(panic_info);
    }));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    install_panic_hook();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            type_text,
            read_audio_file,
            process_audio_file,
            check_accessibility_probe,
            start_system_key_watcher,
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
            list_models,
            get_active_model,
            set_active_model,
            download_model,
            check_model_downloaded,
            list_installed_apps,
            get_frontmost_app,
            list_app_instructions,
            set_app_instruction,
            delete_app_instruction,
            list_app_dictionaries,
            add_app_dictionary_word,
            update_app_dictionary_word,
            delete_app_dictionary_word,
            delete_app_dictionary,
            list_site_dictionaries,
            add_site_dictionary_word,
            update_site_dictionary_word,
            delete_site_dictionary_word,
            delete_site_dictionary,
            list_site_styles,
            set_site_style,
            delete_site_style,
        ])
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_macos_permissions::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .clear_targets()
                .level(log::LevelFilter::Info)
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("voxfusion".to_string()),
                    },
                ))
                .max_file_size(5_000_000)
                .build(),
        )
        .setup(|app| {
            log::info!(
                target: "runtime",
                "setup_started cargo_package_version={}",
                env!("CARGO_PKG_VERSION")
            );

            #[cfg(target_os = "macos")]
            {
                listeners::accessibility_watcher::setup(app.handle());
                log::info!(target: "runtime", "macos_listeners_setup");
            }

            let db_state = handlers::db::init_db(app.handle())?;
            app.manage(db_state);
            log::info!(target: "runtime", "database_initialized");

            let active_model = handlers::models::init_active_model(app.handle());
            app.manage(active_model);
            log::info!(target: "runtime", "active_model_initialized");

            #[cfg(desktop)]
            let _ = app
                .handle()
                .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
                    log::info!(target: "runtime", "single_instance_requested");
                    window::show_or_create_main_window(app);
                }));

            #[cfg(desktop)]
            window::create_voice_control_window(app)?;
            #[cfg(desktop)]
            log::info!(target: "runtime", "voice_control_window_created");

            #[cfg(desktop)]
            menu::setup(app)?;
            #[cfg(desktop)]
            log::info!(target: "runtime", "menu_setup");

            #[cfg(desktop)]
            tray::setup(app)?;
            #[cfg(desktop)]
            log::info!(target: "runtime", "tray_setup");

            log::info!(target: "runtime", "setup_completed");
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                log::info!(
                    target: "runtime",
                    "window_close_requested label={}",
                    window.label()
                );
                if window.label() == "main" {
                    api.prevent_close();
                    log::info!(target: "runtime", "main_window_close_prevented");
                    let _ = window.hide();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            #[cfg(desktop)]
            match event {
                tauri::RunEvent::Reopen { .. } => {
                    log::info!(target: "runtime", "reopen_requested");
                    window::show_or_create_main_window(app);
                }
                tauri::RunEvent::ExitRequested { code, .. } => {
                    log::warn!(target: "runtime", "exit_requested code={code:?}");
                }
                tauri::RunEvent::Exit => {
                    log::warn!(target: "runtime", "exit");
                }
                _ => {}
            }
        });
}
