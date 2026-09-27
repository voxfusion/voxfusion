use std::collections::BTreeSet;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use block2::RcBlock;
use objc2_app_kit::{NSEvent, NSEventMask, NSEventType};
use objc2_core_graphics::{CGEventSource, CGEventSourceStateID};
use serde::Serialize;
use tauri::Emitter;

static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();
static PRESSED_KEYS: OnceLock<Mutex<BTreeSet<SystemKey>>> = OnceLock::new();
static WATCHER_RUNNING: AtomicBool = AtomicBool::new(false);

// Virtual key codes for the modifier keys we track. These are stable,
// hardware-independent key codes shared by Core Graphics and AppKit.
const KEYCODE_FN: u16 = 0x3F;
const KEYCODE_LEFT_CONTROL: u16 = 0x3B;
const KEYCODE_RIGHT_CONTROL: u16 = 0x3E;
const KEYCODE_LEFT_OPTION: u16 = 0x3A;
const KEYCODE_RIGHT_OPTION: u16 = 0x3D;
const KEYCODE_LEFT_SHIFT: u16 = 0x38;
const KEYCODE_RIGHT_SHIFT: u16 = 0x3C;
const KEYCODE_LEFT_COMMAND: u16 = 0x37;
const KEYCODE_RIGHT_COMMAND: u16 = 0x36;

