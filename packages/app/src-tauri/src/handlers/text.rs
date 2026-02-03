use enigo::{Enigo, Keyboard, Settings};
use std::fs;

#[tauri::command]
pub fn type_text(text: String) -> Result<(), String> {
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
    enigo.text(&text).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn read_audio_file(path: String) -> Result<Vec<u8>, String> {
    fs::read(&path).map_err(|e| format!("Failed to read audio file: {}", e))
}

#[cfg(target_os = "macos")]
#[tauri::command]
pub fn check_accessibility_probe() -> bool {
    use core_graphics::event::{
        CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    };

    let tap_result = CGEventTap::new(
        CGEventTapLocation::HID,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::ListenOnly,
        vec![CGEventType::KeyDown],
        |_proxy, _event_type, event| Some(event.clone()),
    );

    tap_result.is_ok()
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn check_accessibility_probe() -> bool {
    true
}
