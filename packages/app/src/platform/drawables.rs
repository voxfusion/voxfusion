//! The surfaces GPUI draws a window into.

use raw_window_handle::HasWindowHandle;

/// Lets the window keep two drawables instead of the three GPUI asks for.
/// Each is as large as the window in device pixels: 18.5 MB for the main
/// window at its default size on a Retina display. Two are what the frame on
/// screen and the next one need; a third only lets the app draw ahead of the
/// display, which an interface that changes when the user acts has no use
/// for. Call on the main thread, once GPUI has opened the window.
#[cfg(target_os = "macos")]
pub fn keep_two(handle: &impl HasWindowHandle) {
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, Bool};
    use objc2::{msg_send, sel};
    use raw_window_handle::RawWindowHandle;

    let Ok(handle) = handle.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return;
    };

    // SAFETY: the handle holds GPUI's live view, whose layer is the
    // `CAMetalLayer` it draws into; this runs on the main thread.
    let view = unsafe { handle.ns_view.cast::<AnyObject>().as_ref() };
    let Some(layer): Option<Retained<AnyObject>> = (unsafe { msg_send![view, layer] }) else {
        return;
    };

    let settable: Bool =
        unsafe { msg_send![&*layer, respondsToSelector: sel!(setMaximumDrawableCount:)] };
    if settable.as_bool() {
        let _: () = unsafe { msg_send![&*layer, setMaximumDrawableCount: 2usize] };
    }
}

#[cfg(not(target_os = "macos"))]
pub fn keep_two(_: &impl HasWindowHandle) {}
