#[cfg(not(target_os = "windows"))]
use enigo::{Enigo, Keyboard, Settings};

#[cfg(not(target_os = "windows"))]
use crate::platform::main_thread;

/// `dwExtraInfo` tag placed on keyboard events synthesized by [`type_text`] on
/// Windows so the low-level key watcher can ignore our own typed output.
#[cfg(target_os = "windows")]
pub(crate) const VOXFUSION_INPUT_MARKER: usize = 0x564F_5846;

/// Types `text` into the focused app as keyboard input, which macOS only
/// lets through with the Accessibility permission.
#[cfg(not(target_os = "windows"))]
pub fn type_text(text: &str) -> Result<(), String> {
    // On the main thread: enigo's macOS backend looks keys up through Text
    // Input Sources, which abort the process on any other thread. `text`
    // avoids those lookups today, but only by how enigo happens to be
    // written.
    main_thread::run(|| {
        let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
        enigo.text(text).map_err(|e| e.to_string())?;
        Ok(())
    })
}

/// Types `text` into the focused Windows application via `SendInput`.
///
/// Uses `KEYEVENTF_UNICODE` by default (immune to Caps Lock, dead keys, and
/// per-window keyboard layouts). Classic Windows console windows
/// (`ConsoleWindowClass`, including PowerShell's `PSReadLine` and `cmd.exe`)
/// ignore `KEYEVENTF_UNICODE` events with `wVk == 0`, so characters targeted
/// at a console window are posted as `WM_CHAR` UTF-16 code units, which
/// `conhost` translates directly into console Unicode input events for ASCII,
/// AltGr, and non-ASCII text alike.
#[cfg(target_os = "windows")]
pub fn type_text(text: &str) -> Result<(), String> {
    use std::time::Duration;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP,
        KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC, MapVirtualKeyW, SendInput, VIRTUAL_KEY, VK_RETURN,
        VK_TAB,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetForegroundWindow, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_CHAR,
        WM_KEYDOWN, WM_KEYUP,
    };

    if text.is_empty() {
        return Ok(());
    }

    const MAX_INPUTS_PER_BATCH: usize = 64;

    let fg = unsafe { GetForegroundWindow() };
    if !fg.0.is_null() && foreground_is_elevated_above_self(fg) {
        return Err(
            "Cannot type into an elevated Administrator window (blocked by Windows UIPI)".to_string(),
        );
    }

    // Detect classic conhost (`ConsoleWindowClass`), where PSReadLine discards
    // `KEYEVENTF_UNICODE` (`wVk == 0`) packets.
    let console_hwnd = unsafe {
        if !fg.0.is_null() {
            let mut cls = [0u16; 64];
            let len = GetClassNameW(fg, &mut cls);
            let name = String::from_utf16_lossy(&cls[..len as usize]);
            (name == "ConsoleWindowClass").then_some(fg)
        } else {
            None
        }
    };

    let make_vk = |vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: VOXFUSION_INPUT_MARKER,
            },
        },
    };

    let make_unicode = |unit: u16, key_up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: unit,
                dwFlags: if key_up {
                    KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
                } else {
                    KEYEVENTF_UNICODE
                },
                time: 0,
                dwExtraInfo: VOXFUSION_INPUT_MARKER,
            },
        },
    };

    let flush = |batch: &mut Vec<INPUT>| -> Result<(), String> {
        if batch.is_empty() {
            return Ok(());
        }
        let sent = unsafe { SendInput(batch, std::mem::size_of::<INPUT>() as i32) };
        let expected = batch.len() as u32;
        batch.clear();
        if sent != expected {
            let err = std::io::Error::last_os_error();
            return Err(format!(
                "SendInput delivered {sent}/{expected} events (target window may be elevated / blocked by UIPI): {err}"
            ));
        }
        Ok(())
    };

    // Send console input synchronously via `SendMessageTimeoutW` so every
    // character and Enter/Tab separator is processed in strict call order
    // directly by conhost's window procedure without `PostMessageW` /
    // `TranslateMessage` queue reordering. Sending `WM_KEYDOWN` + `WM_CHAR` +
    // `WM_KEYUP` satisfies both PowerShell's PSReadLine and `cmd.exe`.
    const CONSOLE_SEND_TIMEOUT_MS: u32 = 500;
    let send_console_msg = |hwnd: windows::Win32::Foundation::HWND,
                            msg: u32,
                            wparam: WPARAM,
                            lparam: LPARAM|
     -> Result<(), String> {
        let ok = unsafe {
            SendMessageTimeoutW(
                hwnd,
                msg,
                wparam,
                lparam,
                SMTO_ABORTIFHUNG,
                CONSOLE_SEND_TIMEOUT_MS,
                None,
            )
        };
        if ok.0 == 0 {
            let err = std::io::Error::last_os_error();
            return Err(format!("Failed to send console input message: {err}"));
        }
        Ok(())
    };
    let send_console_vk =
        |hwnd: windows::Win32::Foundation::HWND, vk: VIRTUAL_KEY, ch: char| -> Result<(), String> {
            let scan = unsafe { MapVirtualKeyW(u32::from(vk.0), MAPVK_VK_TO_VSC) } as usize;
            let down_lparam = LPARAM((1usize | (scan << 16)) as isize);
            let up_lparam =
                LPARAM((1usize | (scan << 16) | (1usize << 30) | (1usize << 31)) as isize);
            send_console_msg(hwnd, WM_KEYDOWN, WPARAM(usize::from(vk.0)), down_lparam)?;
            send_console_msg(hwnd, WM_CHAR, WPARAM(ch as usize), down_lparam)?;
            send_console_msg(hwnd, WM_KEYUP, WPARAM(usize::from(vk.0)), up_lparam)?;
            Ok(())
        };

    let mut batch: Vec<INPUT> = Vec::with_capacity(MAX_INPUTS_PER_BATCH);
    let mut utf16 = [0u16; 2];
    let mut prev_console_unit: Option<u16> = None;

    for ch in text.chars() {
        match ch {
            '\0' => return Err("The text contained a NUL byte".to_string()),
            '\r' => continue,
            '\n' => {
                if let Some(hwnd) = console_hwnd {
                    send_console_vk(hwnd, VK_RETURN, '\r')?;
                    prev_console_unit = None;
                    std::thread::sleep(Duration::from_millis(2));
                    continue;
                }
                flush(&mut batch)?;
                batch.push(make_vk(VK_RETURN, KEYBD_EVENT_FLAGS(0)));
                batch.push(make_vk(VK_RETURN, KEYEVENTF_KEYUP));
                flush(&mut batch)?;
                std::thread::sleep(Duration::from_millis(2));
                continue;
            }
            '\t' => {
                if let Some(hwnd) = console_hwnd {
                    send_console_vk(hwnd, VK_TAB, '\t')?;
                    prev_console_unit = None;
                    std::thread::sleep(Duration::from_millis(2));
                    continue;
                }
                flush(&mut batch)?;
                batch.push(make_vk(VK_TAB, KEYBD_EVENT_FLAGS(0)));
                batch.push(make_vk(VK_TAB, KEYEVENTF_KEYUP));
                flush(&mut batch)?;
                std::thread::sleep(Duration::from_millis(2));
                continue;
            }
            _ => {}
        }

        let units = ch.encode_utf16(&mut utf16);

        if let Some(hwnd) = console_hwnd {
            for &unit in units.iter() {
                // Classic conhost + PSReadLine miscount coalesced consecutive
                // identical WM_CHAR records if the previous record is still
                // unread in the console input buffer; give the reader a moment
                // to drain before sending an identical consecutive code unit.
                if prev_console_unit == Some(unit) {
                    std::thread::sleep(Duration::from_millis(10));
                }
                send_console_msg(hwnd, WM_CHAR, WPARAM(unit as usize), LPARAM(1))?;
                prev_console_unit = Some(unit);
                std::thread::sleep(Duration::from_millis(2));
            }
            continue;
        }

        if batch.len() + units.len() * 2 > MAX_INPUTS_PER_BATCH {
            flush(&mut batch)?;
            std::thread::sleep(Duration::from_millis(2));
        }

        for &unit in units.iter() {
            batch.push(make_unicode(unit, false));
            batch.push(make_unicode(unit, true));
        }
    }

    flush(&mut batch)
}

