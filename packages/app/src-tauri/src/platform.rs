use cpal::traits::{DeviceTrait, HostTrait};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    os: &'static str,
    wayland: bool,
    hyprland: bool,
    text_insertion_available: bool,
}

#[tauri::command]
pub fn platform_info() -> PlatformInfo {
    let wayland = cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some();
    PlatformInfo {
        os: std::env::consts::OS,
        wayland,
        hyprland: cfg!(target_os = "linux")
            && std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some(),
        text_insertion_available: !wayland || executable_available("wtype"),
    }
}

pub fn executable_available(name: &str) -> bool {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| {
            std::fs::metadata(dir.join(name)).is_ok_and(|meta| {
                #[cfg(unix)]
                {
                    meta.is_file() && meta.permissions().mode() & 0o111 != 0
                }
                #[cfg(not(unix))]
                {
                    meta.is_file()
                }
            })
        })
    })
}

#[tauri::command]
pub async fn check_microphone() -> Result<(), String> {
    tokio::task::spawn_blocking(|| {
        let device = cpal::default_host()
            .default_input_device()
            .ok_or("No microphone found. Check your PipeWire input device.")?;
        device
            .default_input_config()
            .map_err(|e| format!("Microphone unavailable: {e}"))?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn register_linux_shortcut(
    app: tauri::AppHandle,
    hotkey: String,
    on_release: bool,
) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    return crate::linux::register_shortcut(&app, &hotkey, on_release);
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app, hotkey, on_release);
        Err("Linux only".into())
    }
}

#[tauri::command]
pub fn unregister_linux_shortcut(app: tauri::AppHandle, hotkey: String) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    return crate::linux::unregister_shortcut(&app, &hotkey);
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app, hotkey);
        Ok(())
    }
}
