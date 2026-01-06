use argon2::{Algorithm, Argon2, Params, Version};
use enigo::{Enigo, Keyboard, Settings};
use tauri::Manager;

#[tauri::command]
fn type_text(text: String) -> Result<(), String> {
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;

    enigo.text(&text).map_err(|e| e.to_string())?;

    Ok(())
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
        .invoke_handler(tauri::generate_handler![type_text])
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_macos_permissions::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_keychain::init())
        .setup(|app| {
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                if let Err(e) = app.deep_link().register("voxfusion") {
                    eprintln!("Failed to register deep link: {}", e);
                }
            }

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
            app.handle()
                .plugin(tauri_plugin_global_shortcut::Builder::new().build());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
