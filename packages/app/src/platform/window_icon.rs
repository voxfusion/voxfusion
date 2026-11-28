//! Native window icon for platforms whose titlebar and taskbar icon come
//! from the HWND rather than an application bundle.

use raw_window_handle::HasWindowHandle;

#[cfg(target_os = "windows")]
mod windows_impl {
    use std::ffi::c_void;
    use std::sync::OnceLock;

    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::Graphics::Dwm::{
        DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWINDOWATTRIBUTE, DwmSetWindowAttribute,
    };
    use windows::Win32::Graphics::Gdi::{
        BI_BITFIELDS, BITMAPINFO, BITMAPV5HEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS,
        DeleteObject, HGDIOBJ,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateIconIndirect, HICON, ICON_BIG, ICON_SMALL, ICONINFO, IsWindow, SWP_FRAMECHANGED,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SendMessageW, SetWindowPos,
        WM_SETICON,
    };
    use windows_core::BOOL;

    /// Pre-2004 Windows 10 (builds 18362/18363) used attribute 19 before
    /// `DWMWA_USE_IMMERSIVE_DARK_MODE` stabilized at 20 in build 19041.
    const DWMWA_USE_IMMERSIVE_DARK_MODE_BEFORE_20H1: DWMWINDOWATTRIBUTE = DWMWINDOWATTRIBUTE(19);

    struct WindowIcons {
        small: usize,
        big: usize,
    }

    static ICONS: OnceLock<Option<WindowIcons>> = OnceLock::new();

    fn decode_app_icon() -> Option<(Vec<u8>, u32, u32)> {
        let bytes = include_bytes!("../../assets/images/app-icon.png");
        let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder.read_info().ok()?;
        let mut buffer = vec![0u8; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buffer).ok()?;
        buffer.truncate(info.buffer_size());

        let rgba = match info.color_type {
            png::ColorType::Rgba => buffer,
            png::ColorType::Rgb => buffer
                .chunks_exact(3)
                .flat_map(|rgb| [rgb[0], rgb[1], rgb[2], 255])
                .collect(),
            _ => return None,
        };
        Some((rgba, info.width, info.height))
    }

    fn downsample_rgba(src: &[u8], src_w: u32, src_h: u32, dst_size: u32) -> Vec<u8> {
        let mut out = vec![0u8; (dst_size * dst_size * 4) as usize];
        for dy in 0..dst_size {
            let y0 = (dy * src_h) / dst_size;
            let y1 = (((dy + 1) * src_h) / dst_size).max(y0 + 1).min(src_h);
            for dx in 0..dst_size {
                let x0 = (dx * src_w) / dst_size;
                let x1 = (((dx + 1) * src_w) / dst_size).max(x0 + 1).min(src_w);
                let (mut r, mut g, mut b, mut a, mut count) = (0u64, 0u64, 0u64, 0u64, 0u64);
                for sy in y0..y1 {
                    for sx in x0..x1 {
                        let idx = ((sy * src_w + sx) * 4) as usize;
                        r += src[idx] as u64;
                        g += src[idx + 1] as u64;
                        b += src[idx + 2] as u64;
                        a += src[idx + 3] as u64;
                        count += 1;
                    }
                }
                let dst = ((dy * dst_size + dx) * 4) as usize;
                out[dst] = (r / count) as u8;
                out[dst + 1] = (g / count) as u8;
                out[dst + 2] = (b / count) as u8;
                out[dst + 3] = (a / count) as u8;
            }
        }
        out
    }

