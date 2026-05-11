mod handlers;
mod listeners;

use tauri::menu::{CheckMenuItem, Menu, MenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Listener, Manager};

use handlers::{
    add_dictionary_word, check_accessibility_probe, check_model_status,
    delete_dictionary_word, download_whisper_model, get_dictionary_prompt,
    list_audio_devices, list_dictionary_words, list_transcriptions,
    mute_media_for_recording, process_audio_file, read_audio_file,
    restore_media_after_recording, save_transcription, start_recording_with_device,
    stop_recording_with_device, transcribe_audio, type_text, update_dictionary_word,
};

#[cfg(desktop)]
fn show_or_create_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        use tauri::WebviewWindowBuilder;
        if let Ok(window) =
            WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("/".into()))
                .title("VoxFusion")
                .inner_size(1360.0, 850.0)
                .resizable(true)
                .decorations(true)
                .title_bar_style(tauri::TitleBarStyle::Overlay)
                .hidden_title(true)
                .fullscreen(false)
                .center()
                .build()
        {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

#[cfg(desktop)]
fn build_microphone_submenu(
    app: &tauri::AppHandle,
    selected_mic: Option<String>,
    devices: Vec<handlers::audio::AudioDevice>,
) -> Result<Submenu<tauri::Wry>, Box<dyn std::error::Error>> {
    let submenu = Submenu::with_id(app, "microphone", "Microphone", true)?;

    for device in devices {
        let label = if device.is_default {
            format!("{} (Default)", device.name)
        } else {
            device.name.clone()
        };
        let id = format!("mic_{}", device.name);
        let is_selected = selected_mic.as_ref().map_or(false, |s| s == &device.name);
        let item = CheckMenuItem::with_id(app, &id, &label, true, is_selected, None::<&str>)?;
        submenu.append(&item)?;
    }

    Ok(submenu)
}

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
                    show_or_create_main_window(app);
                }));

            #[cfg(desktop)]
            {
                use tauri_plugin_store::StoreExt;

                let handle = app.handle().clone();

                let selected_mic = app
                    .store("settings.json")
                    .ok()
                    .and_then(|store| store.get("selectedMicrophoneId"))
                    .and_then(|v| v.as_str().map(|s| s.to_string()));

                let mic_submenu = build_microphone_submenu(&handle, selected_mic, vec![])?;

                let home_item = MenuItem::with_id(app, "home", "Home", true, None::<&str>)?;
                let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

                let menu = Menu::with_items(app, &[&home_item, &mic_submenu, &quit_item])?;

                let mic_submenu_for_menu = mic_submenu.clone();
                let mic_submenu_for_listener = mic_submenu.clone();

                let tray_icon_bytes = include_bytes!("../icons/tray-icon.png");
                let tray_image = tauri::image::Image::from_bytes(tray_icon_bytes)
                    .expect("Failed to load tray icon");
                let _tray = TrayIconBuilder::new()
                    .icon(tray_image)
                    .icon_as_template(true)
                    .menu(&menu)
                    .show_menu_on_left_click(true)
                    .on_menu_event(move |app, event| {
                        let id = event.id().as_ref();
                        match id {
                            "home" => {
                                if let Some(window) = app.get_webview_window("main") {
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                    let _ = window.emit("navigate", "/");
                                } else {
                                    use tauri::WebviewWindowBuilder;
                                    if let Ok(window) = WebviewWindowBuilder::new(
                                        app,
                                        "main",
                                        tauri::WebviewUrl::App("/".into()),
                                    )
                                    .title("VoxFusion")
                                    .inner_size(1360.0, 850.0)
                                    .resizable(true)
                                    .decorations(true)
                                    .title_bar_style(tauri::TitleBarStyle::Overlay)
                                    .hidden_title(true)
                                    .fullscreen(false)
                                    .center()
                                    .build()
                                    {
                                        let _ = window.show();
                                        let _ = window.set_focus();
                                    }
                                }
                            }
                            "quit" => {
                                app.exit(0);
                            }
                            _ if id.starts_with("mic_") => {
                                let device_name = id.strip_prefix("mic_").unwrap_or("").to_string();

                                let devices =
                                    handlers::audio::list_audio_devices().unwrap_or_default();
                                for device in &devices {
                                    let mic_id = format!("mic_{}", device.name);
                                    if let Some(item) = mic_submenu_for_menu.get(&mic_id) {
                                        if let Some(check_item) = item.as_check_menuitem() {
                                            let _ =
                                                check_item.set_checked(device.name == device_name);
                                        }
                                    }
                                }

                                if let Some(window) = app.get_webview_window("main") {
                                    let _ = window.emit("select-microphone", &device_name);
                                }
                            }
                            _ => {}
                        }
                    })
                    .build(app)?;

                let app_handle = app.handle().clone();
                app.listen("settings-changed", move |_| {
                    use tauri_plugin_store::StoreExt;

                    let selected_mic = app_handle
                        .store("settings.json")
                        .ok()
                        .and_then(|store| store.get("selectedMicrophoneId"))
                        .and_then(|v| v.as_str().map(|s| s.to_string()));

                    let devices = handlers::audio::list_audio_devices().unwrap_or_default();

                    for device in &devices {
                        let mic_id = format!("mic_{}", device.name);
                        let is_selected =
                            selected_mic.as_ref().map_or(false, |s| s == &device.name);

                        if let Some(item) = mic_submenu_for_listener.get(&mic_id) {
                            if let Some(check_item) = item.as_check_menuitem() {
                                let _ = check_item.set_checked(is_selected);
                            }
                        } else {
                            let label = if device.is_default {
                                format!("{} (Default)", device.name)
                            } else {
                                device.name.clone()
                            };
                            if let Ok(item) = CheckMenuItem::with_id(
                                &app_handle,
                                &mic_id,
                                &label,
                                true,
                                is_selected,
                                None::<&str>,
                            ) {
                                let _ = mic_submenu_for_listener.append(&item);
                            }
                        }
                    }
                });
            }
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
                show_or_create_main_window(app);
            }
        });
}
