//! Watches the keyboard on Linux, wherever the focus is: for modifier-only
//! hotkeys, and on Wayland for every hotkey.
//!
//! On X11 it follows XInput 2 raw key events, and recognizes modifiers by
//! what the keyboard layout makes of a key, so a key remapped to Control
//! counts as Control. A Wayland compositor shows an app only the keys typed
//! into its own windows, so there it reads the keyboard devices, which takes
//! being in the `input` group, and modifiers are the physical keys.
//!
//! Key codes are the kernel's (evdev) codes, which X11 key codes are offset
//! from by 8.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, OnceLock};

use global_hotkey::hotkey::{Code, HotKey, Modifiers};

use crate::backend::{AppEvent, EventSender, ShortcutState, SystemKey};

/// The kernel's code for Escape.
pub const ESCAPE_KEY_CODE: i64 = 1;

/// Why hotkeys cannot work on Wayland without access to the keyboards.
const NO_KEYBOARD_ACCESS: &str = "The keyboard cannot be read. On Wayland, VoxFusion reads \
     its hotkeys from the keyboard devices, which takes being in the input group: run \
     `sudo usermod -aG input $USER`, then log out and back in.";

static EVENTS: OnceLock<EventSender> = OnceLock::new();
static KEYS: Mutex<Keys> = Mutex::new(Keys::new());
static SHORTCUTS: Mutex<Shortcuts> = Mutex::new(Shortcuts::new());
/// The watcher, once one runs.
static WATCHER: Mutex<Option<Backend>> = Mutex::new(None);

/// Starts watching, unless a watcher runs already. A watcher that could not
/// start is tried again on the next call, as the keyboard may have become
/// readable since.
pub fn setup(events: &EventSender) -> Result<(), String> {
    EVENTS.set(events.clone()).ok();

    let mut watcher = WATCHER.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if watcher.is_some() {
        return Ok(());
    }

    let (name, started) = if super::session::is_wayland() {
        ("devices", devices::start())
    } else {
        ("x11", x11::start())
    };
    match started {
        Ok(started) => {
            log::info!(target: "hotkey", "system_key_watcher_started backend={name}");
            *watcher = Some(started);
            Ok(())
        }
        Err(err) => {
            log::warn!(target: "hotkey", "system_key_watcher_unavailable backend={name} error={err}");
            Err(err)
        }
    }
}

/// Whether hotkeys can be heard in every app. On X11 they always can; on
/// Wayland only with access to a keyboard device.
pub fn can_watch() -> bool {
    !super::session::is_wayland() || devices::any_keyboard_readable()
}

/// Rebuilds the pressed-key state after the app was reopened, when key
/// transitions may have gone unobserved.
pub fn resynchronize(reason: &str) {
    let watcher = WATCHER.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(backend) = watcher.as_ref() else {
        return;
    };
    let current = backend.held_keys();
    drop(watcher);
    log::info!(target: "hotkey", "system_key_state_lifecycle_resynchronized reason={reason}");
    with_keys(|keys, events| keys.reset(current, events));
}

/// Has `hotkey` reported as `name` while the keyboard devices are watched,
/// where no global shortcut can be registered with the system.
pub fn register_shortcut(hotkey: HotKey, name: &str) {
    let mut shortcuts = SHORTCUTS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    shortcuts.registered.retain(|(registered, _)| registered.id() != hotkey.id());
    shortcuts.registered.push((hotkey, name.to_string()));
}

pub fn unregister_shortcut(hotkey: &HotKey) {
    let mut shortcuts = SHORTCUTS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    shortcuts
        .registered
        .retain(|(registered, _)| registered.id() != hotkey.id());
}

/// A running watcher.
enum Backend {
    X11(x11::Watcher),
    Devices(devices::Watcher),
}

impl Backend {
    /// The key codes held right now, as the system reports them.
    fn held_keys(&self) -> Vec<(u32, Option<SystemKey>)> {
        match self {
            Backend::X11(watcher) => watcher.held_keys(),
            Backend::Devices(watcher) => watcher.held_keys(),
        }
    }
}

fn with_keys(change: impl FnOnce(&mut Keys, &EventSender)) {
    let Some(events) = EVENTS.get() else {
        return;
    };
    let mut keys = KEYS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    change(&mut keys, events);
}

