#[cfg(desktop)]
use tauri::Manager;

#[cfg(desktop)]
pub fn show_or_create_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        use tauri::WebviewWindowBuilder;

        let mut builder = WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("/".into()))
            .title("VoxFusion")
            .inner_size(1360.0, 850.0)
            .min_inner_size(1024.0, 720.0)
            .resizable(true)
            .decorations(true)
            .fullscreen(false)
            .center();

        #[cfg(target_os = "macos")]
        {
            builder = builder
                .title_bar_style(tauri::TitleBarStyle::Overlay)
                .hidden_title(true);
        }

        if let Ok(window) = builder.build()
        {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}
