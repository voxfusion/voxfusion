//! Watches the keyboard for modifier-only hotkeys on Linux, wherever the
//! focus is: through XInput 2 raw key events on X11.
//!
//! Key codes are the kernel's (evdev) codes, which X11 key codes are offset
//! from by 8. Modifiers are recognized by what the keyboard layout makes of
//! the key, so a key remapped to Control counts as Control.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, OnceLock};

use crate::backend::{AppEvent, EventSender, SystemKey};

/// The kernel's code for Escape.
pub const ESCAPE_KEY_CODE: i64 = 1;

static EVENTS: OnceLock<EventSender> = OnceLock::new();
static KEYS: Mutex<Keys> = Mutex::new(Keys::new());
/// Set once a watcher runs; holds what it failed with otherwise.
static STARTED: OnceLock<Result<Backend, String>> = OnceLock::new();

/// Starts watching, once. Later calls report how the first one went.
pub fn setup(events: &EventSender) -> Result<(), String> {
    EVENTS.set(events.clone()).ok();

    let started = STARTED.get_or_init(|| {
        let started = x11::start();
        match &started {
            Ok(_) => log::info!(target: "hotkey", "system_key_watcher_started backend=x11"),
            Err(err) => log::warn!(target: "hotkey", "system_key_watcher_unavailable error={err}"),
        }
        started
    });
    started.as_ref().map(|_| ()).map_err(Clone::clone)
}

/// Rebuilds the pressed-key state after the app was reopened, when key
/// transitions may have gone unobserved.
pub fn resynchronize(reason: &str) {
    let Some(Ok(backend)) = STARTED.get() else {
        return;
    };
    let current = backend.held_keys();
    log::info!(target: "hotkey", "system_key_state_lifecycle_resynchronized reason={reason}");
    with_keys(|keys, events| keys.reset(current, events));
}

/// A running watcher.
enum Backend {
    X11(x11::Watcher),
}

impl Backend {
    /// The key codes held right now, as the system reports them.
    fn held_keys(&self) -> Vec<(u32, Option<SystemKey>)> {
        match self {
            Backend::X11(watcher) => watcher.held_keys(),
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
    fn layouts_name_the_modifiers() {
        assert_eq!(modifier_for_keysym(0xffe3), Some(SystemKey::LeftControl));
        assert_eq!(modifier_for_keysym(0xfe03), Some(SystemKey::RightOption));
        assert_eq!(modifier_for_keysym(0xffec), Some(SystemKey::RightCommand));
        assert_eq!(modifier_for_keysym(u32::from(b'k')), None);
    }
}
