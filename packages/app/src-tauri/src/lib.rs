use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use tauri_plugin_keychain::{Keychain, KeychainError};

#[tauri::command]
fn type_text(text: String) -> Result<(), String> {
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;

    enigo.text(&text).map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
async fn store_token(token: String) -> Result<(), String> {
    let keychain = Keychain::new();
    keychain.set_password("voxfusion", "auth_token", &token)
        .map_err(|e| format!("Failed to store token: {}", e))?;
    Ok(())
}

#[tauri::command]
async fn get_token() -> Result<Option<String>, String> {
    let keychain = Keychain::new();
    match keychain.get_password("voxfusion", "auth_token") {
        Ok(token) => Ok(Some(token)),
        Err(KeychainError::NoEntry) => Ok(None),
        Err(e) => Err(format!("Failed to retrieve token: {}", e)),
    }
}

#[tauri::command]
async fn delete_token() -> Result<(), String> {
    let keychain = Keychain::new();
    keychain.delete_password("voxfusion", "auth_token")
        .map_err(|e| format!("Failed to delete token: {}", e))?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![type_text, store_token, get_token, delete_token])
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_macos_permissions::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_keychain::init())
        .setup(|app| {
            // Register deep link scheme at runtime (needed for dev mode on macOS)
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                if let Err(e) = app.deep_link().register("voxfusion") {
                    eprintln!("Failed to register deep link: {}", e);
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
