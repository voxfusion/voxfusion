#[cfg(desktop)]
use crate::handlers;
#[cfg(desktop)]
use crate::window::show_or_create_main_window;

#[cfg(desktop)]
use tauri::menu::{CheckMenuItem, Menu, MenuItem, Submenu};
#[cfg(desktop)]
use tauri::tray::TrayIconBuilder;
#[cfg(desktop)]
use tauri::{Emitter, Listener, Manager};

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

#[cfg(desktop)]
pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
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
    let tray_image = tauri::image::Image::from_bytes(tray_icon_bytes)?;
    let _tray = TrayIconBuilder::new()
        .icon(tray_image)
        .icon_as_template(true)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| {
            let id = event.id().as_ref();
            match id {
                "home" => {
                    show_or_create_main_window(app);
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.emit("navigate", "/");
                    }
                }
                "quit" => {
                    app.exit(0);
                }
                _ if id.starts_with("mic_") => {
                    let device_name = id.strip_prefix("mic_").unwrap_or("").to_string();

                    let devices = handlers::audio::list_audio_devices().unwrap_or_default();
                    for device in &devices {
                        let mic_id = format!("mic_{}", device.name);
                        if let Some(item) = mic_submenu_for_menu.get(&mic_id) {
                            if let Some(check_item) = item.as_check_menuitem() {
                                let _ = check_item.set_checked(device.name == device_name);
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
        let selected_mic = app_handle
            .store("settings.json")
            .ok()
            .and_then(|store| store.get("selectedMicrophoneId"))
            .and_then(|v| v.as_str().map(|s| s.to_string()));

        let devices = handlers::audio::list_audio_devices().unwrap_or_default();

        for device in &devices {
            let mic_id = format!("mic_{}", device.name);
            let is_selected = selected_mic.as_ref().map_or(false, |s| s == &device.name);

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

    Ok(())
}
