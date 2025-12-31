use enigo::{Direction, Enigo, Key, Keyboard, Settings};

#[tauri::command]
fn trigger_paste() -> Result<(), String> {
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;

    // Small delay to ensure clipboard is ready
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Simulate Cmd+V on macOS, Ctrl+V on other platforms
    #[cfg(target_os = "macos")]
    {
        enigo
            .key(Key::Meta, Direction::Press)
            .map_err(|e| e.to_string())?;
        std::thread::sleep(std::time::Duration::from_millis(20));
        enigo
            .key(Key::Unicode('v'), Direction::Press)
            .map_err(|e| e.to_string())?;
        std::thread::sleep(std::time::Duration::from_millis(20));
        enigo
            .key(Key::Unicode('v'), Direction::Release)
            .map_err(|e| e.to_string())?;
        std::thread::sleep(std::time::Duration::from_millis(20));
        enigo
            .key(Key::Meta, Direction::Release)
            .map_err(|e| e.to_string())?;
    }

    #[cfg(not(target_os = "macos"))]
    {
        enigo
            .key(Key::Control, Direction::Press)
            .map_err(|e| e.to_string())?;
        std::thread::sleep(std::time::Duration::from_millis(20));
        enigo
            .key(Key::Unicode('v'), Direction::Press)
            .map_err(|e| e.to_string())?;
        std::thread::sleep(std::time::Duration::from_millis(20));
        enigo
            .key(Key::Unicode('v'), Direction::Release)
            .map_err(|e| e.to_string())?;
        std::thread::sleep(std::time::Duration::from_millis(20));
        enigo
            .key(Key::Control, Direction::Release)
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![trigger_paste])
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_macos_permissions::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            // Register deep link scheme at runtime (needed for dev mode on macOS)
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                if let Err(e) = app.deep_link().register("voxfusion") {
                    eprintln!("Failed to register deep link: {}", e);
                }
            }

            // Position voice-control window at bottom center of screen
            use tauri::Manager;
            if let Some(voice_window) = app.get_webview_window("voice-control") {
                if let Some(monitor) = voice_window.current_monitor().ok().flatten() {
                    let screen_size = monitor.size();
                    let window_size = voice_window.outer_size().unwrap_or_default();

                    // Calculate bottom center position with 40px margin from bottom
                    let x = (screen_size.width as i32 - window_size.width as i32) / 2;
                    let y = screen_size.height as i32 - window_size.height as i32 - 40;

                    let _ = voice_window.set_position(tauri::PhysicalPosition::new(x, y));
                    let _ = voice_window.show();
                }
            }

            Ok(())
        });

    #[cfg(desktop)]
    {
        use tauri::Emitter;
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            println!("New instance opened with {argv:?}");
            // Handle deep link from argv (when app is already running)
            if let Some(url) = argv.iter().find(|arg| arg.starts_with("voxfusion://")) {
                let _ = app.emit("deep-link", url);
            }
        }));
    }

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