// NX_DEVICE*KEYMASK and NX_SECONDARYFNMASK from IOKit/hidsystem/IOLLEvent.h.
// Keep the device-dependent bits: the public Command/Control/Option/Shift
// flags each combine both sides of the keyboard.
const SYSTEM_KEY_FLAGS: [(SystemKey, u64); 9] = [
    (SystemKey::Fn, 0x0080_0000),
    (SystemKey::LeftControl, 0x0000_0001),
    (SystemKey::RightControl, 0x0000_2000),
    (SystemKey::LeftOption, 0x0000_0020),
    (SystemKey::RightOption, 0x0000_0040),
    (SystemKey::LeftShift, 0x0000_0002),
    (SystemKey::RightShift, 0x0000_0004),
    (SystemKey::LeftCommand, 0x0000_0008),
    (SystemKey::RightCommand, 0x0000_0010),
];

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
enum SystemKey {
    Fn,
    LeftControl,
    RightControl,
    LeftOption,
    RightOption,
    LeftShift,
    RightShift,
    LeftCommand,
    RightCommand,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemKeyPressedPayload {
    key: SystemKey,
    pressed_keys: Vec<SystemKey>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemKeyReleasedPayload {
    key: SystemKey,
    pressed_keys: Vec<SystemKey>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct KeyboardKeyPressedPayload {
    key_code: i64,
}

fn pressed_keys() -> &'static Mutex<BTreeSet<SystemKey>> {
    PRESSED_KEYS.get_or_init(|| Mutex::new(BTreeSet::new()))
}

fn key_from_keycode(keycode: u16) -> Option<SystemKey> {
    match keycode {
        KEYCODE_FN => Some(SystemKey::Fn),
        KEYCODE_LEFT_CONTROL => Some(SystemKey::LeftControl),
        KEYCODE_RIGHT_CONTROL => Some(SystemKey::RightControl),
        KEYCODE_LEFT_OPTION => Some(SystemKey::LeftOption),
        KEYCODE_RIGHT_OPTION => Some(SystemKey::RightOption),
        KEYCODE_LEFT_SHIFT => Some(SystemKey::LeftShift),
        KEYCODE_RIGHT_SHIFT => Some(SystemKey::RightShift),
        KEYCODE_LEFT_COMMAND => Some(SystemKey::LeftCommand),
        KEYCODE_RIGHT_COMMAND => Some(SystemKey::RightCommand),
        _ => None,
    }
}

fn pressed_keys_from_flags(flags: u64) -> BTreeSet<SystemKey> {
    SYSTEM_KEY_FLAGS
        .iter()
        .filter_map(|(key, mask)| (flags & mask != 0).then_some(*key))
        .collect()
}

fn current_pressed_keys() -> BTreeSet<SystemKey> {
    pressed_keys_from_flags(
        CGEventSource::flags_state(CGEventSourceStateID::CombinedSessionState).bits(),
    )
}

fn pressed_keys_for_event(event: &NSEvent) -> BTreeSet<SystemKey> {
    // Read the snapshot attached to this event, not the live keycode table.
    // CGEventSourceKeyState can report right Command as left Command, and
    // polling at delivery time can see later transitions already queued up.
    pressed_keys_from_flags(event.modifierFlags().bits() as u64)
}

/// A flags-changed event normally toggles exactly the key identified by its
/// keycode. Any other difference means AppKit skipped at least one transition
/// (for example across sleep, screen lock, or an agent-app lifecycle change).
fn transition_is_contiguous(
    previous: &BTreeSet<SystemKey>,
    current: &BTreeSet<SystemKey>,
    event_key: SystemKey,
) -> bool {
    let is_down = current.contains(&event_key);
    if previous.contains(&event_key) == is_down {
        return false;
    }

    let mut expected = previous.clone();
    if is_down {
        expected.insert(event_key);
    } else {
        expected.remove(&event_key);
    }

    expected == *current
}

fn emit_system_key_event(event: &NSEvent) {
    let Some(app_handle) = APP_HANDLE.get() else {
        return;
    };

    let Some(key) = key_from_keycode(event.keyCode()) else {
        return;
    };

    // Rebuild the complete state on every event instead of accumulating
    // transitions. The event's side-specific flags also repair stale keys
    // immediately after a missed release.
    let current = pressed_keys_for_event(event);
    let mut pressed = pressed_keys()
        .lock()
        .expect("system key pressed state mutex poisoned");
    let previous = pressed.clone();
    let was_resynchronized = !transition_is_contiguous(&previous, &current, key);

    if was_resynchronized {
        log::warn!(
            target: "hotkey",
            "system_key_state_resynchronized event_key={key:?} previous={previous:?} current={current:?}"
        );
        // Reset per-shortcut `isHeld` state before delivering the corrected
        // snapshot. This also recovers when the missed transition was the
        // release of the same key that is being pressed again now.
        let _ = app_handle.emit("system-keys-released", ());
    }

    *pressed = current.clone();
    drop(pressed);

    if current.contains(&key) {
        let _ = app_handle.emit(
            "system-key-pressed",
            SystemKeyPressedPayload {
                key,
                pressed_keys: current.iter().copied().collect(),
            },
        );
        return;
    }

    let _ = app_handle.emit(
        "system-key-released",
        SystemKeyReleasedPayload {
            key,
            pressed_keys: current.iter().copied().collect(),
        },
    );

    if current.is_empty() && !was_resynchronized {
        let _ = app_handle.emit("system-keys-released", ());
    }
}

fn emit_keyboard_key_pressed(event: &NSEvent) {
    let Some(app_handle) = APP_HANDLE.get() else {
        return;
    };

    if event.isARepeat() {
        return;
    }

    let key_code = event.keyCode() as i64;
    let _ = app_handle.emit(
        "keyboard-key-pressed",
        KeyboardKeyPressedPayload { key_code },
    );
}

fn handle_event(event: &NSEvent) {
    match event.r#type() {
        NSEventType::FlagsChanged => emit_system_key_event(event),
        NSEventType::KeyDown => emit_keyboard_key_pressed(event),
        _ => {}
    }
}

pub fn setup(app_handle: &tauri::AppHandle) {
    APP_HANDLE.set(app_handle.clone()).ok();

    if WATCHER_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }

    let initial_keys = current_pressed_keys();
    *pressed_keys()
        .lock()
        .expect("system key pressed state mutex poisoned") = initial_keys.clone();
    log::info!(target: "hotkey", "system_key_watcher_started pressed_keys={initial_keys:?}");

