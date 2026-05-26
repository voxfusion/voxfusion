#[cfg(desktop)]
use crate::window::show_or_create_main_window;

#[cfg(desktop)]
use tauri::menu::{Menu, MenuItem, MenuItemKind, PredefinedMenuItem};
#[cfg(desktop)]
use tauri::{Emitter, Manager};

#[cfg(desktop)]
const CHECK_FOR_UPDATES_ID: &str = "check_for_updates";
const QUIT_ID: &str = "quit";

#[cfg(desktop)]
pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let menu = Menu::default(app.handle())?;

    if let Some(MenuItemKind::Submenu(app_menu)) = menu.items()?.into_iter().next() {
        // The default macOS app menu Quit item sends a Quit AppleEvent, bypassing
        // Tauri's ExitRequested guard. Replace it with an explicit Tauri exit.
        let last_item_index = app_menu.items()?.len().saturating_sub(1);
        let _ = app_menu.remove_at(last_item_index)?;

        let quit_item = MenuItem::with_id(app, QUIT_ID, "Quit VoxFusion", true, Some("Cmd+Q"))?;
        app_menu.insert_items(
            &[
                &MenuItem::with_id(
                    app,
                    CHECK_FOR_UPDATES_ID,
                    "Check for Updates",
                    true,
                    None::<&str>,
                )?,
                &PredefinedMenuItem::separator(app)?,
            ],
            2,
        )?;
        app_menu.append_items(&[&PredefinedMenuItem::separator(app)?, &quit_item])?;
    }

    app.set_menu(menu)?;
    app.on_menu_event(|app, event| match event.id().as_ref() {
        CHECK_FOR_UPDATES_ID => {
            show_or_create_main_window(app);
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.emit("check-for-updates", ());
            }
        }
        QUIT_ID => app.exit(0),
        _ => {}
    });

    Ok(())
}
