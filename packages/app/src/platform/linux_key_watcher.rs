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
//! from by 8. A combination's key is matched by what the layout puts on it,
//! as the hotkey recorder names it, read from XWayland's keymap on Wayland.

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

    let mut watcher = WATCHER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
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
    let watcher = WATCHER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(backend) = watcher.as_ref() else {
        return;
    };
    let current = backend.held_keys();
    drop(watcher);
    log::info!(target: "hotkey", "system_key_state_lifecycle_resynchronized reason={reason}");
    with_keys(|keys, events| reset_keys(keys, current, events));
}

/// Has `hotkey` reported as `name` while the keyboard devices are watched,
/// where no global shortcut can be registered with the system.
pub fn register_shortcut(hotkey: HotKey, name: &str) {
    let mut shortcuts = SHORTCUTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    shortcuts
        .registered
        .retain(|(registered, _)| registered.id() != hotkey.id());
    shortcuts.registered.push((hotkey, name.to_string()));
}

pub fn unregister_shortcut(hotkey: &HotKey) {
    let mut shortcuts = SHORTCUTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    shortcuts
        .registered
        .retain(|(registered, _)| registered.id() != hotkey.id());
}

/// A key on one keyboard: the keyboard, then the key's kernel code. X11
/// merges every keyboard into one, numbered 0.
type KeyId = (usize, u32);

/// Keys held down, with the modifier each is.
type HeldKeys = Vec<(KeyId, Option<SystemKey>)>;

/// A running watcher.
enum Backend {
    X11(x11::Watcher),
    Devices(devices::Watcher),
}