    // NSEvent monitors must be installed on the main thread, where the
    // application run loop dispatches them. Observing `FlagsChanged` (for
    // modifier-only hotkeys) via NSEvent — rather than a Core Graphics event
    // tap — means we no longer require the macOS Input Monitoring permission.
    let scheduled = app_handle.run_on_main_thread(|| {
        let mask = NSEventMask::FlagsChanged | NSEventMask::KeyDown;

        // Events delivered to *other* applications — the usual case while
        // dictating into another app.
        let global_block = RcBlock::new(|event: NonNull<NSEvent>| {
            handle_event(unsafe { event.as_ref() });
        });
        let global_monitor =
            NSEvent::addGlobalMonitorForEventsMatchingMask_handler(mask, &global_block);

        // Events delivered to our own window — e.g. while recording a hotkey
        // in Settings, or for keys AppKit doesn't surface to the DOM (Fn).
        // The handler observes only and passes every event through unchanged.
        let local_block = RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
            handle_event(unsafe { event.as_ref() });
            event.as_ptr()
        });
        // SAFETY: the block returns the event pointer it was given, so the
        // event is always forwarded unchanged.
        let local_monitor =
            unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &local_block) };

        // The monitors live for the entire lifetime of the app; leak the
        // returned tokens so the monitors are never torn down.
        std::mem::forget(global_monitor);
        std::mem::forget(local_monitor);
    });

    if scheduled.is_err() {
        WATCHER_RUNNING.store(false, Ordering::SeqCst);
    }
}

