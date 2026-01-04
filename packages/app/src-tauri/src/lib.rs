use enigo::{Enigo, Keyboard, Settings};

#[tauri::command]
fn type_text(text: String) -> Result<(), String> {
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;

    enigo.text(&text).map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
async fn store_token(token: String) -> Result<(), String> {
    let keyring = keyring::Entry::new("voxfusion", "auth_token")
        .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
    keyring.set_password(&token)
        .map_err(|e| format!("Failed to store token: {}", e))?;
    Ok(())
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
async fn get_token() -> Result<Option<String>, String> {
    let keyring = keyring::Entry::new("voxfusion", "auth_token")
        .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
    match keyring.get_password() {
        Ok(token) => Ok(Some(token)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Failed to retrieve token: {}", e)),
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
async fn delete_token() -> Result<(), String> {
    let keyring = keyring::Entry::new("voxfusion", "auth_token")
        .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
    keyring.delete_password()
        .map_err(|e| format!("Failed to delete token: {}", e))?;
    Ok(())
}

#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command]
async fn store_token(app: tauri::AppHandle, token: String) -> Result<(), String> {
    use tauri_plugin_keychain::{KeychainExt, models::KeychainRequest};
    let keychain = app.save_item();
    let request = KeychainRequest {
        key: Some("voxfusion:auth_token".to_string()),
        password: Some(token),
    };
    keychain.save_item(request)
        .map_err(|e| format!("Failed to store token: {}", e))?;
    Ok(())
}

#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command]
async fn get_token(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_keychain::{KeychainExt, models::KeychainRequest};
    let keychain = app.get_item();
    let request = KeychainRequest {
        key: Some("voxfusion:auth_token".to_string()),
        password: None,
    };
    match keychain.get_item(request) {
        Ok(response) => Ok(response.password),
        Err(e) => {
            let error_str = e.to_string();
            if error_str.contains("not found") || error_str.contains("NoEntry") {
                Ok(None)
            } else {
                Err(format!("Failed to retrieve token: {}", e))
            }
        }
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command]
async fn delete_token(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_keychain::{KeychainExt, models::KeychainRequest};
    let keychain = app.remove_item();
    let request = KeychainRequest {
        key: Some("voxfusion:auth_token".to_string()),
        password: None,
    };
    keychain.remove_item(request)
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