/// Checks whether the foreground window belongs to a process running at a
/// higher integrity level than VoxFusion. On Windows, UIPI causes `SendInput`
/// to silently drop events targeted at an elevated window while still returning
/// the full event count.
#[cfg(target_os = "windows")]
fn foreground_is_elevated_above_self(fg: windows::Win32::Foundation::HWND) -> bool {
    use windows::Win32::Foundation::{CloseHandle, ERROR_ACCESS_DENIED, HANDLE};
    use windows::Win32::Security::{
        GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TOKEN_MANDATORY_LABEL,
        TOKEN_QUERY, TokenIntegrityLevel,
    };
    use windows::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

    const HIGH_INTEGRITY_RID: u32 = 0x3000;

    unsafe fn token_integrity_rid(token: HANDLE) -> Option<u32> {
        let mut needed: u32 = 0;
        let _ = unsafe { GetTokenInformation(token, TokenIntegrityLevel, None, 0, &mut needed) };
        if needed == 0 {
            return None;
        }
        let mut buf = vec![0u8; needed as usize];
        unsafe {
            GetTokenInformation(
                token,
                TokenIntegrityLevel,
                Some(buf.as_mut_ptr().cast()),
                needed,
                &mut needed,
            )
            .ok()?;
            let label = &*(buf.as_ptr() as *const TOKEN_MANDATORY_LABEL);
            let sid = label.Label.Sid;
            let count = *GetSidSubAuthorityCount(sid);
            if count == 0 {
                return None;
            }
            Some(*GetSidSubAuthority(sid, (count - 1) as u32))
        }
    }

    let mut fg_pid: u32 = 0;
    unsafe {
        GetWindowThreadProcessId(fg, Some(&mut fg_pid));
    }
    if fg_pid == 0 || fg_pid == std::process::id() {
        return false;
    }

    let mut self_token = HANDLE::default();
    let self_rid = unsafe {
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut self_token).is_err() {
            return false;
        }
        let rid = token_integrity_rid(self_token);
        let _ = CloseHandle(self_token);
        rid
    };
    let Some(self_rid) = self_rid else {
        return false;
    };
    if self_rid >= HIGH_INTEGRITY_RID {
        return false;
    }

    let proc = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, fg_pid) } {
        Ok(handle) => handle,
        Err(err) => {
            return err.code() == ERROR_ACCESS_DENIED.to_hresult();
        }
    };

    let mut target_token = HANDLE::default();
    let blocked = match unsafe { OpenProcessToken(proc, TOKEN_QUERY, &mut target_token) } {
        Ok(()) => {
            let target_rid = unsafe { token_integrity_rid(target_token) };
            unsafe {
                let _ = CloseHandle(target_token);
            }
            target_rid.is_some_and(|rid| rid > self_rid)
        }
        Err(err) => err.code() == ERROR_ACCESS_DENIED.to_hresult(),
    };
    unsafe {
        let _ = CloseHandle(proc);
    }
    blocked
}
