//! Native control of the dictation overlay's window. GPUI opens the window,
//! but cannot hide it, show it again without activating the app, or move it.
//!
//! Rectangles are in logical screen coordinates with the origin at the top
//! left of the primary display. AppKit's own origin is the bottom left; X11's
//! is the top left of the screen, in device pixels.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2_app_kit::{
        NSEvent, NSFloatingWindowLevel, NSPanel, NSScreen, NSView, NSWindow, NSWindowStyleMask,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    use super::ScreenRect;

    #[derive(Clone)]
    pub struct OverlayWindow {
        window: Retained<NSWindow>,
    }

    /// The height of the primary display, which AppKit's vertical axis is
    /// measured up from.
    fn primary_height() -> Option<f64> {
        let screens = NSScreen::screens(MainThreadMarker::new()?);
        Some(screens.firstObject()?.frame().size.height)
    }

    fn from_appkit(rect: NSRect, primary_height: f64) -> ScreenRect {
        ScreenRect {
            x: rect.origin.x,
            y: primary_height - rect.origin.y - rect.size.height,
            width: rect.size.width,
            height: rect.size.height,
        }
    }

    impl OverlayWindow {
        /// Takes over the window behind `handle`. Call on the main thread.
        pub fn new(handle: &impl HasWindowHandle, _scale_factor: f64) -> Option<Self> {
            MainThreadMarker::new()?;
            let RawWindowHandle::AppKit(handle) = handle.window_handle().ok()?.as_raw() else {
                return None;
            };

            // SAFETY: the handle holds the window's live content view, and
            // this runs on the main thread, where AppKit objects may be used.
            let view = unsafe { Retained::retain(handle.ns_view.as_ptr().cast::<NSView>()) }?;
            let window = view.window()?;

            // Above other apps' windows, below the Dock and the menu bar.
            window.setLevel(NSFloatingWindowLevel);
            // No shadow around whatever the window draws.
            window.setHasShadow(false);
            // Clicking the overlay's buttons must not take the keyboard from
            // the app being dictated into.
            if let Some(panel) = window.downcast_ref::<NSPanel>() {
                panel.setBecomesKeyOnlyIfNeeded(true);
            }

            Some(Self { window })
        }

        /// Makes the window borderless. GPUI gives a window without a title
        /// bar a titled frame with the content drawn over it; macOS outlines
        /// a titled window with a hairline and rounds its corners, which
        /// shows as a faint box around the pill.
        ///
        /// AppKit reports the change to GPUI before this returns, and GPUI
        /// cannot take that while it is updating the app: call this from a
        /// task, not from inside an update.
        pub fn remove_frame(&self) {
            let panel_behavior = self.window.styleMask() & NSWindowStyleMask::NonactivatingPanel;
            self.window
                .setStyleMask(NSWindowStyleMask::Borderless | panel_behavior);
        }

        /// Shows the window without activating the app or taking key focus.
        pub fn show(&self) {
            self.window.orderFrontRegardless();
        }

        pub fn hide(&self) {
            self.window.orderOut(None);
        }

        pub fn frame(&self) -> Option<ScreenRect> {
            Some(from_appkit(self.window.frame(), primary_height()?))
        }

        pub fn set_frame(&self, frame: ScreenRect) {
            let Some(primary_height) = primary_height() else {
                return;
            };

            let rect = NSRect::new(
                NSPoint::new(frame.x, primary_height - frame.y - frame.height),
                NSSize::new(frame.width, frame.height),
            );
            self.window.setFrame_display(rect, true);
        }

        /// Device pixels per logical pixel on the display the window is on.
        pub fn scale_factor(&self) -> f64 {
            self.window.backingScaleFactor()
        }
    }

    /// Every display, the primary one first.
    pub fn displays() -> Vec<ScreenRect> {
        let Some(mtm) = MainThreadMarker::new() else {
            return Vec::new();
        };
        let Some(primary_height) = primary_height() else {
            return Vec::new();
        };

        NSScreen::screens(mtm)
            .iter()
            .map(|screen| from_appkit(screen.frame(), primary_height))
            .collect()
    }

    pub fn cursor_position() -> Option<(f64, f64)> {
        let location = NSEvent::mouseLocation();
        Some((location.x, primary_height()? - location.y))
    }
}

/// On X11 the overlay is an override-redirect window, which the window
/// manager leaves alone: it is mapped, unmapped and moved directly, on a
/// connection of the app's own.
#[cfg(target_os = "linux")]
mod x11 {
    use std::sync::OnceLock;

    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use x11rb::connection::Connection as _;
    use x11rb::protocol::randr::ConnectionExt as _;
    use x11rb::protocol::xproto::{self, ConnectionExt as _};
    use x11rb::rust_connection::RustConnection;

    use super::ScreenRect;

    struct Server {
        connection: RustConnection,
        root: xproto::Window,
    }

    /// Device pixels per logical pixel, as GPUI took it for the window. X11
    /// has one for the whole screen.
    static SCALE_FACTOR: OnceLock<f64> = OnceLock::new();