impl Backend {
    /// The keys held right now, as the system reports them.
    fn held_keys(&self) -> HeldKeys {
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

fn shortcuts() -> std::sync::MutexGuard<'static, Shortcuts> {
    SHORTCUTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Takes the keys held as the system reports them: releases a shortcut whose
/// key is no longer down, and has every modifier-only hotkey start over.
fn reset_keys(keys: &mut Keys, held: HeldKeys, events: &EventSender) {
    shortcuts().release_unless_held(&held, events);
    keys.reset(held, events);
}

/// The keys held down, as followed through their presses and releases.
#[derive(Debug, Default, PartialEq)]
struct Keys {
    /// The modifiers held, by the key holding each.
    modifiers: BTreeMap<KeyId, SystemKey>,
    /// Other keys held, so that their autorepeat is not reported again.
    others: BTreeSet<KeyId>,
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

    fn press(&mut self, id: KeyId, key: Option<SystemKey>, events: &EventSender) {
        let Some(key) = key else {
            if self.others.insert(id) {
                events.emit(AppEvent::KeyboardKeyPressed {
                    key_code: i64::from(id.1),
                });
            }
            return;
        };

        let before = self.pressed_modifiers();
        if self.modifiers.insert(id, key).is_some() {
            // Held already: an autorepeat.
            return;
        }
        let pressed_keys = self.pressed_modifiers();
        if pressed_keys != before {
            events.emit(AppEvent::SystemKeyPressed { key, pressed_keys });
        }
    }

    /// `key` is the modifier the key is now, which matters only when its
    /// press went unobserved: a key is released as what it was pressed as,
    /// even if the layout changed in between. Returns false when the release
    /// does not follow from the keys known to be held.
    fn release(&mut self, id: KeyId, key: Option<SystemKey>, events: &EventSender) -> bool {
        let Some(key) = self.modifiers.remove(&id) else {
            let was_held = self.others.remove(&id);
            return was_held || key.is_none();
        };

        let pressed_keys = self.pressed_modifiers();
        if pressed_keys.contains(&key) {
            // Another key of that modifier is still down.
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
    fn reset(&mut self, held: HeldKeys, events: &EventSender) {
        *self = Self::new();
        for (id, key) in held {
            match key {
                Some(key) => {
                    self.modifiers.insert(id, key);
                }
                None => {
                    self.others.insert(id);
                }
            }
        }
        events.emit(AppEvent::SystemKeysReleased);
    }
}

/// The shortcuts matched against the keyboard devices.
struct Shortcuts {
    registered: Vec<(HotKey, String)>,
    /// The keys that completed shortcuts, with the shortcut each completed,
    /// until they are released.
    held: Vec<(KeyId, String)>,
}

impl Shortcuts {
    const fn new() -> Self {
        Self {
            registered: Vec::new(),
            held: Vec::new(),
        }
    }

    /// `key` is what the layout puts on the key `id` pressed.
    fn key_pressed(
        &mut self,
        id: KeyId,
        key: Option<Code>,
        modifiers: Modifiers,
        events: &EventSender,
    ) {
        let Some(key) = key else {
            return;
        };
        let Some((_, name)) = self
            .registered
            .iter()
            .find(|(hotkey, _)| hotkey.key == key && hotkey.mods == modifiers)
        else {
            return;
        };
        self.held.push((id, name.clone()));
        events.emit(AppEvent::GlobalShortcut {
            shortcut: name.clone(),
            state: ShortcutState::Pressed,
        });
    }

    fn key_released(&mut self, id: KeyId, events: &EventSender) {
        self.release_where(|held| *held == id, events);
    }

    /// Releases the held shortcuts whose keys are not among `held`, as when
    /// a keyboard was unplugged without reporting their release.
    fn release_unless_held(&mut self, held: &[(KeyId, Option<SystemKey>)], events: &EventSender) {
        self.release_where(|id| !held.iter().any(|(down, _)| down == id), events);
    }

    fn release_where(&mut self, released: impl Fn(&KeyId) -> bool, events: &EventSender) {
        let (gone, still_held) = std::mem::take(&mut self.held)
            .into_iter()
            .partition(|(id, _)| released(id));
        self.held = still_held;
        for (_, shortcut) in gone {
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

/// The key a shortcut names for the key with this kernel code, which the
/// layout puts `keysym` on. The recorder names a key by the character it
/// types, so the layout decides; keys that type no character it names, such
/// as the keypad's, are taken by their place on a US keyboard.
fn combination_key(code: u32, keysym: Option<u32>) -> Option<Code> {
    keysym
        .and_then(key_for_keysym)
        .or_else(|| physical_key(code))
}

/// The key a shortcut names for a keysym: every key the hotkey recorder
/// writes that a layout moves around.
fn key_for_keysym(keysym: u32) -> Option<Code> {
    const LETTERS: [Code; 26] = [
        Code::KeyA,
        Code::KeyB,
        Code::KeyC,
        Code::KeyD,
        Code::KeyE,
        Code::KeyF,
        Code::KeyG,
        Code::KeyH,
        Code::KeyI,
        Code::KeyJ,
        Code::KeyK,
        Code::KeyL,
        Code::KeyM,
        Code::KeyN,
        Code::KeyO,
        Code::KeyP,
        Code::KeyQ,
        Code::KeyR,
        Code::KeyS,
        Code::KeyT,
        Code::KeyU,
        Code::KeyV,
        Code::KeyW,
        Code::KeyX,
        Code::KeyY,
        Code::KeyZ,
    ];
    const DIGITS: [Code; 10] = [
        Code::Digit0,
        Code::Digit1,
        Code::Digit2,
        Code::Digit3,
        Code::Digit4,
        Code::Digit5,
        Code::Digit6,
        Code::Digit7,
        Code::Digit8,
        Code::Digit9,
    ];

    let character = char::from_u32(keysym).filter(|_| keysym < 0x100)?;
    if character.is_ascii_alphabetic() {
        return Some(LETTERS[usize::from(character.to_ascii_uppercase() as u8 - b'A')]);
    }
    if let Some(digit) = character.to_digit(10) {
        return Some(DIGITS[digit as usize]);
    }
    Some(match character {
        '`' => Code::Backquote,
        '\\' => Code::Backslash,
        '[' => Code::BracketLeft,
        ']' => Code::BracketRight,
        ',' => Code::Comma,
        '=' => Code::Equal,
        '-' => Code::Minus,
        '.' => Code::Period,
        '\'' => Code::Quote,
        ';' => Code::Semicolon,
        '/' => Code::Slash,
        _ => return None,
    })
}

/// The key a shortcut names for a kernel key code, by its place on a US
/// keyboard: every key the hotkey recorder can write.
fn physical_key(code: u32) -> Option<Code> {
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

    use super::{Backend, HeldKeys, SystemKey, modifier_for_keysym, reset_keys, with_keys};

    /// X11 key codes are the kernel's plus this.
    pub(super) const KEYCODE_OFFSET: u32 = 8;

    #[derive(Clone)]
    pub struct Watcher {
        connection: Arc<RustConnection>,
        mapping: Arc<RwLock<HashMap<u32, SystemKey>>>,
    }

    fn error(context: &str, err: impl std::fmt::Display) -> String {
        format!("{context}: {err}")
    }

    /// The keysym the layout puts on each X key code, without Shift.
    fn layout_keysyms(connection: &RustConnection) -> Result<HashMap<u32, u32>, String> {
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
                let keysym = *keysyms.first()?;
                (keysym != 0).then(|| (u32::from(first) + index as u32, keysym))
            })
            .collect())
    }

    /// The modifier each X key code produces, by what the layout puts on it.
    fn load_mapping(connection: &RustConnection) -> Result<HashMap<u32, SystemKey>, String> {
        Ok(layout_keysyms(connection)?
            .into_iter()
            .filter_map(|(keycode, keysym)| modifier_for_keysym(keysym).map(|key| (keycode, key)))
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
        with_keys(|keys, events| reset_keys(keys, held, events));

        let watching = watcher.clone();
        std::thread::Builder::new()
            .name("x11-key-watcher".into())
            .spawn(move || watch(watching))
            .map_err(|err| error("Starting the key watcher", err))?;

        Ok(Backend::X11(watcher))
    }

    impl Watcher {
        pub fn held_keys(&self) -> HeldKeys {
            let Ok(reply) = self
                .connection
                .query_keymap()
                .map_err(|err| err.to_string())
                .and_then(|cookie| cookie.reply().map_err(|err| err.to_string()))
            else {
                return Vec::new();
            };
            let mapping = self
                .mapping
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());

            (0..256u32)
                .filter(|keycode| reply.keys[*keycode as usize / 8] & (1 << (keycode % 8)) != 0)
                .filter(|keycode| *keycode >= KEYCODE_OFFSET)
                .map(|keycode| {
                    (
                        (0, keycode - KEYCODE_OFFSET),
                        mapping.get(&keycode).copied(),
                    )
                })
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
                    with_keys(|keys, events| keys.press((0, code), key, events));
                }
                Event::XinputRawKeyRelease(event) => {
                    let Some(code) = event.detail.checked_sub(KEYCODE_OFFSET) else {
                        continue;
                    };
                    let key = modifier(event.detail);
                    with_keys(|keys, events| {
                        if !keys.release((0, code), key, events) {
                            let held = watcher.held_keys();
                            log::warn!(
                                target: "hotkey",
                                "system_key_state_resynchronized event_key={key:?}"
                            );
                            reset_keys(keys, held, events);
                        }
                    });
                }
                Event::MappingNotify(_) => match load_mapping(connection) {
                    Ok(loaded) => {
                        *mapping
                            .write()
                            .unwrap_or_else(|poisoned| poisoned.into_inner()) = loaded;
                    }
                    Err(err) => {
                        log::warn!(target: "hotkey", "keyboard_layout_reload_failed error={err}")
                    }
                },
                _ => {}
            }
        }
    }
}

/// What the keyboard layout puts on each key, read from XWayland, which
/// follows the compositor's keymap, in whichever of its layouts (XKB groups)
/// is active. Without XWayland keys are matched by their place on a US
/// keyboard.
mod layout {
    use std::collections::HashMap;
    use std::sync::RwLock;

    use x11rb::connection::Connection as _;
    use x11rb::protocol::Event;
    use x11rb::protocol::xkb::{self, ConnectionExt as _};
    use x11rb::rust_connection::RustConnection;

    use super::x11::KEYCODE_OFFSET;

    struct Layout {
        /// By X key code, the keysym each group puts on the key, unshifted.
        keys: HashMap<u32, Vec<u32>>,
        /// The active group.
        group: usize,
    }

    static LAYOUT: RwLock<Option<Layout>> = RwLock::new(None);

    const CORE_KEYBOARD: xkb::DeviceSpec = 0x100;

    fn error(context: &str, err: impl std::fmt::Display) -> String {
        format!("{context}: {err}")
    }

    fn read_keys(connection: &RustConnection) -> Result<HashMap<u32, Vec<u32>>, String> {
        let setup = connection.setup();
        let (first, last) = (setup.min_keycode, setup.max_keycode);
        let none = xkb::MapPart::from(0u16);
        let reply = connection
            .xkb_get_map(
                CORE_KEYBOARD,
                xkb::MapPart::KEY_SYMS,
                none,
                0,
                0,
                first,
                last - first + 1,
                0,
                0,
                0,
                0,
                xkb::VMod::from(0u16),
                0,
                0,
                0,
                0,
                0,
                0,
            )
            .map_err(|err| error("Reading the keyboard layout", err))?
            .reply()
            .map_err(|err| error("Reading the keyboard layout", err))?;

        Ok(reply
            .map
            .syms_rtrn
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(index, key)| {
                let width = usize::from(key.width).max(1);
                let groups = usize::from(key.group_info & 0x0f);
                let firsts = (0..groups)
                    .filter_map(|group| key.syms.get(group * width).copied())
                    .collect();
                (u32::from(first) + index as u32, firsts)
            })
            .collect())
    }

    fn read_group(connection: &RustConnection) -> Result<usize, String> {
        let state = connection
            .xkb_get_state(CORE_KEYBOARD)
            .map_err(|err| error("Reading the keyboard state", err))?
            .reply()
            .map_err(|err| error("Reading the keyboard state", err))?;
        Ok(usize::from(u8::from(state.group)))
    }

    fn read(connection: &RustConnection) {
        match read_keys(connection).and_then(|keys| Ok((keys, read_group(connection)?))) {
            Ok((keys, group)) => {
                log::info!(target: "hotkey", "keyboard_layout_read keys={} group={group}", keys.len());
                *LAYOUT
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                    Some(Layout { keys, group });
            }
            Err(err) => log::warn!(target: "hotkey", "keyboard_layout_read_failed error={err}"),
        }
    }

    fn set_group(group: usize) {
        if let Some(layout) = LAYOUT
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_mut()
        {
            layout.group = group;
        }
    }

    /// Reads the layout, and again whenever it or the active group changes.
    pub fn follow() {
        let Ok((connection, _)) = x11rb::connect(None) else {
            log::info!(target: "hotkey", "keyboard_layout_unavailable");
            return;
        };
        let supported = connection
            .xkb_use_extension(1, 0)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .is_some_and(|reply| reply.supported);
        let selected = supported
            && connection
                .xkb_select_events(
                    CORE_KEYBOARD,
                    xkb::EventType::from(0u16),
                    xkb::EventType::NEW_KEYBOARD_NOTIFY | xkb::EventType::MAP_NOTIFY,
                    xkb::MapPart::KEY_SYMS,
                    xkb::MapPart::KEY_SYMS,
                    &xkb::SelectEventsAux {
                        state_notify: Some(xkb::SelectEventsAuxStateNotify {
                            affect_state: xkb::StatePart::GROUP_STATE,
                            state_details: xkb::StatePart::GROUP_STATE,
                        }),
                        ..Default::default()
                    },
                )
                .ok()
                .and_then(|cookie| cookie.check().ok())
                .is_some();
        if !selected {
            log::info!(target: "hotkey", "keyboard_layout_unavailable");
            return;
        }
        read(&connection);

        let spawned = std::thread::Builder::new()
            .name("keyboard-layout".into())
            .spawn(move || {
                while let Ok(event) = connection.wait_for_event() {
                    match event {
                        Event::XkbStateNotify(state) => {
                            set_group(usize::from(u8::from(state.group)))
                        }
                        Event::XkbMapNotify(_)
                        | Event::XkbNewKeyboardNotify(_)
                        | Event::MappingNotify(_) => read(&connection),
                        _ => {}
                    }
                }
            });
        if let Err(err) = spawned {
            log::warn!(target: "hotkey", "keyboard_layout_watch_failed error={err}");
        }
    }

    /// The keysym on the key with this kernel code in the active layout.
    pub fn keysym(code: u32) -> Option<u32> {
        let layout = LAYOUT
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let layout = layout.as_ref()?;
        let groups = layout.keys.get(&(code + KEYCODE_OFFSET))?;
        // A key with fewer groups wraps the active one around them.
        let keysym = *groups.get(layout.group % groups.len().max(1))?;
        (keysym != 0).then_some(keysym)
    }
}

/// The keyboard devices, read directly. Each is read on a thread of its own;
/// keyboards plugged in later are found by looking again every few seconds.
mod devices {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use evdev::{Device, EventSummary, KeyCode};

