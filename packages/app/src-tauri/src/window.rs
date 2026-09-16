#[cfg(desktop)]
use tauri::Manager;

#[cfg(desktop)]
pub fn create_voice_control_window(app: &tauri::App) -> tauri::Result<()> {
    use tauri::WebviewWindowBuilder;

    let builder = WebviewWindowBuilder::new(
        app,
        "voice-control",
        tauri::WebviewUrl::App("voice-control.html".into()),
    )
    .title("VoxFusion Voice Control")
    .inner_size(
        if cfg!(target_os = "linux") {
            260.0
        } else {
            100.0
        },
        28.0,
    )
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .visible_on_all_workspaces(true)
    .skip_taskbar(true)
    .visible(false)
    .focused(false)
    .focusable(false);
    #[cfg(target_os = "macos")]
    let builder = builder.accept_first_mouse(true);
    builder.build()?;
    Ok(())
}

#[cfg(desktop)]
pub fn show_or_create_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        use tauri::WebviewWindowBuilder;

        let builder = WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("/".into()))
            .title("VoxFusion")
            .inner_size(1360.0, 850.0)
            .min_inner_size(1024.0, 720.0)
            .resizable(true)
            .decorations(true)
            .fullscreen(false)
            .center();
        #[cfg(target_os = "macos")]
        let builder = builder
            .title_bar_style(tauri::TitleBarStyle::Overlay)
            .hidden_title(true);
        if let Ok(window) = builder.build() {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}