    fn server() -> Option<&'static Server> {
        static SERVER: OnceLock<Option<Server>> = OnceLock::new();
        SERVER
            .get_or_init(|| match x11rb::connect(None) {
                Ok((connection, screen)) => {
                    let root = connection.setup().roots[screen].root;
                    Some(Server { connection, root })
                }
                Err(err) => {
                    log::warn!(target: "runtime", "overlay_x11_unavailable error={err}");
                    None
                }
            })
            .as_ref()
    }

    fn scale() -> f64 {
        SCALE_FACTOR.get().copied().unwrap_or(1.)
    }

    /// Whether GPUI draws through X11, where the overlay can be controlled.
    pub fn is_supported() -> bool {
        let set = |name| std::env::var_os(name).is_some_and(|value| !value.is_empty());
        !set("WAYLAND_DISPLAY") && set("DISPLAY")
    }

    #[derive(Clone)]
    pub struct OverlayWindow {
        window: xproto::Window,
    }

    impl OverlayWindow {
        /// Takes over the window behind `handle`, if it is an X11 window.
        pub fn new(handle: &impl HasWindowHandle, scale_factor: f64) -> Option<Self> {
            let window = match handle.window_handle().ok()?.as_raw() {
                RawWindowHandle::Xcb(handle) => handle.window.get(),
                RawWindowHandle::Xlib(handle) => u32::try_from(handle.window).ok()?,
                _ => return None,
            };
            server()?;
            SCALE_FACTOR.set(scale_factor).ok();
            Some(Self { window })
        }

        /// Override-redirect windows have no frame.
        pub fn remove_frame(&self) {}

        /// Shows the window above the others. It never takes the focus.
        pub fn show(&self) {
            let Some(server) = server() else {
                return;
            };
            let connection = &server.connection;
            let _ = connection.map_window(self.window);
            let _ = connection.configure_window(
                self.window,
                &xproto::ConfigureWindowAux::new().stack_mode(xproto::StackMode::ABOVE),
            );
            let _ = connection.flush();
        }

        pub fn hide(&self) {
            let Some(server) = server() else {
                return;
            };
            let _ = server.connection.unmap_window(self.window);
            let _ = server.connection.flush();
        }

        pub fn frame(&self) -> Option<ScreenRect> {
            let server = server()?;
            let connection = &server.connection;
            let geometry = connection.get_geometry(self.window).ok()?.reply().ok()?;
            let origin = connection
                .translate_coordinates(self.window, server.root, 0, 0)
                .ok()?
                .reply()
                .ok()?;
            let scale = scale();

            Some(ScreenRect {
                x: f64::from(origin.dst_x) / scale,
                y: f64::from(origin.dst_y) / scale,
                width: f64::from(geometry.width) / scale,
                height: f64::from(geometry.height) / scale,
            })
        }

        pub fn set_frame(&self, frame: ScreenRect) {
            let Some(server) = server() else {
                return;
            };
            let scale = scale();
            let device = |value: f64| (value * scale).round();
            let _ = server.connection.configure_window(
                self.window,
                &xproto::ConfigureWindowAux::new()
                    .x(device(frame.x) as i32)
                    .y(device(frame.y) as i32)
                    .width(device(frame.width).max(1.) as u32)
                    .height(device(frame.height).max(1.) as u32),
            );
            let _ = server.connection.flush();
        }

        pub fn scale_factor(&self) -> f64 {
            scale()
        }
    }

    /// Every display, the primary one first.
    pub fn displays() -> Vec<ScreenRect> {
        let Some(server) = server() else {
            return Vec::new();
        };
        let Some(reply) = server
            .connection
            .randr_get_monitors(server.root, true)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
        else {
            return Vec::new();
        };
        let scale = scale();

        let mut monitors = reply.monitors;
        // Stable, so the others keep the server's order.
        monitors.sort_by_key(|monitor| !monitor.primary);
        monitors
            .iter()
            .map(|monitor| ScreenRect {
                x: f64::from(monitor.x) / scale,
                y: f64::from(monitor.y) / scale,
                width: f64::from(monitor.width) / scale,
                height: f64::from(monitor.height) / scale,
            })
            .collect()
    }

    pub fn cursor_position() -> Option<(f64, f64)> {
        let server = server()?;
        let pointer = server
            .connection
            .query_pointer(server.root)
            .ok()?
            .reply()
            .ok()?;
        let scale = scale();
        Some((
            f64::from(pointer.root_x) / scale,
            f64::from(pointer.root_y) / scale,
        ))
    }
}

/// Other platforms have no native control: the window stays where and as GPUI
/// opened it.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod fallback {
    use raw_window_handle::HasWindowHandle;

    use super::ScreenRect;

    #[derive(Clone)]
    pub enum OverlayWindow {}

    impl OverlayWindow {
        pub fn new(_handle: &impl HasWindowHandle, _scale_factor: f64) -> Option<Self> {
            None
        }

        pub fn remove_frame(&self) {
            match *self {}
        }

        pub fn show(&self) {
            match *self {}
        }

        pub fn hide(&self) {
            match *self {}
        }

        pub fn frame(&self) -> Option<ScreenRect> {
            match *self {}
        }

        pub fn set_frame(&self, _frame: ScreenRect) {
            match *self {}
        }

        pub fn scale_factor(&self) -> f64 {
            match *self {}
        }
    }

    pub fn displays() -> Vec<ScreenRect> {
        Vec::new()
    }

    pub fn cursor_position() -> Option<(f64, f64)> {
        None
    }
}

#[cfg(target_os = "macos")]
pub use macos::{OverlayWindow, cursor_position, displays};

#[cfg(target_os = "linux")]
pub use x11::{OverlayWindow, cursor_position, displays};

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub use fallback::{OverlayWindow, cursor_position, displays};

/// Whether the overlay's window can be shown and hidden later. Where it
/// cannot, it is opened shown, and draws nothing between dictations.
pub fn can_hide() -> bool {
    #[cfg(target_os = "macos")]
    return true;

    #[cfg(target_os = "linux")]
    return x11::is_supported();

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    false
}
