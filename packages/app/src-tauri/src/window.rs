#[cfg(desktop)]
use tauri::Manager;

#[cfg(desktop)]
pub fn show_or_create_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        use tauri::WebviewWindowBuilder;

        if let Ok(window) =
            WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("/".into()))
                .title("VoxFusion")
                .inner_size(1360.0, 850.0)
                .resizable(true)
                .decorations(true)
                .title_bar_style(tauri::TitleBarStyle::Overlay)
                .hidden_title(true)
                .fullscreen(false)
                .center()
                .build()
        {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}
