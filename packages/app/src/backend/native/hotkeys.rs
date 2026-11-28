//! The two kinds of hotkey. Modifier-only ones (`LeftControl+LeftOption`) are
//! observed by the system key watcher; combinations ending in a regular key
//! (`Command+Shift+K`) are registered with the system as global shortcuts.

#[cfg(not(target_os = "windows"))]
use std::sync::{Arc, Mutex};

use global_hotkey::hotkey::HotKey;
#[cfg(not(target_os = "windows"))]
use global_hotkey::GlobalHotKeyManager;
#[cfg(any(not(target_os = "windows"), test))]
use global_hotkey::GlobalHotKeyEvent;
#[cfg(test)]
use global_hotkey::HotKeyState;

use crate::backend::EventSender;
#[cfg(any(not(target_os = "windows"), test))]
use crate::backend::{AppEvent, ShortcutState};
#[cfg(not(target_os = "windows"))]
use crate::platform::main_thread;

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn start_system_key_watcher(events: &EventSender) -> Result<(), String> {
    crate::platform::system_key_watcher::setup(events);
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn start_system_key_watcher(_events: &EventSender) -> Result<(), String> {
    Ok(())
}

/// Brings the system key watcher back in step with the keyboard. Call it
/// when the app is reopened or becomes active again: modifier transitions
/// can go unobserved across sleep, screen lock and activation changes.
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn resynchronize_system_keys(reason: &str) {
    crate::platform::system_key_watcher::resynchronize(reason);
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn resynchronize_system_keys(_reason: &str) {}

/// Parses a shortcut as settings store it: modifiers first, then one key,
/// joined by `+` (`Command+Shift+K`, `Alt+Space`, `Escape`). Names are the
/// ones the Tauri global shortcut plugin accepted, so stored hotkeys keep
/// working.
fn parse_shortcut(shortcut: &str) -> Result<HotKey, String> {
    shortcut.parse::<HotKey>().map_err(|err| err.to_string())
}

#[cfg(any(not(target_os = "windows"), test))]
type ShortcutNames = std::sync::Mutex<std::collections::HashMap<u32, String>>;

/// The event for a hotkey press or release, naming the shortcut by the
/// string it was registered as.
#[cfg(any(not(target_os = "windows"), test))]
fn shortcut_event(
    names: &ShortcutNames,
    event: global_hotkey::GlobalHotKeyEvent,
) -> Option<AppEvent> {
    let shortcut = names.lock().ok()?.get(&event.id)?.clone();
    let state = match event.state {
        global_hotkey::HotKeyState::Pressed => ShortcutState::Pressed,
        global_hotkey::HotKeyState::Released => ShortcutState::Released,
    };

    Some(AppEvent::GlobalShortcut { shortcut, state })
}

/// The global shortcuts the app holds.
#[cfg(not(target_os = "windows"))]
pub struct GlobalShortcuts {
    /// Created with the first shortcut. Its lock also keeps changes from
    /// interleaving.
    manager: Mutex<Option<GlobalHotKeyManager>>,
    /// The string each registered hotkey was registered as, by hotkey id.
    /// The thread that delivers hotkey events locks it, so it is never held
    /// across a call into the manager, which may wait for that thread.
    names: Arc<ShortcutNames>,
}

#[cfg(not(target_os = "windows"))]
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

/// On Windows, `RegisterHotKey` delivers `WM_HOTKEY` exclusively to the thread
/// that registered it, which must pump a Win32 message loop. A dedicated
/// worker thread owns the hidden message window and all registrations so
/// `GlobalShortcuts` is `Send + Sync` and independent of GPUI's thread.
#[cfg(target_os = "windows")]
pub struct GlobalShortcuts {
    commands: std::sync::mpsc::Sender<win_hotkeys::Command>,
    worker_thread_id: std::sync::Arc<std::sync::atomic::AtomicU32>,
}

#[cfg(target_os = "windows")]
impl GlobalShortcuts {
    pub fn new(events: EventSender) -> Self {
        win_hotkeys::start(events)
    }

    pub fn register(&self, shortcut: &str) -> Result<(), String> {
        let hotkey = parse_shortcut(shortcut)?;
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.commands
            .send(win_hotkeys::Command::Register {
                hotkey,
                name: shortcut.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| "Hotkey worker thread stopped".to_string())?;
        win_hotkeys::wake(&self.worker_thread_id);
        reply_rx
            .recv()
            .map_err(|_| "Hotkey worker thread stopped".to_string())?
    }

    pub fn unregister(&self, shortcut: &str) -> Result<(), String> {
        let hotkey = parse_shortcut(shortcut)?;
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.commands
            .send(win_hotkeys::Command::Unregister {
                hotkey,
                reply: reply_tx,
            })
            .map_err(|_| "Hotkey worker thread stopped".to_string())?;
        win_hotkeys::wake(&self.worker_thread_id);
        reply_rx
            .recv()
            .map_err(|_| "Hotkey worker thread stopped".to_string())?
    }
}

#[cfg(target_os = "windows")]
mod win_hotkeys {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::mpsc;
    use std::time::Duration;

    use global_hotkey::hotkey::{Code, HotKey, Modifiers};
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT,
        MOD_WIN, RegisterHotKey, UnregisterHotKey, VIRTUAL_KEY, VK_0, VK_1, VK_2, VK_3, VK_4, VK_5,
        VK_6, VK_7, VK_8, VK_9, VK_A, VK_ADD, VK_B, VK_BACK, VK_C, VK_CAPITAL, VK_CONTROL, VK_D,
        VK_DECIMAL, VK_DELETE, VK_DIVIDE, VK_DOWN, VK_E, VK_END, VK_ESCAPE, VK_F, VK_F1, VK_F2,
        VK_F3, VK_F4, VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_F10, VK_F11, VK_F12, VK_F13, VK_F14,
        VK_F15, VK_F16, VK_F17, VK_F18, VK_F19, VK_F20, VK_F21, VK_F22, VK_F23, VK_F24, VK_G,
        VK_H, VK_HOME, VK_I, VK_INSERT, VK_J, VK_K, VK_L, VK_LEFT, VK_LWIN, VK_M,
        VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK, VK_MEDIA_STOP, VK_MENU,
        VK_MULTIPLY, VK_N, VK_NEXT, VK_NUMLOCK, VK_NUMPAD0, VK_NUMPAD1, VK_NUMPAD2, VK_NUMPAD3,
        VK_NUMPAD4, VK_NUMPAD5, VK_NUMPAD6, VK_NUMPAD7, VK_NUMPAD8, VK_NUMPAD9, VK_O, VK_OEM_1,
        VK_OEM_2, VK_OEM_3, VK_OEM_4, VK_OEM_5, VK_OEM_6, VK_OEM_7, VK_OEM_COMMA, VK_OEM_MINUS,
        VK_OEM_PERIOD, VK_OEM_PLUS, VK_P, VK_PAUSE, VK_PLAY, VK_PRIOR, VK_Q, VK_R, VK_RETURN,
        VK_RIGHT, VK_RWIN, VK_S, VK_SCROLL, VK_SHIFT, VK_SNAPSHOT, VK_SPACE, VK_SUBTRACT, VK_T,
        VK_TAB, VK_U, VK_UP, VK_V, VK_VOLUME_DOWN, VK_VOLUME_MUTE, VK_VOLUME_UP, VK_W, VK_X, VK_Y,
        VK_Z,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, WM_APP, WM_HOTKEY,
    };

    use super::GlobalShortcuts;
    use crate::backend::{AppEvent, EventSender, ShortcutState};

    const WM_APP_COMMAND: u32 = WM_APP + 1;

    pub enum Command {
        Register {
            hotkey: HotKey,
            name: String,
            reply: mpsc::Sender<Result<(), String>>,
        },
        Unregister {
            hotkey: HotKey,
            reply: mpsc::Sender<Result<(), String>>,
        },
    }

    pub fn wake(thread_id: &AtomicU32) {
        let tid = thread_id.load(Ordering::Acquire);
        if tid != 0 {
            unsafe {
                let _ = PostThreadMessageW(tid, WM_APP_COMMAND, WPARAM(0), LPARAM(0));
            }
        }
    }

    pub fn start(events: EventSender) -> GlobalShortcuts {
        let (tx, rx) = mpsc::channel::<Command>();
        let worker_thread_id = Arc::new(AtomicU32::new(0));
        let (ready_tx, ready_rx) = mpsc::channel::<u32>();

        let tid_slot = worker_thread_id.clone();
        let _ = std::thread::Builder::new()
            .name("global-hotkeys".into())
            .spawn(move || {
                let mut msg = MSG::default();
                // Force creation of this thread's message queue before publishing tid.
                unsafe {
                    let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
                }
                let tid = unsafe { GetCurrentThreadId() };
                tid_slot.store(tid, Ordering::Release);
                let _ = ready_tx.send(tid);

                // Win32 RegisterHotKey expects application IDs in 0x0000..=0xBFFF.
                // Map global_hotkey's 32-bit hash id to a small sequential Win32 id.
                let mut hash_to_win_id: HashMap<u32, i32> = HashMap::new();
                let mut registered: HashMap<i32, (String, u16, Modifiers)> = HashMap::new();
                let mut next_win_id: i32 = 1;
                // Drain any commands sent before the queue was ready.
                drain_commands(&rx, &mut hash_to_win_id, &mut registered, &mut next_win_id);

                while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
                    if msg.message == WM_APP_COMMAND {
                        drain_commands(&rx, &mut hash_to_win_id, &mut registered, &mut next_win_id);
                    } else if msg.message == WM_HOTKEY {
                        let id = msg.wParam.0 as i32;
                        if let Some((shortcut, vk, mods)) = registered.get(&id).cloned() {
                            events.emit(AppEvent::GlobalShortcut {
                                shortcut: shortcut.clone(),
                                state: ShortcutState::Pressed,
                            });
                            let release_events = events.clone();
                            let _ = std::thread::Builder::new()
                                .name("hotkey-release-watch".into())
                                .spawn(move || {
                                    // Poll gently until the main key or any required modifier
                                    // is released so hold-to-speak combo hotkeys get their
                                    // Released event cleanly without busy-spinning.
                                    while is_chord_held(vk, mods) {
                                        std::thread::sleep(Duration::from_millis(15));
                                    }
                                    release_events.emit(AppEvent::GlobalShortcut {
                                        shortcut,
                                        state: ShortcutState::Released,
                                    });
                                });
                        }
                    }
                }
            });

        let _ = ready_rx.recv();

        GlobalShortcuts {
            commands: tx,
            worker_thread_id,
        }
    }

    fn is_chord_held(vk: u16, mods: Modifiers) -> bool {
        let down = |code: VIRTUAL_KEY| unsafe { GetAsyncKeyState(code.0 as i32) < 0 };
        if !down(VIRTUAL_KEY(vk)) {
            return false;
        }
        if mods.contains(Modifiers::CONTROL) && !down(VK_CONTROL) {
            return false;
        }
        if mods.contains(Modifiers::ALT) && !down(VK_MENU) {
            return false;
        }
        if mods.contains(Modifiers::SHIFT) && !down(VK_SHIFT) {
            return false;
        }
        if mods.intersects(Modifiers::SUPER | Modifiers::META)
            && !down(VK_LWIN)
            && !down(VK_RWIN)
        {
            return false;
        }
        true
    }

    fn drain_commands(
        rx: &mpsc::Receiver<Command>,
        hash_to_win_id: &mut HashMap<u32, i32>,
        registered: &mut HashMap<i32, (String, u16, Modifiers)>,
        next_win_id: &mut i32,
    ) {
        while let Ok(command) = rx.try_recv() {
            match command {
                Command::Register {
                    hotkey,
                    name,
                    reply,
                } => {
                    let hash_id = hotkey.id();
                    let result = if let Some(&win_id) = hash_to_win_id.get(&hash_id) {
                        if let Some(entry) = registered.get_mut(&win_id) {
                            entry.0 = name;
                        }
                        Ok(())
                    } else {
                        let win_id = alloc_win_id(registered, next_win_id);
                        register_hotkey(win_id, hotkey).map(|vk| {
                            hash_to_win_id.insert(hash_id, win_id);
                            registered.insert(win_id, (name, vk, hotkey.mods));
                        })
                    };
                    let _ = reply.send(result);
                }
                Command::Unregister { hotkey, reply } => {
                    let hash_id = hotkey.id();
                    let result = if let Some(win_id) = hash_to_win_id.remove(&hash_id) {
                        registered.remove(&win_id);
                        unsafe { UnregisterHotKey(None, win_id) }.map_err(|err| err.to_string())
                    } else {
                        Ok(())
                    };
                    let _ = reply.send(result);
                }
            }
        }
    }

    fn alloc_win_id(
        registered: &HashMap<i32, (String, u16, Modifiers)>,
        next_win_id: &mut i32,
    ) -> i32 {
        for _ in 0..0xBFFF {
            let candidate = *next_win_id;
            *next_win_id = if candidate >= 0xBFFF { 1 } else { candidate + 1 };
            if !registered.contains_key(&candidate) {
                return candidate;
            }
        }
        1
    }

    fn register_hotkey(id: i32, hotkey: HotKey) -> Result<u16, String> {
        let mut mods = MOD_NOREPEAT;
        if hotkey.mods.contains(Modifiers::SHIFT) {
            mods |= MOD_SHIFT;
        }
        if hotkey.mods.intersects(Modifiers::SUPER | Modifiers::META) {
            mods |= MOD_WIN;
        }
        if hotkey.mods.contains(Modifiers::ALT) {
            mods |= MOD_ALT;
        }
        if hotkey.mods.contains(Modifiers::CONTROL) {
            mods |= MOD_CONTROL;
        }
        let vk = key_to_vk(&hotkey.key)
            .ok_or_else(|| format!("Unsupported Windows virtual key for {:?}", hotkey.key))?;
        unsafe { RegisterHotKey(None, id, HOT_KEY_MODIFIERS(mods.0), vk.0 as u32) }
            .map_err(|err| format!("Failed to register global shortcut: {err}"))?;
        Ok(vk.0)
    }

    fn key_to_vk(key: &Code) -> Option<VIRTUAL_KEY> {
        Some(match key {
            Code::KeyA => VK_A,
            Code::KeyB => VK_B,
            Code::KeyC => VK_C,
            Code::KeyD => VK_D,
            Code::KeyE => VK_E,
            Code::KeyF => VK_F,
            Code::KeyG => VK_G,
            Code::KeyH => VK_H,
            Code::KeyI => VK_I,
            Code::KeyJ => VK_J,
            Code::KeyK => VK_K,
            Code::KeyL => VK_L,
            Code::KeyM => VK_M,
            Code::KeyN => VK_N,
            Code::KeyO => VK_O,
            Code::KeyP => VK_P,
            Code::KeyQ => VK_Q,
            Code::KeyR => VK_R,
            Code::KeyS => VK_S,
            Code::KeyT => VK_T,
            Code::KeyU => VK_U,
            Code::KeyV => VK_V,
            Code::KeyW => VK_W,
            Code::KeyX => VK_X,
            Code::KeyY => VK_Y,
            Code::KeyZ => VK_Z,
            Code::Digit0 => VK_0,
            Code::Digit1 => VK_1,
            Code::Digit2 => VK_2,
            Code::Digit3 => VK_3,
            Code::Digit4 => VK_4,
            Code::Digit5 => VK_5,
            Code::Digit6 => VK_6,
            Code::Digit7 => VK_7,
            Code::Digit8 => VK_8,
            Code::Digit9 => VK_9,
            Code::Equal => VK_OEM_PLUS,
            Code::Comma => VK_OEM_COMMA,
            Code::Minus => VK_OEM_MINUS,
            Code::Period => VK_OEM_PERIOD,
            Code::Semicolon => VK_OEM_1,
            Code::Slash => VK_OEM_2,
            Code::Backquote => VK_OEM_3,
            Code::BracketLeft => VK_OEM_4,
            Code::Backslash => VK_OEM_5,
            Code::BracketRight => VK_OEM_6,
            Code::Quote => VK_OEM_7,
            Code::Backspace => VK_BACK,
            Code::Tab => VK_TAB,
            Code::Space => VK_SPACE,
            Code::Enter => VK_RETURN,
            Code::CapsLock => VK_CAPITAL,
            Code::Escape => VK_ESCAPE,
            Code::PageUp => VK_PRIOR,
            Code::PageDown => VK_NEXT,
            Code::End => VK_END,
            Code::Home => VK_HOME,
            Code::ArrowLeft => VK_LEFT,
            Code::ArrowUp => VK_UP,
            Code::ArrowRight => VK_RIGHT,
            Code::ArrowDown => VK_DOWN,
            Code::PrintScreen => VK_SNAPSHOT,
            Code::Insert => VK_INSERT,
            Code::Delete => VK_DELETE,
            Code::F1 => VK_F1,
            Code::F2 => VK_F2,
            Code::F3 => VK_F3,
            Code::F4 => VK_F4,
            Code::F5 => VK_F5,
            Code::F6 => VK_F6,
            Code::F7 => VK_F7,
            Code::F8 => VK_F8,
            Code::F9 => VK_F9,
            Code::F10 => VK_F10,
            Code::F11 => VK_F11,
            Code::F12 => VK_F12,
            Code::F13 => VK_F13,
            Code::F14 => VK_F14,
            Code::F15 => VK_F15,
            Code::F16 => VK_F16,
            Code::F17 => VK_F17,
            Code::F18 => VK_F18,
            Code::F19 => VK_F19,
            Code::F20 => VK_F20,
            Code::F21 => VK_F21,
            Code::F22 => VK_F22,
            Code::F23 => VK_F23,
            Code::F24 => VK_F24,
            Code::NumLock => VK_NUMLOCK,
            Code::Numpad0 => VK_NUMPAD0,
            Code::Numpad1 => VK_NUMPAD1,
            Code::Numpad2 => VK_NUMPAD2,
            Code::Numpad3 => VK_NUMPAD3,
            Code::Numpad4 => VK_NUMPAD4,
            Code::Numpad5 => VK_NUMPAD5,
            Code::Numpad6 => VK_NUMPAD6,
            Code::Numpad7 => VK_NUMPAD7,
            Code::Numpad8 => VK_NUMPAD8,
            Code::Numpad9 => VK_NUMPAD9,
            Code::NumpadAdd => VK_ADD,
            Code::NumpadDecimal => VK_DECIMAL,
            Code::NumpadDivide => VK_DIVIDE,
            Code::NumpadEnter => VK_RETURN,
            Code::NumpadEqual => VIRTUAL_KEY(0x92), // VK_OEM_NEC_EQUAL
            Code::NumpadMultiply => VK_MULTIPLY,
            Code::NumpadSubtract => VK_SUBTRACT,
            Code::ScrollLock => VK_SCROLL,
            Code::AudioVolumeDown => VK_VOLUME_DOWN,
            Code::AudioVolumeUp => VK_VOLUME_UP,
            Code::AudioVolumeMute => VK_VOLUME_MUTE,
            Code::MediaPlay => VK_PLAY,
            Code::MediaPause => VK_PAUSE,
            Code::MediaPlayPause => VK_MEDIA_PLAY_PAUSE,
            Code::MediaStop => VK_MEDIA_STOP,
            Code::MediaTrackNext => VK_MEDIA_NEXT_TRACK,
            Code::MediaTrackPrevious => VK_MEDIA_PREV_TRACK,
            Code::Pause => VK_PAUSE,
            _ => return None,
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
