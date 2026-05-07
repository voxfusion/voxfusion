use std::process::Command;
use std::sync::{LazyLock, Mutex};

#[derive(Default)]
struct MediaMuteState {
    active: bool,
    was_muted: bool,
}

static MEDIA_MUTE_STATE: LazyLock<Mutex<MediaMuteState>> =
    LazyLock::new(|| Mutex::new(MediaMuteState::default()));

#[cfg(target_os = "macos")]
fn run_osascript(script: &str) -> Result<String, String> {
    let output = Command::new("osascript")
        .args(["-e", script])
        .output()
        .map_err(|err| format!("Failed to run osascript: {}", err))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(target_os = "macos")]
fn get_output_muted() -> Result<bool, String> {
    Ok(run_osascript("output muted of (get volume settings)")? == "true")
}

#[cfg(target_os = "macos")]
fn set_output_muted(muted: bool) -> Result<(), String> {
    run_osascript(if muted {
        "set volume output muted true"
    } else {
        "set volume output muted false"
    })
    .map(|_| ())
}

#[tauri::command]
pub fn mute_media_for_recording() -> Result<(), String> {
    let mut state = MEDIA_MUTE_STATE.lock().map_err(|err| err.to_string())?;
    if state.active {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        state.was_muted = get_output_muted()?;
        if !state.was_muted {
            set_output_muted(true)?;
        }
        state.active = true;
    }

    #[cfg(not(target_os = "macos"))]
    {
        state.active = true;
        state.was_muted = false;
    }

    Ok(())
}

#[tauri::command]
pub fn restore_media_after_recording() -> Result<(), String> {
    let mut state = MEDIA_MUTE_STATE.lock().map_err(|err| err.to_string())?;
    if !state.active {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        if !state.was_muted {
            set_output_muted(false)?;
        }
    }

    state.active = false;
    state.was_muted = false;
    Ok(())
}