/// The keys held down, as followed through their presses and releases.
#[derive(Debug, Default, PartialEq)]
struct Keys {
    /// The modifiers held, by the code of the key holding each.
    modifiers: BTreeMap<u32, SystemKey>,
    /// Other keys held, so that their autorepeat is not reported again.
    others: BTreeSet<u32>,
}

impl Keys {
    const fn new() -> Self {
        Self {
            modifiers: BTreeMap::new(),
            others: BTreeSet::new(),
        }
    }

    fn pressed_modifiers(&self) -> Vec<SystemKey> {
        let held: BTreeSet<SystemKey> = self.modifiers.values().copied().collect();
        held.into_iter().collect()
    }

    /// The held modifiers as a shortcut has them, either side counting.
    fn combination_modifiers(&self) -> Modifiers {
        self.modifiers
            .values()
            .fold(Modifiers::empty(), |held, key| {
                held | match key {
                    SystemKey::LeftControl | SystemKey::RightControl => Modifiers::CONTROL,
                    SystemKey::LeftOption | SystemKey::RightOption => Modifiers::ALT,
                    SystemKey::LeftShift | SystemKey::RightShift => Modifiers::SHIFT,
                    SystemKey::LeftCommand | SystemKey::RightCommand => Modifiers::SUPER,
                    SystemKey::Fn => Modifiers::empty(),
                }
            })
    }

    fn press(&mut self, code: u32, key: Option<SystemKey>, events: &EventSender) {
        let Some(key) = key else {
            if self.others.insert(code) {
                events.emit(AppEvent::KeyboardKeyPressed {
                    key_code: i64::from(code),
                });
            }
            return;
        };

        let before = self.pressed_modifiers();
        if self.modifiers.insert(code, key).is_some() {
            // Held already: an autorepeat.
            return;
        }
        let pressed_keys = self.pressed_modifiers();
        if pressed_keys != before {
            events.emit(AppEvent::SystemKeyPressed { key, pressed_keys });
        }
    }

    /// Returns false when the release does not follow from the keys known
    /// to be held: a press went unobserved.
    fn release(&mut self, code: u32, key: Option<SystemKey>, events: &EventSender) -> bool {
        let Some(key) = key else {
            self.others.remove(&code);
            return true;
        };
        if self.modifiers.remove(&code).is_none() {
            return false;
        }

        let pressed_keys = self.pressed_modifiers();
        if pressed_keys.contains(&key) {
            // The other key of that modifier is still down.
            return true;
        }
        let none_left = pressed_keys.is_empty();
        events.emit(AppEvent::SystemKeyReleased { key, pressed_keys });
        if none_left {
            events.emit(AppEvent::SystemKeysReleased);
        }
        true
    }

    /// Takes the keys held as the system reports them, and has every hotkey
    /// start over from there.
    fn reset(&mut self, held: Vec<(u32, Option<SystemKey>)>, events: &EventSender) {
        *self = Self::new();
        for (code, key) in held {
            match key {
                Some(key) => {
                    self.modifiers.insert(code, key);
                }
                None => {
                    self.others.insert(code);
                }
            }
        }
        events.emit(AppEvent::SystemKeysReleased);
    }
}

/// The shortcuts matched against the keyboard devices.
struct Shortcuts {
    registered: Vec<(HotKey, String)>,
    /// The key that completed a shortcut, with the shortcut, until the key
    /// is released.
    held: Option<(u32, String)>,
}

impl Shortcuts {
    const fn new() -> Self {
        Self {
            registered: Vec::new(),
            held: None,
        }
    }

    fn key_pressed(&mut self, code: u32, modifiers: Modifiers, events: &EventSender) {
        let Some(key) = combination_key(code) else {
            return;
        };
        let Some((_, name)) = self
            .registered
            .iter()
            .find(|(hotkey, _)| hotkey.key == key && hotkey.mods == modifiers)
        else {
            return;
        };
        self.held = Some((code, name.clone()));
        events.emit(AppEvent::GlobalShortcut {
            shortcut: name.clone(),
            state: ShortcutState::Pressed,
        });
    }

