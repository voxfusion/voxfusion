mod handlers;

use argon2::{Algorithm, Argon2, Params, Version};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Listener, Manager};

use handlers::{
    check_accessibility_probe, list_audio_devices, read_audio_file,
    start_recording_with_device, stop_recording_with_device, type_text,
};

#[cfg(target_os = "macos")]
mod accessibility_watcher {
    use std::os::raw::c_void;
    use std::sync::OnceLock;

    use core_foundation::base::TCFType;
    use core_foundation::string::CFString;
    use core_foundation_sys::dictionary::CFDictionaryRef;
    use core_foundation_sys::notification_center::{
        CFNotificationCenterAddObserver, CFNotificationCenterGetDistributedCenter,
        CFNotificationCenterRef, CFNotificationName,
        CFNotificationSuspensionBehaviorDeliverImmediately,
    };
    use tauri::Emitter;

    static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

    extern "C" fn on_accessibility_changed(
        _center: CFNotificationCenterRef,
        _observer: *mut c_void,
        _name: CFNotificationName,
        _object: *const c_void,
        _user_info: CFDictionaryRef,
    ) {
        if let Some(handle) = APP_HANDLE.get() {
            let _ = handle.emit("accessibility-changed", ());
        }
    }

    pub fn setup(app_handle: &tauri::AppHandle) {
        APP_HANDLE.set(app_handle.clone()).ok();

        unsafe {
            let center = CFNotificationCenterGetDistributedCenter();
            let name = CFString::new("com.apple.accessibility.api");
            CFNotificationCenterAddObserver(
                center,
                std::ptr::null(),
                on_accessibility_changed,
                name.as_concrete_TypeRef(),
                std::ptr::null(),
                CFNotificationSuspensionBehaviorDeliverImmediately,
            );
            // The observer retains the name, but we must keep it alive
            std::mem::forget(name);
        }
    }
}

#[cfg(desktop)]
fn build_microphone_submenu(app: &tauri::AppHandle, selected_mic: Option<String>, devices: Vec<handlers::audio::AudioDevice>) -> Result<Submenu<tauri::Wry>, Box<dyn std::error::Error>> {
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
        .plugin(
            tauri_plugin_stronghold::Builder::new(|password| {
                let salt = b"voxfusion-stronghold-salt";

                let params =
                    Params::new(10_000, 10, 4, Some(32)).expect("failed to create argon2 params");

                let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

                let mut key = vec![0u8; 32];
                argon2
                    .hash_password_into(password.as_ref(), salt, &mut key)
                    .expect("failed to hash password");

                key
            })
            .build(),
        )
        .invoke_handler(tauri::generate_handler![
            type_text,
            read_audio_file,
            check_accessibility_probe,
            list_audio_devices,
            start_recording_with_device,
            stop_recording_with_device
        ])
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_macos_permissions::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_keychain::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                if let Err(e) = app.deep_link().register("voxfusion") {
                    eprintln!("Failed to register deep link: {}", e);
                }
            }

            #[cfg(target_os = "macos")]
            accessibility_watcher::setup(app.handle());

            Ok(())
        })
        .setup(|app| {
            #[cfg(desktop)]
            let _ = app
                .handle()
                .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
                    let _ = app
                        .get_webview_window("main")
                        .expect("no main window")
                        .set_focus();
                }));

            Ok(())
        })
        .setup(|app| {
            #[cfg(desktop)]
            {
                use tauri_plugin_store::StoreExt;

                let handle = app.handle().clone();

                // Read selected microphone from store
                let selected_mic = app
                    .store("settings.json")
                    .ok()
                    .and_then(|store| store.get("selectedMicrophoneId"))
                    .and_then(|v| v.as_str().map(|s| s.to_string()));

                // Build microphone submenu with empty devices initially
                // Devices will be populated when the user grants microphone permission
                // This avoids triggering a microphone permission request on app startup
                let mic_submenu = build_microphone_submenu(&handle, selected_mic, vec![])?;

                // Create menu items (no shortcuts to avoid conflicts with global shortcuts)
                let home_item = MenuItem::with_id(app, "home", "Home", true, None::<&str>)?;
                let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

                // Create menu
                let menu = Menu::with_items(app, &[&home_item, &mic_submenu, &quit_item])?;

                // Clone submenu for use in closures
                let mic_submenu_for_menu = mic_submenu.clone();
                let mic_submenu_for_listener = mic_submenu.clone();

                // Build tray icon with dedicated tray icon
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
                                    // Emit event to navigate to home
                                    let _ = window.emit("navigate", "/");
                                } else {
                                    // Window was closed, recreate it with same settings as tauri.conf.json
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

                                // Update checkmarks in submenu
                                let devices = handlers::audio::list_audio_devices().unwrap_or_default();
                                for device in &devices {
                                    let mic_id = format!("mic_{}", device.name);
                                    if let Some(item) = mic_submenu_for_menu.get(&mic_id) {
                                        if let Some(check_item) = item.as_check_menuitem() {
                                            let _ = check_item.set_checked(device.name == device_name);
                                        }
                                    }
                                }

                                // Emit event to frontend to save microphone selection
                                if let Some(window) = app.get_webview_window("main") {
                                    let _ = window.emit("select-microphone", &device_name);
                                }
                            }
                            _ => {}
                        }
                    })
                    .build(app)?;

                // Listen for settings changes from frontend to update tray menu
                let app_handle = app.handle().clone();
                app.listen("settings-changed", move |_| {
                    use tauri_plugin_store::StoreExt;

                    // Read the new selected microphone from store
                    let selected_mic = app_handle
                        .store("settings.json")
                        .ok()
                        .and_then(|store| store.get("selectedMicrophoneId"))
                        .and_then(|v| v.as_str().map(|s| s.to_string()));

                    // Fetch devices and update menu
                    // This is safe to call here as it only happens after user interaction
                    let devices = handlers::audio::list_audio_devices().unwrap_or_default();
                    
                    for device in &devices {
                        let mic_id = format!("mic_{}", device.name);
                        let is_selected = selected_mic.as_ref().map_or(false, |s| s == &device.name);
                        
                        // Try to update existing item, or add new one
                        if let Some(item) = mic_submenu_for_listener.get(&mic_id) {
                            if let Some(check_item) = item.as_check_menuitem() {
                                let _ = check_item.set_checked(is_selected);
                            }
                        } else {
                            // Add new item if it doesn't exist
                            let label = if device.is_default {
                                format!("{} (Default)", device.name)
                            } else {
                                device.name.clone()
                            };
                            if let Ok(item) = CheckMenuItem::with_id(&app_handle, &mic_id, &label, true, is_selected, None::<&str>) {
                                let _ = mic_submenu_for_listener.append(&item);
                            }
                        }
                    }
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