    use super::{
        Backend, HeldKeys, KeyId, NO_KEYBOARD_ACCESS, combination_key, layout, modifier_for_code,
        reset_keys, shortcuts, with_keys,
    };

    /// How often new keyboards are looked for.
    const RESCAN_INTERVAL: Duration = Duration::from_secs(2);

    #[derive(Clone, Default)]
    pub struct Watcher {
        /// The devices being read, with the number each keyboard was given.
        watched: Arc<Mutex<HashMap<PathBuf, usize>>>,
        /// The number the next keyboard gets. X11's keyboard is 0.
        last_keyboard: Arc<AtomicUsize>,
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
        fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<PathBuf, usize>> {
            self.watched
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
        }

        /// Starts reading the keyboards not read yet. Returns how many there
        /// are now, and whether any device could not be opened.
        fn watch_new(&self) -> (usize, bool) {
            let mut refused = false;
            for node in event_nodes() {
                if self.lock().contains_key(&node) {
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

                let keyboard = self.last_keyboard.fetch_add(1, Ordering::Relaxed) + 1;
                log::info!(
                    target: "hotkey",
                    "keyboard_watched device={:?} keyboard={keyboard} name={:?}",
                    node,
                    device.name().unwrap_or_default()
                );
                self.lock().insert(node.clone(), keyboard);
                let watcher = self.clone();
                let spawned = std::thread::Builder::new()
                    .name("keyboard-watcher".into())
                    .spawn(move || watcher.read(&node, keyboard, device));
                if let Err(err) = spawned {
                    log::warn!(target: "hotkey", "keyboard_watch_failed error={err}");
                }
            }
            (self.lock().len(), refused)
        }

        fn read(&self, node: &Path, keyboard: usize, mut device: Device) {
            loop {
                let events = match device.fetch_events() {
                    Ok(events) => events,
                    Err(err) => {
                        // Unplugged: its keys are no longer held.
                        log::info!(target: "hotkey", "keyboard_gone device={node:?} error={err}");
                        self.lock().remove(node);
                        let held = self.held_keys();
                        with_keys(|keys, events| reset_keys(keys, held, events));
                        return;
                    }
                };
                for event in events {
                    let EventSummary::Key(_, key, value) = event.destructure() else {
                        continue;
                    };
                    let code = u32::from(key.code());
                    let id: KeyId = (keyboard, code);
                    let modifier = modifier_for_code(code);
                    match value {
                        1 => with_keys(|keys, events| {
                            keys.press(id, modifier, events);
                            if modifier.is_none() {
                                let key = combination_key(code, layout::keysym(code));
                                let modifiers = keys.combination_modifiers();
                                shortcuts().key_pressed(id, key, modifiers, events);
                            }
                        }),
                        0 => with_keys(|keys, events| {
                            shortcuts().key_released(id, events);
                            if !keys.release(id, modifier, events) {
                                let held = self.held_keys();
                                reset_keys(keys, held, events);
                            }
                        }),
                        // Autorepeat.
                        _ => {}
                    }
                }
            }
        }

        /// The keys held on every keyboard read.
        pub fn held_keys(&self) -> HeldKeys {
            let watched: Vec<(PathBuf, usize)> = self
                .lock()
                .iter()
                .map(|(node, keyboard)| (node.clone(), *keyboard))
                .collect();
            let mut held = Vec::new();
            for (node, keyboard) in watched {
                let Ok(state) = Device::open(&node).and_then(|device| device.get_key_state())
                else {
                    continue;
                };
                for key in state.iter() {
                    let code = u32::from(key.code());
                    held.push(((keyboard, code), modifier_for_code(code)));
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
        with_keys(|keys, events| reset_keys(keys, held, events));
        layout::follow();

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

    const LEFT_CONTROL: KeyId = (0, 29);
    const LEFT_ALT: KeyId = (0, 56);
    const RIGHT_META: KeyId = (0, 126);
    const K: KeyId = (0, 37);

    fn sender() -> (EventSender, async_channel::Receiver<AppEvent>) {
        let (sender, receiver) = async_channel::unbounded();
        (EventSender(sender), receiver)
    }

    fn drain(receiver: &async_channel::Receiver<AppEvent>) -> Vec<AppEvent> {
        std::iter::from_fn(|| receiver.try_recv().ok()).collect()
    }

    fn shortcut(name: &str, state: ShortcutState) -> AppEvent {
        AppEvent::GlobalShortcut {
            shortcut: name.into(),
            state,
        }
    }

    fn registered(name: &str) -> Shortcuts {
        let mut shortcuts = Shortcuts::new();
        shortcuts
            .registered
            .push((name.parse().unwrap(), name.to_string()));
        shortcuts
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
                AppEvent::KeyboardKeyPressed { key_code: 37 },
            ]
        );
    }

    #[test]
    fn a_modifier_held_by_two_keys_is_released_with_the_last() {
        let (events, received) = sender();
        let mut keys = Keys::new();

        // Alt_L and Meta_L on two keys of one layout.
        keys.press(LEFT_ALT, Some(SystemKey::LeftOption), &events);
        keys.press((0, 205), Some(SystemKey::LeftOption), &events);
        keys.release(LEFT_ALT, Some(SystemKey::LeftOption), &events);
        assert_eq!(drain(&received).len(), 1);

        keys.release((0, 205), Some(SystemKey::LeftOption), &events);
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
    fn the_same_key_on_another_keyboard_does_not_release_the_first() {
        let (events, received) = sender();
        let mut keys = Keys::new();
        let (laptop, external) = ((1, 97), (2, 97));

        keys.press(laptop, Some(SystemKey::RightControl), &events);
        drain(&received);
        keys.press(external, Some(SystemKey::RightControl), &events);
        keys.release(external, Some(SystemKey::RightControl), &events);
        assert!(drain(&received).is_empty());
        assert_eq!(keys.pressed_modifiers(), vec![SystemKey::RightControl]);

        keys.release(laptop, Some(SystemKey::RightControl), &events);
        assert_eq!(
            drain(&received),
            vec![
                AppEvent::SystemKeyReleased {
                    key: SystemKey::RightControl,
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
        let mut shortcuts = registered("Control+Shift+K");
        let mut keys = Keys::new();
        keys.press((0, 97), Some(SystemKey::RightControl), &events);
        keys.press((0, 42), Some(SystemKey::LeftShift), &events);
        drain(&received);

        shortcuts.key_pressed(K, Some(Code::KeyK), keys.combination_modifiers(), &events);
        shortcuts.key_released(K, &events);

        assert_eq!(
            drain(&received),
            vec![
                shortcut("Control+Shift+K", ShortcutState::Pressed),
                shortcut("Control+Shift+K", ShortcutState::Released),
            ]
        );
    }

    #[test]
    fn a_combination_needs_exactly_its_modifiers() {
        let (events, received) = sender();
        let mut shortcuts = registered("Control+K");

        shortcuts.key_pressed(
            K,
            Some(Code::KeyK),
            Modifiers::CONTROL | Modifiers::SHIFT,
            &events,
        );
        shortcuts.key_pressed(K, Some(Code::KeyK), Modifiers::empty(), &events);
        shortcuts.key_released(K, &events);

        assert!(drain(&received).is_empty());
    }

    #[test]
    fn a_held_combination_is_released_when_its_key_is_gone() {
        let (events, received) = sender();
        let mut shortcuts = registered("Control+K");
        let external_k = (2, 37);
        shortcuts.key_pressed(external_k, Some(Code::KeyK), Modifiers::CONTROL, &events);
        drain(&received);

        // Another keyboard's keys are still down; the one holding K is gone.
        shortcuts.release_unless_held(&[((1, 29), Some(SystemKey::LeftControl))], &events);
        assert_eq!(
            drain(&received),
            vec![shortcut("Control+K", ShortcutState::Released)]
        );

        // Nothing more to release, even when the key comes up after all.
        shortcuts.key_released(external_k, &events);
        assert!(drain(&received).is_empty());
    }

    #[test]
    fn a_key_is_released_as_the_modifier_it_was_pressed_as() {
        let (events, received) = sender();
        let mut keys = Keys::new();
        keys.press(LEFT_CONTROL, Some(SystemKey::LeftControl), &events);
        drain(&received);

        // A remapper turned the key into a letter while it was down.
        assert!(keys.release(LEFT_CONTROL, None, &events));
        assert_eq!(
            drain(&received),
            vec![
                AppEvent::SystemKeyReleased {
                    key: SystemKey::LeftControl,
                    pressed_keys: vec![],
                },
                AppEvent::SystemKeysReleased,
            ]
        );
    }

    #[test]
    fn combinations_held_together_are_released_one_by_one() {
        let (events, received) = sender();
        let mut shortcuts = registered("Control+K");
        shortcuts
            .registered
            .push(("Control+J".parse().unwrap(), "Control+J".to_string()));
        let j = (0, 36);
        shortcuts.key_pressed(K, Some(Code::KeyK), Modifiers::CONTROL, &events);
        shortcuts.key_pressed(j, Some(Code::KeyJ), Modifiers::CONTROL, &events);
        shortcuts.key_released(j, &events);
        shortcuts.key_released(K, &events);

        assert_eq!(
            drain(&received),
            vec![
                shortcut("Control+K", ShortcutState::Pressed),
                shortcut("Control+J", ShortcutState::Pressed),
                shortcut("Control+J", ShortcutState::Released),
                shortcut("Control+K", ShortcutState::Released),
            ]
        );
    }

    #[test]
    fn a_held_combination_survives_a_reset_that_finds_its_key_down() {
        let (events, received) = sender();
        let mut shortcuts = registered("Control+K");
        shortcuts.key_pressed(K, Some(Code::KeyK), Modifiers::CONTROL, &events);
        drain(&received);

        shortcuts.release_unless_held(&[(K, None)], &events);
        assert!(drain(&received).is_empty());
    }

    #[test]
    fn combination_keys_follow_the_layout() {
        const KEY_Q: u32 = 16;
        const KEY_SEMICOLON: u32 = 39;
        const KEY_KP0: u32 = 82;

        // On AZERTY the key in Q's place types A.
        assert_eq!(
            combination_key(KEY_Q, Some(u32::from(b'a'))),
            Some(Code::KeyA)
        );
        assert_eq!(
            combination_key(KEY_Q, Some(u32::from(b'q'))),
            Some(Code::KeyQ)
        );
        assert_eq!(
            combination_key(KEY_SEMICOLON, Some(u32::from(b'm'))),
            Some(Code::KeyM)
        );
        // Without a layout, or for a keysym the recorder never writes, the
        // key is taken by its place.
        assert_eq!(combination_key(KEY_Q, None), Some(Code::KeyQ));
        assert_eq!(combination_key(KEY_KP0, Some(0xff9e)), Some(Code::Numpad0));
        assert_eq!(key_for_keysym(0xffbe), None);
    }

    #[test]
    fn every_key_the_recorder_writes_has_a_kernel_code() {
        let names = [
            "A",
            "Z",
            "0",
            "9",
            "`",
            "\\",
            "[",
            "]",
            ",",
            "=",
            "-",
            ".",
            "'",
            ";",
            "/",
            "Space",
            "Tab",
            "Enter",
            "Escape",
            "Backspace",
            "Delete",
            "CapsLock",
            "ArrowDown",
            "ArrowLeft",
            "ArrowRight",
            "ArrowUp",
            "Home",
            "End",
            "PageUp",
            "PageDown",
            "Numpad0",
            "NumpadAdd",
            "NumpadEnter",
            "F1",
            "F12",
            "F13",
            "F24",
        ];
        let codes: Vec<Code> = (0..256).filter_map(physical_key).collect();
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