    fn key_released(&mut self, code: u32, events: &EventSender) {
        if self.held.as_ref().is_some_and(|(held, _)| *held == code)
            && let Some((_, shortcut)) = self.held.take()
        {
            events.emit(AppEvent::GlobalShortcut {
                shortcut,
                state: ShortcutState::Released,
            });
        }
    }
}

/// The modifier a physical key is.
fn modifier_for_code(code: u32) -> Option<SystemKey> {
    match code {
        29 => Some(SystemKey::LeftControl),
        97 => Some(SystemKey::RightControl),
        42 => Some(SystemKey::LeftShift),
        54 => Some(SystemKey::RightShift),
        56 => Some(SystemKey::LeftOption),
        100 => Some(SystemKey::RightOption),
        125 => Some(SystemKey::LeftCommand),
        126 => Some(SystemKey::RightCommand),
        _ => None,
    }
}

/// The key a shortcut names for a kernel key code: every key the hotkey
/// recorder can write.
fn combination_key(code: u32) -> Option<Code> {
    use evdev::KeyCode as K;

    let code = K::new(u16::try_from(code).ok()?);
    Some(match code {
        K::KEY_A => Code::KeyA,
        K::KEY_B => Code::KeyB,
        K::KEY_C => Code::KeyC,
        K::KEY_D => Code::KeyD,
        K::KEY_E => Code::KeyE,
        K::KEY_F => Code::KeyF,
        K::KEY_G => Code::KeyG,
        K::KEY_H => Code::KeyH,
        K::KEY_I => Code::KeyI,
        K::KEY_J => Code::KeyJ,
        K::KEY_K => Code::KeyK,
        K::KEY_L => Code::KeyL,
        K::KEY_M => Code::KeyM,
        K::KEY_N => Code::KeyN,
        K::KEY_O => Code::KeyO,
        K::KEY_P => Code::KeyP,
        K::KEY_Q => Code::KeyQ,
        K::KEY_R => Code::KeyR,
        K::KEY_S => Code::KeyS,
        K::KEY_T => Code::KeyT,
        K::KEY_U => Code::KeyU,
        K::KEY_V => Code::KeyV,
        K::KEY_W => Code::KeyW,
        K::KEY_X => Code::KeyX,
        K::KEY_Y => Code::KeyY,
        K::KEY_Z => Code::KeyZ,
        K::KEY_0 => Code::Digit0,
        K::KEY_1 => Code::Digit1,
        K::KEY_2 => Code::Digit2,
        K::KEY_3 => Code::Digit3,
        K::KEY_4 => Code::Digit4,
        K::KEY_5 => Code::Digit5,
        K::KEY_6 => Code::Digit6,
        K::KEY_7 => Code::Digit7,
        K::KEY_8 => Code::Digit8,
        K::KEY_9 => Code::Digit9,
        K::KEY_GRAVE => Code::Backquote,
        K::KEY_BACKSLASH => Code::Backslash,
        K::KEY_LEFTBRACE => Code::BracketLeft,
        K::KEY_RIGHTBRACE => Code::BracketRight,
        K::KEY_COMMA => Code::Comma,
        K::KEY_EQUAL => Code::Equal,
        K::KEY_MINUS => Code::Minus,
        K::KEY_DOT => Code::Period,
        K::KEY_APOSTROPHE => Code::Quote,
        K::KEY_SEMICOLON => Code::Semicolon,
        K::KEY_SLASH => Code::Slash,
        K::KEY_SPACE => Code::Space,
        K::KEY_TAB => Code::Tab,
        K::KEY_ENTER => Code::Enter,
        K::KEY_ESC => Code::Escape,
        K::KEY_BACKSPACE => Code::Backspace,
        K::KEY_DELETE => Code::Delete,
        K::KEY_INSERT => Code::Insert,
        K::KEY_CAPSLOCK => Code::CapsLock,
        K::KEY_DOWN => Code::ArrowDown,
        K::KEY_LEFT => Code::ArrowLeft,
        K::KEY_RIGHT => Code::ArrowRight,
        K::KEY_UP => Code::ArrowUp,
        K::KEY_HOME => Code::Home,
        K::KEY_END => Code::End,
        K::KEY_PAGEUP => Code::PageUp,
        K::KEY_PAGEDOWN => Code::PageDown,
        K::KEY_KP0 => Code::Numpad0,
        K::KEY_KP1 => Code::Numpad1,
        K::KEY_KP2 => Code::Numpad2,
        K::KEY_KP3 => Code::Numpad3,
        K::KEY_KP4 => Code::Numpad4,
        K::KEY_KP5 => Code::Numpad5,
        K::KEY_KP6 => Code::Numpad6,
        K::KEY_KP7 => Code::Numpad7,
        K::KEY_KP8 => Code::Numpad8,
        K::KEY_KP9 => Code::Numpad9,
        K::KEY_KPPLUS => Code::NumpadAdd,
        K::KEY_KPDOT => Code::NumpadDecimal,
        K::KEY_KPSLASH => Code::NumpadDivide,
        K::KEY_KPENTER => Code::NumpadEnter,
        K::KEY_KPEQUAL => Code::NumpadEqual,
        K::KEY_KPASTERISK => Code::NumpadMultiply,
        K::KEY_KPMINUS => Code::NumpadSubtract,
        K::KEY_F1 => Code::F1,
        K::KEY_F2 => Code::F2,
        K::KEY_F3 => Code::F3,
        K::KEY_F4 => Code::F4,
        K::KEY_F5 => Code::F5,
        K::KEY_F6 => Code::F6,
        K::KEY_F7 => Code::F7,
        K::KEY_F8 => Code::F8,
        K::KEY_F9 => Code::F9,
        K::KEY_F10 => Code::F10,
        K::KEY_F11 => Code::F11,
        K::KEY_F12 => Code::F12,
        K::KEY_F13 => Code::F13,
        K::KEY_F14 => Code::F14,
        K::KEY_F15 => Code::F15,
        K::KEY_F16 => Code::F16,
        K::KEY_F17 => Code::F17,
        K::KEY_F18 => Code::F18,
        K::KEY_F19 => Code::F19,
        K::KEY_F20 => Code::F20,
        K::KEY_F21 => Code::F21,
        K::KEY_F22 => Code::F22,
        K::KEY_F23 => Code::F23,
        K::KEY_F24 => Code::F24,
        _ => return None,
    })
}

