//! Native control of the dictation overlay's window. GPUI opens the window,
//! but cannot hide it, show it again without activating the app, or move it.
//!
//! Rectangles are in logical screen coordinates with the origin at the top
//! left of the primary display. AppKit's own origin is the bottom left.

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
        pub fn new(handle: &impl HasWindowHandle) -> Option<Self> {
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

/// Other platforms have no native control: the window stays where and as GPUI
/// opened it.
#[cfg(not(target_os = "macos"))]
mod fallback {
    use raw_window_handle::HasWindowHandle;

    use super::ScreenRect;

    #[derive(Clone)]
    pub enum OverlayWindow {}

    impl OverlayWindow {
        pub fn new(_handle: &impl HasWindowHandle) -> Option<Self> {
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

#[cfg(not(target_os = "macos"))]
pub use fallback::{OverlayWindow, cursor_position, displays};
