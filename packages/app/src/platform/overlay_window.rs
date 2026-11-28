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

#[cfg(target_os = "windows")]
mod windows_impl {
    use std::collections::HashMap;
    use std::ffi::c_void;
    use std::sync::{LazyLock, Mutex};

    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
    use windows::Win32::Graphics::Dwm::{
        DWMWA_BORDER_COLOR, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
        DwmSetWindowAttribute,
    };
    use windows::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
    };
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    use windows::Win32::UI::WindowsAndMessaging::{
        CallWindowProcW, DefWindowProcW, GWL_EXSTYLE, GWL_STYLE, GWLP_WNDPROC, GetCursorPos,
        GetWindowLongPtrW, GetWindowRect, HWND_TOPMOST, MA_NOACTIVATE, MONITORINFOF_PRIMARY,
        SW_HIDE, SW_SHOWNOACTIVATE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SWP_NOZORDER, SWP_SHOWWINDOW, SetWindowLongPtrW, SetWindowPos, ShowWindow,
        WM_MOUSEACTIVATE, WM_NCDESTROY, WNDPROC, WS_BORDER, WS_CAPTION, WS_DLGFRAME,
        WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_SYSMENU,
        WS_THICKFRAME,
    };
    use windows_core::BOOL;

    use super::ScreenRect;

    const DWMWA_COLOR_NONE: u32 = 0xFFFF_FFFE;

    static PREV_WNDPROCS: LazyLock<Mutex<HashMap<usize, isize>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    /// Prevents mouse clicks or hover-to-activate from giving focus to the
    /// overlay pill while still delivering clicks to GPUI's window procedure.
    unsafe extern "system" fn overlay_wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_MOUSEACTIVATE {
            return LRESULT(MA_NOACTIVATE as isize);
        }
        let key = hwnd.0 as usize;
        let prev = {
            let mut map = PREV_WNDPROCS.lock().unwrap_or_else(|e| e.into_inner());
            if msg == WM_NCDESTROY {
                map.remove(&key)
            } else {
                map.get(&key).copied()
            }
        };
        if let Some(prev) = prev {
            if msg == WM_NCDESTROY {
                unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC, prev) };
            }
            let proc: WNDPROC = unsafe { std::mem::transmute::<isize, WNDPROC>(prev) };
            unsafe { CallWindowProcW(proc, hwnd, msg, wparam, lparam) }
        } else {
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
    }

    #[derive(Clone, Copy)]
    pub struct OverlayWindow {
        hwnd: usize,
    }

    impl OverlayWindow {
        fn hwnd(&self) -> HWND {
            HWND(self.hwnd as *mut c_void)
        }

        pub fn new(handle: &impl HasWindowHandle) -> Option<Self> {
            let RawWindowHandle::Win32(handle) = handle.window_handle().ok()?.as_raw() else {
                return None;
            };
            let hwnd = HWND(handle.hwnd.get() as *mut c_void);

            // Do not mutate window styles here: `new` runs inside GPUI's
            // `open_window` callback while `AppCell` is borrowed, and Win32
            // style changes synchronously re-enter `WndProc`. `remove_frame`
            // applies both style and extended style from an async task.
            Some(Self {
                hwnd: hwnd.0 as usize,
            })
        }

        pub fn remove_frame(&self) {
            let hwnd = self.hwnd();
            unsafe {
                let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                let new_ex_style = (ex_style & !(WS_EX_APPWINDOW.0 as isize))
                    | WS_EX_NOACTIVATE.0 as isize
                    | WS_EX_TOOLWINDOW.0 as isize
                    | WS_EX_TOPMOST.0 as isize;
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_ex_style);

                let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
                let stripped = style
                    & !(WS_CAPTION.0
                        | WS_THICKFRAME.0
                        | WS_BORDER.0
                        | WS_DLGFRAME.0
                        | WS_SYSMENU.0) as isize;
                SetWindowLongPtrW(hwnd, GWL_STYLE, stripped | WS_POPUP.0 as isize);

                let corner = DWMWCP_DONOTROUND;
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_WINDOW_CORNER_PREFERENCE,
                    (&raw const corner).cast(),
                    std::mem::size_of_val(&corner) as u32,
                );
                let border = DWMWA_COLOR_NONE;
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_BORDER_COLOR,
                    (&raw const border).cast(),
                    std::mem::size_of_val(&border) as u32,
                );

                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                );
            }

            let already_subclassed = PREV_WNDPROCS
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .contains_key(&self.hwnd);
            if !already_subclassed {
                let prev = unsafe {
                    SetWindowLongPtrW(
                        hwnd,
                        GWLP_WNDPROC,
                        overlay_wndproc as *const () as isize,
                    )
                };
                if prev != 0 {
                    PREV_WNDPROCS
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(self.hwnd, prev);
                }
            }
        }

        pub fn show(&self) {
            let hwnd = self.hwnd();
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
        }

        pub fn hide(&self) {
            unsafe {
                let _ = ShowWindow(self.hwnd(), SW_HIDE);
            }
        }

        /// Moves and resizes the overlay in physical screen coordinates on the
        /// monitor currently containing the cursor, avoiding mixed-DPI desktop
        /// coordinate distortion across monitors with different scale factors.
        pub fn reposition_to_cursor(
            &self,
            logical_width: f64,
            logical_height: f64,
            logical_bottom_padding: f64,
        ) {
            use windows::Win32::Graphics::Gdi::{
                MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY, MonitorFromPoint,
            };

            let mut pt = POINT::default();
            let monitor = if unsafe { GetCursorPos(&mut pt) }.is_ok() {
                unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST) }
            } else {
                unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) }
            };
            if monitor.0.is_null() {
                return;
            }

            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
                return;
            }

            let scale = monitor_scale(monitor);
            let work = info.rcWork;
            let width = (logical_width * scale).round() as i32;
            let height = (logical_height * scale).round() as i32;
            let bottom_padding = (logical_bottom_padding * scale).round() as i32;
            let x = work.left + ((work.right - work.left) - width) / 2;
            let y = work.bottom - height - bottom_padding;

            let mut current = RECT::default();
            if unsafe { GetWindowRect(self.hwnd(), &mut current) }.is_ok()
                && current.left == x
                && current.top == y
                && (current.right - current.left) == width
                && (current.bottom - current.top) == height
            {
                return;
            }

            unsafe {
                let _ = SetWindowPos(
                    self.hwnd(),
                    Some(HWND_TOPMOST),
                    x,
                    y,
                    width,
                    height,
                    SWP_NOACTIVATE,
                );
            }
        }
    }

    fn monitor_scale(monitor: HMONITOR) -> f64 {
        let mut dpi_x = 96u32;
        let mut dpi_y = 96u32;
        if unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) }.is_ok()
            && dpi_x > 0
        {
            dpi_x as f64 / 96.0
        } else {
            1.0
        }
    }

    /// Every display's work area (excluding the taskbar), primary first, in
    /// logical coordinates matching `OverlayWindow::scale_factor`.
    pub fn displays() -> Vec<ScreenRect> {
        struct Collector {
            primary: Option<ScreenRect>,
            others: Vec<ScreenRect>,
        }

        unsafe extern "system" fn callback(
            monitor: HMONITOR,
            _hdc: HDC,
            _rect: *mut RECT,
            data: LPARAM,
        ) -> BOOL {
            let collector = unsafe { &mut *(data.0 as *mut Collector) };
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
                let scale = monitor_scale(monitor);
                let r = info.rcWork;
                let rect = ScreenRect {
                    x: r.left as f64 / scale,
                    y: r.top as f64 / scale,
                    width: (r.right - r.left) as f64 / scale,
                    height: (r.bottom - r.top) as f64 / scale,
                };
                if info.dwFlags & MONITORINFOF_PRIMARY != 0 {
                    collector.primary = Some(rect);
                } else {
                    collector.others.push(rect);
                }
            }
            BOOL(1)
        }

        let mut collector = Collector {
            primary: None,
            others: Vec::new(),
        };
        unsafe {
            let _ = EnumDisplayMonitors(
                None,
                None,
                Some(callback),
                LPARAM((&raw mut collector) as isize),
            );
        }

        let mut out = Vec::with_capacity(collector.others.len() + 1);
        if let Some(primary) = collector.primary {
            out.push(primary);
        }
        out.extend(collector.others);
        out
    }
}

/// Other platforms have no native control: the window stays where and as GPUI
/// opened it.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
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

#[cfg(target_os = "windows")]
pub use windows_impl::{OverlayWindow, displays};

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub use fallback::{OverlayWindow, cursor_position, displays};