/// The modifier an X keysym stands for.
fn modifier_for_keysym(keysym: u32) -> Option<SystemKey> {
    match keysym {
        0xffe3 => Some(SystemKey::LeftControl),  // Control_L
        0xffe4 => Some(SystemKey::RightControl), // Control_R
        0xffe1 => Some(SystemKey::LeftShift),    // Shift_L
        0xffe2 => Some(SystemKey::RightShift),   // Shift_R
        // Alt_L, Meta_L
        0xffe9 | 0xffe7 => Some(SystemKey::LeftOption),
        // Alt_R, Meta_R, and AltGr (ISO_Level3_Shift), which layouts put on
        // the right Alt key.
        0xffea | 0xffe8 | 0xfe03 => Some(SystemKey::RightOption),
        0xffeb => Some(SystemKey::LeftCommand),  // Super_L
        0xffec => Some(SystemKey::RightCommand), // Super_R
        _ => None,
    }
}

mod x11 {
    use std::collections::HashMap;
    use std::sync::{Arc, RwLock};

    use x11rb::connection::Connection as _;
    use x11rb::protocol::Event;
    use x11rb::protocol::xinput::{self, ConnectionExt as _};
    use x11rb::protocol::xproto::ConnectionExt as _;
    use x11rb::rust_connection::RustConnection;

    use super::{Backend, SystemKey, modifier_for_keysym, with_keys};

    /// X11 key codes are the kernel's plus this.
    const KEYCODE_OFFSET: u32 = 8;

    #[derive(Clone)]
    pub struct Watcher {
        connection: Arc<RustConnection>,
        mapping: Arc<RwLock<HashMap<u32, SystemKey>>>,
    }

    fn error(context: &str, err: impl std::fmt::Display) -> String {
        format!("{context}: {err}")
    }

