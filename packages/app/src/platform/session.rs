//! The kind of desktop session the app runs in on Linux.
//!
//! On Wayland the app still draws through X11, by way of XWayland: the
//! dictation overlay has to float above the app in front, at a place of its
//! choosing, without taking the focus, and Wayland gives an app no such
//! window on GNOME, which has no layer-shell protocol. Hotkeys then come from
//! the keyboard devices themselves, since a Wayland compositor shows an app
//! only the keys typed into its own windows.

use std::sync::OnceLock;

static WAYLAND: OnceLock<bool> = OnceLock::new();

fn is_set(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| !value.is_empty())
}

/// Remembers whether the session is Wayland, and has GPUI draw through
/// XWayland when there is one.
///
/// Call it first thing, before any other thread starts: it changes the
/// environment, which other threads may be reading.
pub fn init() {
    let wayland = is_set("WAYLAND_DISPLAY");
    WAYLAND.set(wayland).ok();

    if wayland && is_set("DISPLAY") {
        // SAFETY: no other thread runs yet (see above).
        unsafe { std::env::remove_var("WAYLAND_DISPLAY") };
    }
}

/// Whether the desktop is a Wayland compositor, even when the app draws
/// through XWayland.
pub fn is_wayland() -> bool {
    WAYLAND.get().copied().unwrap_or(false)
}
