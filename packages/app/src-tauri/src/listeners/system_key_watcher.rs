use std::collections::BTreeSet;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use block2::RcBlock;
use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags, NSEventType};
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
const KEYCODE_LEFT_COMMAND: u16 = 0x37;
const KEYCODE_RIGHT_COMMAND: u16 = 0x36;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
enum SystemKey {
    Fn,
    LeftControl,
    RightControl,
    LeftOption,
    RightOption,
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
        KEYCODE_LEFT_COMMAND => Some(SystemKey::LeftCommand),
        KEYCODE_RIGHT_COMMAND => Some(SystemKey::RightCommand),
        _ => None,
    }
}

fn key_is_down_in_flags(key: SystemKey, flags: NSEventModifierFlags) -> bool {
    match key {
        SystemKey::Fn => flags.contains(NSEventModifierFlags::Function),
        SystemKey::LeftControl | SystemKey::RightControl => {
            flags.contains(NSEventModifierFlags::Control)
        }
        SystemKey::LeftOption | SystemKey::RightOption => {
            flags.contains(NSEventModifierFlags::Option)
        }
        SystemKey::LeftCommand | SystemKey::RightCommand => {
            flags.contains(NSEventModifierFlags::Command)
        }
    }
}

fn emit_system_key_event(event: &NSEvent) {
    let Some(app_handle) = APP_HANDLE.get() else {
        return;
    };

    let Some(key) = key_from_keycode(event.keyCode()) else {
        return;
    };

    let flags = event.modifierFlags();
    let is_down = key_is_down_in_flags(key, flags);
    let mut pressed = pressed_keys()
        .lock()
        .expect("system key pressed state mutex poisoned");

    if is_down {
        let was_new_key = pressed.insert(key);

        if was_new_key {
            let _ = app_handle.emit(
                "system-key-pressed",
                SystemKeyPressedPayload {
                    key,
                    pressed_keys: pressed.iter().copied().collect(),
                },
            );
        }

        return;
    }

    if pressed.remove(&key) {
        let pressed_keys = pressed.iter().copied().collect::<Vec<_>>();
        let _ = app_handle.emit(
            "system-key-released",
            SystemKeyReleasedPayload { key, pressed_keys },
        );

        if pressed.is_empty() {
            let _ = app_handle.emit("system-keys-released", ());
        }
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