    fn create_hicon(rgba: &[u8], size: u32) -> Option<HICON> {
        let mut header = BITMAPV5HEADER::default();
        header.bV5Size = std::mem::size_of::<BITMAPV5HEADER>() as u32;
        header.bV5Width = size as i32;
        header.bV5Height = -(size as i32);
        header.bV5Planes = 1;
        header.bV5BitCount = 32;
        header.bV5Compression = BI_BITFIELDS;
        header.bV5RedMask = 0x00FF_0000;
        header.bV5GreenMask = 0x0000_FF00;
        header.bV5BlueMask = 0x0000_00FF;
        header.bV5AlphaMask = 0xFF00_0000;

        let mut bits: *mut c_void = std::ptr::null_mut();
        let color = unsafe {
            CreateDIBSection(
                None,
                (&raw const header).cast::<BITMAPINFO>(),
                DIB_RGB_COLORS,
                &mut bits,
                None,
                0,
            )
            .ok()?
        };
        if bits.is_null() {
            unsafe {
                let _ = DeleteObject(HGDIOBJ(color.0));
            }
            return None;
        }

        let pixel_count = (size * size) as usize;
        let dst = unsafe { std::slice::from_raw_parts_mut(bits.cast::<u8>(), pixel_count * 4) };
        for i in 0..pixel_count {
            let s = i * 4;
            dst[s] = rgba[s + 2];
            dst[s + 1] = rgba[s + 1];
            dst[s + 2] = rgba[s];
            dst[s + 3] = rgba[s + 3];
        }

        let mask = unsafe { CreateBitmap(size as i32, size as i32, 1, 1, None) };
        let icon = if mask.is_invalid() {
            None
        } else {
            let info = ICONINFO {
                fIcon: BOOL(1),
                xHotspot: 0,
                yHotspot: 0,
                hbmMask: mask,
                hbmColor: color,
            };
            let result = unsafe { CreateIconIndirect(&info).ok() };
            unsafe {
                let _ = DeleteObject(HGDIOBJ(mask.0));
            }
            result
        };

        unsafe {
            let _ = DeleteObject(HGDIOBJ(color.0));
        }
        icon
    }

    fn cached_icons() -> Option<&'static WindowIcons> {
        ICONS
            .get_or_init(|| {
                let (rgba, width, height) = decode_app_icon()?;
                let small_px = downsample_rgba(&rgba, width, height, 16);
                let big_px = downsample_rgba(&rgba, width, height, 32);
                let small = create_hicon(&small_px, 16)?;
                let big = create_hicon(&big_px, 32)?;
                Some(WindowIcons {
                    small: small.0 as usize,
                    big: big.0 as usize,
                })
            })
            .as_ref()
    }

    pub fn raw_hwnd(handle: &impl HasWindowHandle) -> Option<usize> {
        let RawWindowHandle::Win32(handle) = handle.window_handle().ok()?.as_raw() else {
            return None;
        };
        Some(handle.hwnd.get() as usize)
    }

    pub fn apply_to_hwnd(hwnd: usize) {
        let Some(icons) = cached_icons() else {
            return;
        };
        let hwnd = HWND(hwnd as *mut c_void);
        unsafe {
            if !IsWindow(Some(hwnd)).as_bool() {
                return;
            }
            let _ = SendMessageW(
                hwnd,
                WM_SETICON,
                Some(WPARAM(ICON_SMALL as usize)),
                Some(LPARAM(icons.small as isize)),
            );
            let _ = SendMessageW(
                hwnd,
                WM_SETICON,
                Some(WPARAM(ICON_BIG as usize)),
                Some(LPARAM(icons.big as isize)),
            );
        }
    }

    /// Matches the native Windows 10/11 titlebar to the app's active light/dark
    /// theme. Call from an async task rather than synchronously inside a GPUI
    /// update, because `SWP_FRAMECHANGED` re-enters `WndProc`.
    pub fn set_dark_titlebar(hwnd: usize, dark: bool) {
        let hwnd = HWND(hwnd as *mut c_void);
        let value = BOOL(i32::from(dark));
        unsafe {
            if !IsWindow(Some(hwnd)).as_bool() {
                return;
            }
            if DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                (&raw const value).cast(),
                std::mem::size_of_val(&value) as u32,
            )
            .is_err()
            {
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_USE_IMMERSIVE_DARK_MODE_BEFORE_20H1,
                    (&raw const value).cast(),
                    std::mem::size_of_val(&value) as u32,
                );
            }
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
    }
}

#[cfg(target_os = "windows")]
pub fn raw_hwnd(handle: &impl HasWindowHandle) -> Option<usize> {
    windows_impl::raw_hwnd(handle)
}

#[cfg(target_os = "windows")]
pub fn apply_to_hwnd(hwnd: usize) {
    windows_impl::apply_to_hwnd(hwnd);
}

#[cfg(target_os = "windows")]
pub fn set_dark_titlebar(hwnd: usize, dark: bool) {
    windows_impl::set_dark_titlebar(hwnd, dark);
}

#[cfg(not(target_os = "windows"))]
pub fn raw_hwnd(_handle: &impl HasWindowHandle) -> Option<usize> {
    None
}

#[cfg(not(target_os = "windows"))]
pub fn apply_to_hwnd(_hwnd: usize) {}

#[cfg(not(target_os = "windows"))]
pub fn set_dark_titlebar(_hwnd: usize, _dark: bool) {}