    /// The modifier each X key code produces, by what the layout puts on it.
    fn load_mapping(connection: &RustConnection) -> Result<HashMap<u32, SystemKey>, String> {
        let setup = connection.setup();
        let (first, last) = (setup.min_keycode, setup.max_keycode);
        let reply = connection
            .get_keyboard_mapping(first, last - first + 1)
            .map_err(|err| error("Reading the keyboard layout", err))?
            .reply()
            .map_err(|err| error("Reading the keyboard layout", err))?;
        let per_keycode = usize::from(reply.keysyms_per_keycode).max(1);

        Ok(reply
            .keysyms
            .chunks(per_keycode)
            .enumerate()
            .filter_map(|(index, keysyms)| {
                let keycode = u32::from(first) + index as u32;
                modifier_for_keysym(*keysyms.first()?).map(|key| (keycode, key))
            })
            .collect())
    }

    pub fn start() -> Result<Backend, String> {
        let (connection, screen) =
            x11rb::connect(None).map_err(|err| error("Connecting to the X server", err))?;
        let root = connection.setup().roots[screen].root;

        connection
            .xinput_xi_query_version(2, 1)
            .map_err(|err| error("Querying XInput", err))?
            .reply()
            .map_err(|err| error("XInput 2 is unavailable", err))?;
        connection
            .xinput_xi_select_events(
                root,
                &[xinput::EventMask {
                    deviceid: u16::from(bool::from(xinput::Device::ALL_MASTER)),
                    mask: vec![
                        xinput::XIEventMask::RAW_KEY_PRESS | xinput::XIEventMask::RAW_KEY_RELEASE,
                    ],
                }],
            )
            .map_err(|err| error("Selecting key events", err))?
            .check()
            .map_err(|err| error("Selecting key events", err))?;

        let watcher = Watcher {
            mapping: Arc::new(RwLock::new(load_mapping(&connection)?)),
            connection: Arc::new(connection),
        };
        let held = watcher.held_keys();
        with_keys(|keys, events| keys.reset(held, events));

        let watching = watcher.clone();
        std::thread::Builder::new()
            .name("x11-key-watcher".into())
            .spawn(move || watch(watching))
            .map_err(|err| error("Starting the key watcher", err))?;

        Ok(Backend::X11(watcher))
    }

    impl Watcher {
        pub fn held_keys(&self) -> Vec<(u32, Option<SystemKey>)> {
            let Ok(reply) = self
                .connection
                .query_keymap()
                .map_err(|err| err.to_string())
                .and_then(|cookie| cookie.reply().map_err(|err| err.to_string()))
            else {
                return Vec::new();
            };
            let mapping = self.mapping.read().unwrap_or_else(|poisoned| poisoned.into_inner());

            (0..256u32)
                .filter(|keycode| reply.keys[*keycode as usize / 8] & (1 << (keycode % 8)) != 0)
                .filter(|keycode| *keycode >= KEYCODE_OFFSET)
                .map(|keycode| (keycode - KEYCODE_OFFSET, mapping.get(&keycode).copied()))
                .collect()
        }
    }

    fn watch(watcher: Watcher) {
        let Watcher {
            connection,
            mapping,
        } = &watcher;
        let modifier = |keycode: u32| {
            mapping
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .get(&keycode)
                .copied()
        };

        loop {
            let event = match connection.wait_for_event() {
                Ok(event) => event,
                Err(err) => {
                    log::error!(target: "hotkey", "system_key_watcher_stopped error={err}");
                    return;
                }
            };

            match event {
                Event::XinputRawKeyPress(event) => {
                    if u32::from(event.flags) & u32::from(xinput::KeyEventFlags::KEY_REPEAT) != 0 {
                        continue;
                    }
                    let Some(code) = event.detail.checked_sub(KEYCODE_OFFSET) else {
                        continue;
                    };
                    let key = modifier(event.detail);
                    with_keys(|keys, events| keys.press(code, key, events));
                }
                Event::XinputRawKeyRelease(event) => {
                    let Some(code) = event.detail.checked_sub(KEYCODE_OFFSET) else {
                        continue;
                    };
                    let key = modifier(event.detail);
                    with_keys(|keys, events| {
                        if !keys.release(code, key, events) {
                            let held = watcher.held_keys();
                            log::warn!(
                                target: "hotkey",
                                "system_key_state_resynchronized event_key={key:?}"
                            );
                            keys.reset(held, events);
                        }
                    });
                }
                Event::MappingNotify(_) => match load_mapping(connection) {
                    Ok(loaded) => {
                        *mapping.write().unwrap_or_else(|poisoned| poisoned.into_inner()) = loaded;
                    }
                    Err(err) => log::warn!(target: "hotkey", "keyboard_layout_reload_failed error={err}"),
                },
                _ => {}
            }
        }
    }
}

