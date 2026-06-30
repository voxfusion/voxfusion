#[cfg(desktop)]
use tauri::Manager;

#[cfg(desktop)]
pub fn create_voice_control_window(app: &tauri::App) -> tauri::Result<()> {
    use tauri::WebviewWindowBuilder;

    WebviewWindowBuilder::new(
        app,
        "voice-control",
        tauri::WebviewUrl::App("voice-control.html".into()),
    )
    .title("Voice Control")
    .inner_size(100.0, 28.0)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .visible_on_all_workspaces(true)
    .skip_taskbar(true)
    .visible(false)
    .focused(false)
    .accept_first_mouse(true)
    .build()?;
    Ok(())
}

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
                .min_inner_size(1024.0, 720.0)
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
