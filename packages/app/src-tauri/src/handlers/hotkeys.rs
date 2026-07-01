#[cfg(target_os = "macos")]
#[tauri::command]
pub fn start_system_key_watcher(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::listeners::system_key_watcher::setup(&app_handle);
    Ok(())
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn start_system_key_watcher(_app_handle: tauri::AppHandle) -> Result<(), String> {
    Ok(())
}