pub fn resynchronize(app_handle: &tauri::AppHandle, reason: &str) {
    let current = current_pressed_keys();
    let mut pressed = pressed_keys()
        .lock()
        .expect("system key pressed state mutex poisoned");
    let previous = std::mem::replace(&mut *pressed, current.clone());
    drop(pressed);

    log::info!(
        target: "hotkey",
        "system_key_state_lifecycle_resynchronized reason={reason} previous={previous:?} current={current:?}"
    );
    // Lifecycle changes can also strand the frontend's per-registration
    // `isHeld` flag even when Core Graphics already reports no keys down.
    let _ = app_handle.emit("system-keys-released", ());
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2::rc::Retained;
    use objc2_app_kit::NSEventModifierFlags;
    use objc2_foundation::{NSPoint, ns_string};

    fn keys(keys: &[SystemKey]) -> BTreeSet<SystemKey> {
        keys.iter().copied().collect()
    }

    fn event(keycode: u16, flags: u64) -> Retained<NSEvent> {
        NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            NSEventType::FlagsChanged,
            NSPoint::new(0.0, 0.0),
            NSEventModifierFlags::from_bits_retain(flags as usize),
            0.0,
            0,
            None,
            ns_string!(""),
            ns_string!(""),
            false,
            keycode,
        )
        .expect("flags-changed event")
    }

    #[test]
    fn right_command_press_and_release_use_the_event_snapshot() {
        // These events are not posted to the OS. The live keycode table
        // cannot describe them, just as it cannot describe a queued event.
        let pressed = event(KEYCODE_RIGHT_COMMAND, 0x0010_0010);
        let released = event(KEYCODE_RIGHT_COMMAND, 0);

        assert_eq!(
            key_from_keycode(pressed.keyCode()),
            Some(SystemKey::RightCommand)
        );
        assert_eq!(
            pressed_keys_for_event(&pressed),
            keys(&[SystemKey::RightCommand])
        );
        assert_eq!(pressed_keys_for_event(&released), keys(&[]));
        // Constructing a later release must not alter the earlier press.
        assert_eq!(
            pressed_keys_for_event(&pressed),
            keys(&[SystemKey::RightCommand])
        );
    }

    #[test]
    fn releasing_right_command_preserves_a_held_left_command() {
        let both = pressed_keys_for_event(&event(KEYCODE_RIGHT_COMMAND, 0x0010_0018));
        let left = pressed_keys_for_event(&event(KEYCODE_RIGHT_COMMAND, 0x0010_0008));

        assert_eq!(
            both,
            keys(&[SystemKey::LeftCommand, SystemKey::RightCommand])
        );
        assert_eq!(left, keys(&[SystemKey::LeftCommand]));
        assert!(transition_is_contiguous(
            &both,
            &left,
            SystemKey::RightCommand
        ));
    }

    #[test]
    fn flags_distinguish_every_supported_modifier() {
        for (flags, key) in [
            (0x0080_0000, SystemKey::Fn),
            (0x0004_0001, SystemKey::LeftControl),
            (0x0004_2000, SystemKey::RightControl),
            (0x0008_0020, SystemKey::LeftOption),
            (0x0008_0040, SystemKey::RightOption),
            (0x0002_0002, SystemKey::LeftShift),
            (0x0002_0004, SystemKey::RightShift),
            (0x0010_0008, SystemKey::LeftCommand),
            (0x0010_0010, SystemKey::RightCommand),
        ] {
            assert_eq!(pressed_keys_from_flags(flags), keys(&[key]));
        }
    }

    #[test]
    fn queued_chord_events_preserve_their_own_modifier_state() {
        let control = event(KEYCODE_LEFT_CONTROL, 0x0004_0001);
        let option = event(KEYCODE_LEFT_OPTION, 0x000c_0021);
        let shift = event(KEYCODE_LEFT_SHIFT, 0x000e_0023);
        let command = event(KEYCODE_LEFT_COMMAND, 0x001e_002b);

        let mut previous = keys(&[]);
        for event in [control, option, shift, command] {
            let current = pressed_keys_for_event(&event);
            let key = key_from_keycode(event.keyCode()).unwrap();
            assert!(transition_is_contiguous(&previous, &current, key));
            previous = current;
        }
        assert_eq!(
            previous,
            keys(&[
                SystemKey::LeftControl,
                SystemKey::LeftOption,
                SystemKey::LeftShift,
                SystemKey::LeftCommand,
            ])
        );
    }

    #[test]
    fn event_snapshot_removes_a_stale_modifier_before_right_command() {
        let current = pressed_keys_for_event(&event(KEYCODE_RIGHT_COMMAND, 0x0010_0010));
        assert_eq!(current, keys(&[SystemKey::RightCommand]));
        assert!(!transition_is_contiguous(
            &keys(&[SystemKey::Fn]),
            &current,
            SystemKey::RightCommand,
        ));
    }

    #[test]
    fn unrelated_flags_do_not_create_a_modifier_hotkey() {
        // Caps Lock, numeric pad, help and event-coalescing flags.
        assert_eq!(pressed_keys_from_flags(0x0061_0100), keys(&[]));
    }

    #[test]
    fn accepts_a_normal_press_and_release_sequence() {
        assert!(transition_is_contiguous(
            &keys(&[]),
            &keys(&[SystemKey::LeftControl]),
            SystemKey::LeftControl,
        ));
        assert!(transition_is_contiguous(
            &keys(&[SystemKey::LeftControl]),
            &keys(&[]),
            SystemKey::LeftControl,
        ));
    }

    #[test]
    fn detects_a_missed_release_before_the_same_key_is_pressed_again() {
        assert!(!transition_is_contiguous(
            &keys(&[SystemKey::LeftControl]),
            &keys(&[SystemKey::LeftControl]),
            SystemKey::LeftControl,
        ));
    }

    #[test]
    fn detects_an_unrelated_stale_modifier() {
        assert!(!transition_is_contiguous(
            &keys(&[SystemKey::Fn]),
            &keys(&[SystemKey::LeftControl]),
            SystemKey::LeftControl,
        ));
    }
}