/// The keyboard devices, read directly. Each is read on a thread of its own;
/// keyboards plugged in later are found by looking again every few seconds.
mod devices {
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use evdev::{Device, EventSummary, KeyCode};

    use super::{Backend, NO_KEYBOARD_ACCESS, SHORTCUTS, SystemKey, modifier_for_code, with_keys};

    /// How often new keyboards are looked for.
    const RESCAN_INTERVAL: Duration = Duration::from_secs(2);

    #[derive(Clone, Default)]
    pub struct Watcher {
        /// The devices being read.
        watched: Arc<Mutex<HashSet<PathBuf>>>,
    }

    fn event_nodes() -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir("/dev/input") else {
            return Vec::new();
        };
        let mut nodes: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("event"))
            })
            .collect();
        nodes.sort();
        nodes
    }

    /// Keyboards have letters or modifiers; power buttons and mice do not.
    fn is_keyboard(device: &Device) -> bool {
        device.supported_keys().is_some_and(|keys| {
            [
                KeyCode::KEY_A,
                KeyCode::KEY_LEFTCTRL,
                KeyCode::KEY_RIGHTCTRL,
                KeyCode::KEY_LEFTMETA,
            ]
            .into_iter()
            .any(|key| keys.contains(key))
        })
    }

    /// Whether a keyboard device can be read.
    pub fn any_keyboard_readable() -> bool {
        event_nodes()
            .iter()
            .any(|node| Device::open(node).is_ok_and(|device| is_keyboard(&device)))
    }

    impl Watcher {
        fn lock(&self) -> std::sync::MutexGuard<'_, HashSet<PathBuf>> {
            self.watched.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
        }

        /// Starts reading the keyboards not read yet. Returns how many there
        /// are now, and whether any device could not be opened.
        fn watch_new(&self) -> (usize, bool) {
            let mut refused = false;
            for node in event_nodes() {
                if self.lock().contains(&node) {
                    continue;
                }
                let device = match Device::open(&node) {
                    Ok(device) => device,
                    Err(err) => {
                        refused |= err.kind() == std::io::ErrorKind::PermissionDenied;
                        continue;
                    }
                };
                if !is_keyboard(&device) {
                    continue;
                }

                log::info!(
                    target: "hotkey",
                    "keyboard_watched device={:?} name={:?}",
                    node,
                    device.name().unwrap_or_default()
                );
                self.lock().insert(node.clone());
                let watcher = self.clone();
                let spawned = std::thread::Builder::new()
                    .name("keyboard-watcher".into())
                    .spawn(move || watcher.read(&node, device));
                if let Err(err) = spawned {
                    log::warn!(target: "hotkey", "keyboard_watch_failed error={err}");
                }
            }
            (self.lock().len(), refused)
        }

        fn read(&self, node: &Path, mut device: Device) {
            loop {
                let events = match device.fetch_events() {
                    Ok(events) => events,
                    Err(err) => {
                        // Unplugged: its keys are no longer held.
                        log::info!(target: "hotkey", "keyboard_gone device={node:?} error={err}");
                        self.lock().remove(node);
                        let held = self.held_keys();
                        with_keys(|keys, events| keys.reset(held, events));
                        return;
                    }
                };
                for event in events {
                    let EventSummary::Key(_, key, value) = event.destructure() else {
                        continue;
                    };
                    let code = u32::from(key.code());
                    let modifier = modifier_for_code(code);
                    match value {
                        1 => with_keys(|keys, events| {
                            keys.press(code, modifier, events);
                            if modifier.is_none() {
                                let modifiers = keys.combination_modifiers();
                                SHORTCUTS
                                    .lock()
                                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                                    .key_pressed(code, modifiers, events);
                            }
                        }),
                        0 => with_keys(|keys, events| {
                            if !keys.release(code, modifier, events) {
                                let held = self.held_keys();
                                keys.reset(held, events);
                            }
                            SHORTCUTS
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner())
                                .key_released(code, events);
                        }),
                        // Autorepeat.
                        _ => {}
                    }
                }
            }
        }

        /// The keys held on every keyboard read.
        pub fn held_keys(&self) -> Vec<(u32, Option<SystemKey>)> {
            let nodes: Vec<PathBuf> = self.lock().iter().cloned().collect();
            let mut held = Vec::new();
            for node in nodes {
                let Ok(state) = Device::open(&node).and_then(|device| device.get_key_state()) else {
                    continue;
                };
                for key in state.iter() {
                    let code = u32::from(key.code());
                    held.push((code, modifier_for_code(code)));
                }
            }
            held
        }
    }

    pub fn start() -> Result<Backend, String> {
        let watcher = Watcher::default();
        let (keyboards, refused) = watcher.watch_new();
        if keyboards == 0 {
            return Err(if refused {
                NO_KEYBOARD_ACCESS.to_string()
            } else {
                "No keyboard was found".to_string()
            });
        }
        let held = watcher.held_keys();
        with_keys(|keys, events| keys.reset(held, events));

        let rescanning = watcher.clone();
        std::thread::Builder::new()
            .name("keyboard-scan".into())
            .spawn(move || {
                loop {
                    std::thread::sleep(RESCAN_INTERVAL);
                    rescanning.watch_new();
                }
            })
            .map_err(|err| format!("Starting the keyboard scan: {err}"))?;

        Ok(Backend::Devices(watcher))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEFT_CONTROL: u32 = 29;
    const LEFT_ALT: u32 = 56;
    const RIGHT_META: u32 = 126;
    const K: u32 = 37;

    fn sender() -> (EventSender, async_channel::Receiver<AppEvent>) {
        let (sender, receiver) = async_channel::unbounded();
        (EventSender(sender), receiver)
    }

    fn drain(receiver: &async_channel::Receiver<AppEvent>) -> Vec<AppEvent> {
        std::iter::from_fn(|| receiver.try_recv().ok()).collect()
    }

    #[test]
    fn a_chord_reports_each_modifier_with_everything_held() {
        let (events, received) = sender();
        let mut keys = Keys::new();

        keys.press(LEFT_CONTROL, Some(SystemKey::LeftControl), &events);
        keys.press(LEFT_ALT, Some(SystemKey::LeftOption), &events);
        keys.release(LEFT_ALT, Some(SystemKey::LeftOption), &events);
        keys.release(LEFT_CONTROL, Some(SystemKey::LeftControl), &events);

        assert_eq!(
            drain(&received),
            vec![
                AppEvent::SystemKeyPressed {
                    key: SystemKey::LeftControl,
                    pressed_keys: vec![SystemKey::LeftControl],
                },
                AppEvent::SystemKeyPressed {
                    key: SystemKey::LeftOption,
                    pressed_keys: vec![SystemKey::LeftControl, SystemKey::LeftOption],
                },
                AppEvent::SystemKeyReleased {
                    key: SystemKey::LeftOption,
                    pressed_keys: vec![SystemKey::LeftControl],
                },
                AppEvent::SystemKeyReleased {
                    key: SystemKey::LeftControl,
                    pressed_keys: vec![],
                },
                AppEvent::SystemKeysReleased,
            ]
        );
    }

    #[test]
    fn autorepeat_is_reported_once() {
        let (events, received) = sender();
        let mut keys = Keys::new();

        keys.press(RIGHT_META, Some(SystemKey::RightCommand), &events);
        keys.press(RIGHT_META, Some(SystemKey::RightCommand), &events);
        keys.press(K, None, &events);
        keys.press(K, None, &events);

        assert_eq!(
            drain(&received),
            vec![
                AppEvent::SystemKeyPressed {
                    key: SystemKey::RightCommand,
                    pressed_keys: vec![SystemKey::RightCommand],
                },
                AppEvent::KeyboardKeyPressed {
                    key_code: i64::from(K),
                },
            ]
        );
    }

    #[test]
    fn a_modifier_held_by_two_keys_is_released_with_the_last() {
        let (events, received) = sender();
        let mut keys = Keys::new();

        // Alt_L and Meta_L on two keys of one layout.
        keys.press(LEFT_ALT, Some(SystemKey::LeftOption), &events);
        keys.press(205, Some(SystemKey::LeftOption), &events);
        keys.release(LEFT_ALT, Some(SystemKey::LeftOption), &events);
        assert_eq!(drain(&received).len(), 1);

        keys.release(205, Some(SystemKey::LeftOption), &events);
        assert_eq!(
            drain(&received),
            vec![
                AppEvent::SystemKeyReleased {
                    key: SystemKey::LeftOption,
                    pressed_keys: vec![],
                },
                AppEvent::SystemKeysReleased,
            ]
        );
    }

    #[test]
    fn a_release_without_its_press_asks_for_a_resynchronization() {
        let (events, received) = sender();
        let mut keys = Keys::new();

        assert!(!keys.release(LEFT_CONTROL, Some(SystemKey::LeftControl), &events));
        assert!(keys.release(K, None, &events));
        assert!(drain(&received).is_empty());
    }

    #[test]
    fn a_reset_takes_the_held_keys_and_releases_every_hotkey() {
        let (events, received) = sender();
        let mut keys = Keys::new();
        keys.press(LEFT_CONTROL, Some(SystemKey::LeftControl), &events);
        drain(&received);

        keys.reset(vec![(RIGHT_META, Some(SystemKey::RightCommand))], &events);

        assert_eq!(drain(&received), vec![AppEvent::SystemKeysReleased]);
        assert_eq!(keys.pressed_modifiers(), vec![SystemKey::RightCommand]);
        // The key held through the reset is not pressed again by a repeat.
        keys.press(RIGHT_META, Some(SystemKey::RightCommand), &events);
        assert!(drain(&received).is_empty());
    }

    #[test]
    fn combinations_match_whichever_side_the_modifiers_are_on() {
        let (events, received) = sender();
        let hotkey: HotKey = "Control+Shift+K".parse().unwrap();
        let mut shortcuts = Shortcuts::new();
        shortcuts
            .registered
            .push((hotkey, "Control+Shift+K".to_string()));
        let mut keys = Keys::new();
        keys.press(97, Some(SystemKey::RightControl), &events);
        keys.press(42, Some(SystemKey::LeftShift), &events);
        drain(&received);

        shortcuts.key_pressed(K, keys.combination_modifiers(), &events);
        shortcuts.key_released(K, &events);

        assert_eq!(
            drain(&received),
            vec![
                AppEvent::GlobalShortcut {
                    shortcut: "Control+Shift+K".into(),
                    state: ShortcutState::Pressed,
                },
                AppEvent::GlobalShortcut {
                    shortcut: "Control+Shift+K".into(),
                    state: ShortcutState::Released,
                },
            ]
        );
    }

    #[test]
    fn a_combination_needs_exactly_its_modifiers() {
        let (events, received) = sender();
        let mut shortcuts = Shortcuts::new();
        shortcuts
            .registered
            .push(("Control+K".parse().unwrap(), "Control+K".to_string()));

        shortcuts.key_pressed(K, Modifiers::CONTROL | Modifiers::SHIFT, &events);
        shortcuts.key_pressed(K, Modifiers::empty(), &events);
        shortcuts.key_released(K, &events);

        assert!(drain(&received).is_empty());
    }

    #[test]
    fn every_key_the_recorder_writes_has_a_kernel_code() {
        let names = [
            "A", "Z", "0", "9", "`", "\\", "[", "]", ",", "=", "-", ".", "'", ";", "/", "Space",
            "Tab", "Enter", "Escape", "Backspace", "Delete", "CapsLock", "ArrowDown", "ArrowLeft",
            "ArrowRight", "ArrowUp", "Home", "End", "PageUp", "PageDown", "Numpad0", "NumpadAdd",
            "NumpadEnter", "F1", "F12", "F13", "F24",
        ];
        let codes: Vec<Code> = (0..256).filter_map(combination_key).collect();
        for name in names {
            let hotkey: HotKey = format!("Control+{name}").parse().unwrap();
            assert!(codes.contains(&hotkey.key), "{name}");
        }
    }

    #[test]
    fn layouts_name_the_modifiers() {
        assert_eq!(modifier_for_keysym(0xffe3), Some(SystemKey::LeftControl));
        assert_eq!(modifier_for_keysym(0xfe03), Some(SystemKey::RightOption));
        assert_eq!(modifier_for_keysym(0xffec), Some(SystemKey::RightCommand));
        assert_eq!(modifier_for_keysym(u32::from(b'k')), None);
    }
}
