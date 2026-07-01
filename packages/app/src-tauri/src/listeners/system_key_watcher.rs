use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;

use core_foundation::runloop::{CFRunLoop, kCFRunLoopCommonModes};
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventTapProxy, CGEventType, EventField, KeyCode,
};
use serde::Serialize;
use tauri::Emitter;

static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();
static PRESSED_KEYS: OnceLock<Mutex<BTreeSet<SystemKey>>> = OnceLock::new();
static WATCHER_RUNNING: AtomicBool = AtomicBool::new(false);

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

fn key_from_keycode(keycode: i64) -> Option<SystemKey> {
    match keycode as u16 {
        KeyCode::FUNCTION => Some(SystemKey::Fn),
        KeyCode::CONTROL => Some(SystemKey::LeftControl),
        KeyCode::RIGHT_CONTROL => Some(SystemKey::RightControl),
        KeyCode::OPTION => Some(SystemKey::LeftOption),
        KeyCode::RIGHT_OPTION => Some(SystemKey::RightOption),
        KeyCode::COMMAND => Some(SystemKey::LeftCommand),
        KeyCode::RIGHT_COMMAND => Some(SystemKey::RightCommand),
        _ => None,
    }
}

fn key_is_down_in_flags(key: SystemKey, flags: CGEventFlags) -> bool {
    match key {
        SystemKey::Fn => flags.contains(CGEventFlags::CGEventFlagSecondaryFn),
        SystemKey::LeftControl | SystemKey::RightControl => {
            flags.contains(CGEventFlags::CGEventFlagControl)
        }
        SystemKey::LeftOption | SystemKey::RightOption => {
            flags.contains(CGEventFlags::CGEventFlagAlternate)
        }
        SystemKey::LeftCommand | SystemKey::RightCommand => {
            flags.contains(CGEventFlags::CGEventFlagCommand)
        }
    }
}

fn emit_system_key_event(event: &CGEvent) {
    let Some(app_handle) = APP_HANDLE.get() else {
        return;
    };

    let keycode = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
    let Some(key) = key_from_keycode(keycode) else {
        return;
    };

    let flags = event.get_flags();
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

fn emit_keyboard_key_pressed(event: &CGEvent) {
    let Some(app_handle) = APP_HANDLE.get() else {
        return;
    };

    let is_repeat = event.get_integer_value_field(EventField::KEYBOARD_EVENT_AUTOREPEAT) != 0;
    if is_repeat {
        return;
    }

    let key_code = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
    let _ = app_handle.emit(
        "keyboard-key-pressed",
        KeyboardKeyPressedPayload { key_code },
    );
}

pub fn setup(app_handle: &tauri::AppHandle) {
    APP_HANDLE.set(app_handle.clone()).ok();

    if WATCHER_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }

    thread::spawn(|| {
        let tap = CGEventTap::new(
            CGEventTapLocation::HID,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::ListenOnly,
            vec![CGEventType::FlagsChanged, CGEventType::KeyDown],
            |_proxy: CGEventTapProxy, event_type: CGEventType, event: &CGEvent| {
                match event_type {
                    CGEventType::FlagsChanged => emit_system_key_event(event),
                    CGEventType::KeyDown => emit_keyboard_key_pressed(event),
                    _ => {}
                }
                None
            },
        );

        let Ok(tap) = tap else {
            WATCHER_RUNNING.store(false, Ordering::SeqCst);
            return;
        };

        unsafe {
            let source = tap
                .mach_port
                .create_runloop_source(0)
                .expect("failed to create system key run loop source");

            let run_loop = CFRunLoop::get_current();
            run_loop.add_source(&source, kCFRunLoopCommonModes);
            tap.enable();
            CFRunLoop::run_current();
        }

        WATCHER_RUNNING.store(false, Ordering::SeqCst);
    });
}
