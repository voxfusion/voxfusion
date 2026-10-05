//! The two kinds of hotkey. Modifier-only ones (`LeftControl+LeftOption`) are
//! observed by the system key watcher; combinations ending in a regular key
//! (`Command+Shift+K`) are registered with the system as global shortcuts.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

use crate::backend::{AppEvent, EventSender, ShortcutState};
use crate::platform::main_thread;

#[cfg(target_os = "macos")]
pub fn start_system_key_watcher(events: &EventSender) -> Result<(), String> {
    crate::platform::system_key_watcher::setup(events);
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn start_system_key_watcher(events: &EventSender) -> Result<(), String> {
    crate::platform::linux_key_watcher::setup(events)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn start_system_key_watcher(_events: &EventSender) -> Result<(), String> {
    Ok(())
}

/// Brings the system key watcher back in step with the keyboard. Call it
/// when the app is reopened or becomes active again: modifier transitions
/// can go unobserved across sleep, screen lock and activation changes.
#[cfg(target_os = "macos")]
pub fn resynchronize_system_keys(reason: &str) {
    crate::platform::system_key_watcher::resynchronize(reason);
}

#[cfg(target_os = "linux")]
pub fn resynchronize_system_keys(reason: &str) {
    crate::platform::linux_key_watcher::resynchronize(reason);
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn resynchronize_system_keys(_reason: &str) {}

/// Parses a shortcut as settings store it: modifiers first, then one key,
/// joined by `+` (`Command+Shift+K`, `Alt+Space`, `Escape`). Names are the
/// ones the Tauri global shortcut plugin accepted, so stored hotkeys keep
/// working.
fn parse_shortcut(shortcut: &str) -> Result<HotKey, String> {
    shortcut.parse::<HotKey>().map_err(|err| err.to_string())
}

type ShortcutNames = Mutex<HashMap<u32, String>>;

/// The event for a hotkey press or release, naming the shortcut by the
/// string it was registered as.
fn shortcut_event(names: &ShortcutNames, event: GlobalHotKeyEvent) -> Option<AppEvent> {
    let shortcut = names.lock().ok()?.get(&event.id)?.clone();
    let state = match event.state {
        HotKeyState::Pressed => ShortcutState::Pressed,
        HotKeyState::Released => ShortcutState::Released,
    };

    Some(AppEvent::GlobalShortcut { shortcut, state })
}

/// The global shortcuts the app holds.
pub struct GlobalShortcuts {
    /// Created with the first shortcut. Its lock also keeps changes from
    /// interleaving.
    manager: Mutex<Option<GlobalHotKeyManager>>,
    /// The string each registered hotkey was registered as, by hotkey id.
    /// The thread that delivers hotkey events locks it, so it is never held
    /// across a call into the manager, which may wait for that thread.
    names: Arc<ShortcutNames>,
}

impl GlobalShortcuts {
    /// The hotkey library reports to one handler per process, so only the
    /// first instance receives events.
    pub fn new(events: EventSender) -> Self {
        let names = Arc::new(ShortcutNames::default());

        let registered = names.clone();
        GlobalHotKeyEvent::set_event_handler(Some(move |event| {
            if let Some(event) = shortcut_event(&registered, event) {
                events.emit(event);
            }
        }));

        Self {
            manager: Mutex::new(None),
            names,
        }
    }

    fn is_registered(&self, hotkey: &HotKey) -> Result<bool, String> {
        let names = self.names.lock().map_err(|err| err.to_string())?;
        Ok(names.contains_key(&hotkey.id()))
    }

    /// Registering a shortcut the app already holds only updates the string
    /// its events carry.
    pub fn register(&self, shortcut: &str) -> Result<(), String> {
        let hotkey = parse_shortcut(shortcut)?;

        // macOS delivers hotkeys through the main thread's event loop, and
        // the handler and every hotkey must be installed from that thread.
        main_thread::run(|| {
            let mut manager = self.manager.lock().map_err(|err| err.to_string())?;

            if !self.is_registered(&hotkey)? {
                let manager = match &mut *manager {
                    Some(manager) => manager,
                    empty => {
                        empty.insert(GlobalHotKeyManager::new().map_err(|err| err.to_string())?)
                    }
                };
                manager.register(hotkey).map_err(|err| err.to_string())?;
            }

            self.names
                .lock()
                .map_err(|err| err.to_string())?
                .insert(hotkey.id(), shortcut.to_string());
            Ok(())
        })
    }

    /// Does nothing for a shortcut the app does not hold.
    pub fn unregister(&self, shortcut: &str) -> Result<(), String> {
        let hotkey = parse_shortcut(shortcut)?;

        main_thread::run(|| {
            let manager = self.manager.lock().map_err(|err| err.to_string())?;

            let Some(manager) = manager.as_ref() else {
                return Ok(());
            };
            if !self.is_registered(&hotkey)? {
                return Ok(());
            }

            manager.unregister(hotkey).map_err(|err| err.to_string())?;

            self.names
                .lock()
                .map_err(|err| err.to_string())?
                .remove(&hotkey.id());
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use global_hotkey::hotkey::{CMD_OR_CTRL, Code, Modifiers};

    fn parsed(shortcut: &str) -> (Modifiers, Code) {
        let hotkey = parse_shortcut(shortcut)
            .unwrap_or_else(|err| panic!("{shortcut:?} should parse: {err}"));
        (hotkey.mods, hotkey.key)
    }

    #[test]
    fn parses_combinations_and_single_keys() {
        assert_eq!(
            parsed("Command+Shift+K"),
            (Modifiers::SUPER | Modifiers::SHIFT, Code::KeyK)
        );
        assert_eq!(parsed("Alt+Space"), (Modifiers::ALT, Code::Space));
        assert_eq!(parsed("Control+;"), (Modifiers::CONTROL, Code::Semicolon));
        assert_eq!(parsed("Escape"), (Modifiers::empty(), Code::Escape));
        assert_eq!(parsed("F5"), (Modifiers::empty(), Code::F5));
        assert_eq!(
            parsed("Command+Control+Alt+Shift+F12"),
            (
                Modifiers::SUPER | Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT,
                Code::F12
            )
        );
    }

    #[test]
    fn parses_every_modifier_name_settings_can_hold() {
        for (name, modifier) in [
            ("Command", Modifiers::SUPER),
            ("Cmd", Modifiers::SUPER),
            ("CommandOrControl", CMD_OR_CTRL),
            ("CommandOrCtrl", CMD_OR_CTRL),
            ("CmdOrCtrl", CMD_OR_CTRL),
            ("CmdOrControl", CMD_OR_CTRL),
            ("Control", Modifiers::CONTROL),
            ("Ctrl", Modifiers::CONTROL),
            ("Alt", Modifiers::ALT),
            ("Option", Modifiers::ALT),
            ("Shift", Modifiers::SHIFT),
        ] {
            assert_eq!(parsed(&format!("{name}+K")), (modifier, Code::KeyK));
        }
    }

    #[test]
    fn parses_every_key_name_the_hotkey_recorder_writes() {
        // Letters and digits are stored without their `Key` and `Digit`
        // prefixes, punctuation as the character itself.
        let keys = [
            ("A", Code::KeyA),
            ("B", Code::KeyB),
            ("C", Code::KeyC),
            ("D", Code::KeyD),
            ("E", Code::KeyE),
            ("F", Code::KeyF),
            ("G", Code::KeyG),
            ("H", Code::KeyH),
            ("I", Code::KeyI),
            ("J", Code::KeyJ),
            ("K", Code::KeyK),
            ("L", Code::KeyL),
            ("M", Code::KeyM),
            ("N", Code::KeyN),
            ("O", Code::KeyO),
            ("P", Code::KeyP),
            ("Q", Code::KeyQ),
            ("R", Code::KeyR),
            ("S", Code::KeyS),
            ("T", Code::KeyT),
            ("U", Code::KeyU),
            ("V", Code::KeyV),
            ("W", Code::KeyW),
            ("X", Code::KeyX),
            ("Y", Code::KeyY),
            ("Z", Code::KeyZ),
            ("0", Code::Digit0),
            ("1", Code::Digit1),
            ("2", Code::Digit2),
            ("3", Code::Digit3),
            ("4", Code::Digit4),
            ("5", Code::Digit5),
            ("6", Code::Digit6),
            ("7", Code::Digit7),
            ("8", Code::Digit8),
            ("9", Code::Digit9),
            ("`", Code::Backquote),
            ("\\", Code::Backslash),
            ("[", Code::BracketLeft),
            ("]", Code::BracketRight),
            (",", Code::Comma),
            ("=", Code::Equal),
            ("-", Code::Minus),
            (".", Code::Period),
            ("'", Code::Quote),
            (";", Code::Semicolon),
            ("/", Code::Slash),
            ("Space", Code::Space),
            ("Tab", Code::Tab),
            ("Enter", Code::Enter),
            ("Escape", Code::Escape),
            ("Backspace", Code::Backspace),
            ("Delete", Code::Delete),
            ("CapsLock", Code::CapsLock),
            ("ArrowDown", Code::ArrowDown),
            ("ArrowLeft", Code::ArrowLeft),
            ("ArrowRight", Code::ArrowRight),
            ("ArrowUp", Code::ArrowUp),
            ("Home", Code::Home),
            ("End", Code::End),
            ("PageUp", Code::PageUp),
            ("PageDown", Code::PageDown),
            ("Numpad0", Code::Numpad0),
            ("Numpad1", Code::Numpad1),
            ("Numpad2", Code::Numpad2),
            ("Numpad3", Code::Numpad3),
            ("Numpad4", Code::Numpad4),
            ("Numpad5", Code::Numpad5),
            ("Numpad6", Code::Numpad6),
            ("Numpad7", Code::Numpad7),
            ("Numpad8", Code::Numpad8),
            ("Numpad9", Code::Numpad9),
            ("NumpadAdd", Code::NumpadAdd),
            ("NumpadDecimal", Code::NumpadDecimal),
            ("NumpadDivide", Code::NumpadDivide),
            ("NumpadEnter", Code::NumpadEnter),
            ("NumpadEqual", Code::NumpadEqual),
            ("NumpadMultiply", Code::NumpadMultiply),
            ("NumpadSubtract", Code::NumpadSubtract),
            ("F1", Code::F1),
            ("F2", Code::F2),
            ("F3", Code::F3),
            ("F4", Code::F4),
            ("F5", Code::F5),
            ("F6", Code::F6),
            ("F7", Code::F7),
            ("F8", Code::F8),
            ("F9", Code::F9),
            ("F10", Code::F10),
            ("F11", Code::F11),
            ("F12", Code::F12),
            ("F13", Code::F13),
            ("F14", Code::F14),
            ("F15", Code::F15),
            ("F16", Code::F16),
            ("F17", Code::F17),
            ("F18", Code::F18),
            ("F19", Code::F19),
            ("F20", Code::F20),
            ("F21", Code::F21),
            ("F22", Code::F22),
            ("F23", Code::F23),
            ("F24", Code::F24),
        ];

        for (name, code) in keys {
            assert_eq!(
                parsed(&format!("Command+{name}")),
                (Modifiers::SUPER, code),
                "Command+{name}"
            );
        }
    }

    #[test]
    fn rejects_what_is_not_a_global_shortcut() {
        for shortcut in [
            "",
            "Command+Shift",
            "Command+",
            "Command+K+J",
            "K+Command",
            // Modifier-only hotkeys belong to the system key watcher.
            "LeftControl+LeftOption",
            "RightCommand",
            // Not modifiers the system can combine into a shortcut.
            "Fn+K",
            "CapsLock+K",
            "Command+NoSuchKey",
        ] {
            assert!(parse_shortcut(shortcut).is_err(), "{shortcut:?}");
        }
    }

    #[test]
    fn spellings_of_one_combination_share_an_id() {
        let id = |shortcut| parse_shortcut(shortcut).unwrap().id();

        assert_eq!(id("Command+K"), id("Cmd+K"));
        assert_eq!(id("Alt+Shift+K"), id("Shift+Option+K"));
        assert_ne!(id("Command+K"), id("Control+K"));
        assert_ne!(id("Command+K"), id("Command+J"));
    }

    #[test]
    fn events_name_the_shortcut_as_it_was_registered() {
        let hotkey = parse_shortcut("Command+Shift+K").unwrap();
        let names = ShortcutNames::default();
        names
            .lock()
            .unwrap()
            .insert(hotkey.id(), "Command+Shift+K".to_string());

        let event = |state| GlobalHotKeyEvent {
            id: hotkey.id(),
            state,
        };

        assert_eq!(
            shortcut_event(&names, event(HotKeyState::Pressed)),
            Some(AppEvent::GlobalShortcut {
                shortcut: "Command+Shift+K".into(),
                state: ShortcutState::Pressed,
            })
        );
        assert_eq!(
            shortcut_event(&names, event(HotKeyState::Released)),
            Some(AppEvent::GlobalShortcut {
                shortcut: "Command+Shift+K".into(),
                state: ShortcutState::Released,
            })
        );

        let other = parse_shortcut("Escape").unwrap();
        assert_eq!(
            shortcut_event(
                &names,
                GlobalHotKeyEvent {
                    id: other.id(),
                    state: HotKeyState::Pressed,
                }
            ),
            None
        );
    }
}
