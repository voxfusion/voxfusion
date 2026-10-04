//! The menu bar item. VoxFusion has no Dock icon and so no menu bar of its
//! own; this menu is where the main window, the microphone choice, an update
//! check and Quit are reached.

#[cfg(target_os = "macos")]
pub use macos::Tray;
#[cfg(not(target_os = "macos"))]
pub use unsupported::Tray;

/// What the user chose in the menu.
// Only the macOS menu produces these.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayCommand {
    ShowHome,
    SelectMicrophone(String),
    CheckForUpdates,
    Quit,
}

#[cfg(target_os = "macos")]
mod macos {
    use std::cell::RefCell;
    use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, Submenu};
    use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

    use super::TrayCommand;

    const HOME_ID: &str = "home";
    const CHECK_FOR_UPDATES_ID: &str = "check_for_updates";
    const QUIT_ID: &str = "quit";
    const MICROPHONE_PREFIX: &str = "mic_";

    fn command_for(id: &str) -> Option<TrayCommand> {
        match id {
            HOME_ID => Some(TrayCommand::ShowHome),
            CHECK_FOR_UPDATES_ID => Some(TrayCommand::CheckForUpdates),
            QUIT_ID => Some(TrayCommand::Quit),
            _ => id
                .strip_prefix(MICROPHONE_PREFIX)
                .map(|name| TrayCommand::SelectMicrophone(name.to_string())),
        }
    }

    /// The menu bar item. It must be created and kept on the main thread.
    pub struct Tray {
        _icon: TrayIcon,
        microphones: Submenu,
        microphone_items: RefCell<Vec<CheckMenuItem>>,
    }

    impl Tray {
        /// Adds the item to the menu bar. `on_command` runs on the main
        /// thread when a menu item is chosen.
        pub fn new(
            on_command: impl Fn(TrayCommand) + Send + Sync + 'static,
        ) -> Result<Tray, String> {
            let error = |error: &dyn std::fmt::Display| error.to_string();

            let microphones = Submenu::with_id("microphone", "Microphone", true);
            let menu = Menu::with_items(&[
                &MenuItem::with_id(HOME_ID, "Home", true, None),
                &microphones,
                &MenuItem::with_id(CHECK_FOR_UPDATES_ID, "Check for Updates", true, None),
                &MenuItem::with_id(QUIT_ID, "Quit", true, None),
            ])
            .map_err(|e| error(&e))?;

            let image = decode_png(include_bytes!("../assets/images/tray-icon.png"))?;
            let icon =
                Icon::from_rgba(image.pixels, image.width, image.height).map_err(|e| error(&e))?;

            let tray_icon = TrayIconBuilder::new()
                .with_icon(icon)
                // A template image is drawn in the menu bar's own color.
                .with_icon_as_template(true)
                .with_menu(Box::new(menu))
                .with_menu_on_left_click(true)
                .build()
                .map_err(|e| error(&e))?;

            MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                if let Some(command) = command_for(event.id().as_ref()) {
                    on_command(command);
                }
            }));

            Ok(Tray {
                _icon: tray_icon,
                microphones,
                microphone_items: RefCell::new(Vec::new()),
            })
        }

        /// Lists `devices` (name, whether it is the system default) in the
        /// Microphone submenu, with a check mark on the selected one.
        pub fn set_microphones(&self, devices: &[(String, bool)], selected: Option<&str>) {
            let mut items = self.microphone_items.borrow_mut();

            for item in items.drain(..) {
                let _ = self.microphones.remove(&item);
            }

            for (name, is_default) in devices {
                let label = if *is_default {
                    format!("{name} (Default)")
                } else {
                    name.clone()
                };
                let item = CheckMenuItem::with_id(
                    format!("{MICROPHONE_PREFIX}{name}"),
                    label,
                    true,
                    selected == Some(name.as_str()),
                    None,
                );

                if self.microphones.append(&item).is_ok() {
                    items.push(item);
                }
            }
        }
    }

    struct Rgba {
        pixels: Vec<u8>,
        width: u32,
        height: u32,
    }

    fn decode_png(bytes: &[u8]) -> Result<Rgba, String> {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        // Palette and gray images become 8-bit, which is what is handled below.
        decoder.set_transformations(png::Transformations::normalize_to_color8());

        let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
        let mut buffer = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buffer).map_err(|e| e.to_string())?;
        buffer.truncate(info.buffer_size());

        let pixels = match info.color_type {
            png::ColorType::Rgba => buffer,
            png::ColorType::Rgb => buffer
                .chunks_exact(3)
                .flat_map(|rgb| [rgb[0], rgb[1], rgb[2], 255])
                .collect(),
            png::ColorType::GrayscaleAlpha => buffer
                .chunks_exact(2)
                .flat_map(|ga| [ga[0], ga[0], ga[0], ga[1]])
                .collect(),
            png::ColorType::Grayscale => buffer.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
            other => return Err(format!("unsupported tray icon color type {other:?}")),
        };

        Ok(Rgba {
            pixels,
            width: info.width,
            height: info.height,
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn menu_ids_map_to_commands() {
            assert_eq!(command_for("home"), Some(TrayCommand::ShowHome));
            assert_eq!(command_for("quit"), Some(TrayCommand::Quit));
            assert_eq!(
                command_for("check_for_updates"),
                Some(TrayCommand::CheckForUpdates)
            );
            assert_eq!(
                command_for("mic_MacBook Pro Microphone"),
                Some(TrayCommand::SelectMicrophone(
                    "MacBook Pro Microphone".into()
                ))
            );
            assert_eq!(command_for("something-else"), None);
        }

        #[test]
        fn the_tray_icon_decodes() {
            let image = decode_png(include_bytes!("../assets/images/tray-icon.png")).unwrap();

            assert_eq!(
                image.pixels.len(),
                (image.width * image.height * 4) as usize
            );
        }
    }
}

/// Other platforms run the app for development only, without a menu bar item.
#[cfg(not(target_os = "macos"))]
mod unsupported {
    use super::TrayCommand;

    pub struct Tray;

    impl Tray {
        pub fn new(
            _on_command: impl Fn(TrayCommand) + Send + Sync + 'static,
        ) -> Result<Tray, String> {
            Ok(Tray)
        }

        pub fn set_microphones(&self, _devices: &[(String, bool)], _selected: Option<&str>) {}
    }
}
