use enigo::{Enigo, Keyboard, Settings};
use std::fs;

#[tauri::command]
pub fn type_text(text: String) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let mut child = Command::new("wtype")
            .arg("-")
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Wayland text insertion requires wtype: {e}"))?;
        // Pipe text, including leading hyphens and Unicode, without shell parsing.
        let write_result = child
            .stdin
            .take()
            .ok_or("wtype stdin unavailable")?
            .write_all(text.as_bytes());
        let output = child.wait_with_output().map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(format!(
                "wtype failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        return write_result.map_err(|e| e.to_string());
    }
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
    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }

    unsafe { AXIsProcessTrusted() }
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn check_accessibility_probe() -> bool {
    if cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some() {
        return crate::platform::executable_available("wtype");
    }
    true
}
