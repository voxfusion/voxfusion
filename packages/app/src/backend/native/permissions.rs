//! The microphone and Accessibility permissions, as macOS holds them for the
//! app. Other platforms ask for neither.

use crate::backend::PermissionState;

#[cfg(target_os = "macos")]
mod macos {
    use block2::RcBlock;
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::string::{CFString, CFStringRef};
    use objc2::runtime::Bool;
    use objc2_av_foundation::{
        AVAuthorizationStatus, AVCaptureDevice, AVMediaType, AVMediaTypeAudio,
    };

    use crate::backend::PermissionState;

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrusted() -> bool;

        fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;

        static kAXTrustedCheckOptionPrompt: CFStringRef;
    }

    fn audio() -> &'static AVMediaType {
        // SAFETY: AVFoundation sets the constant when it loads and never
        // changes it.
        unsafe { AVMediaTypeAudio }.expect("AVFoundation defines AVMediaTypeAudio")
    }

    pub fn microphone_permission() -> PermissionState {
        // SAFETY: audio is one of the two media types the method accepts.
        match unsafe { AVCaptureDevice::authorizationStatusForMediaType(audio()) } {
            AVAuthorizationStatus::Authorized => PermissionState::Granted,
            AVAuthorizationStatus::NotDetermined => PermissionState::Prompt,
            // Denied by the user, or restricted by a device policy the user
            // cannot change. Neither can be asked for again.
            _ => PermissionState::Denied,
        }
    }

    pub fn request_microphone_permission() -> bool {
        let (answered, answer) = std::sync::mpsc::channel();
        let handler = RcBlock::new(move |granted: Bool| {
            let _ = answered.send(granted.as_bool());
        });

        // macOS shows its prompt only while the permission is undetermined,
        // and otherwise answers at once with what was decided. It calls the
        // handler on a queue of its own, so waiting here cannot block it.
        //
        // SAFETY: audio is one of the two media types the method accepts,
        // and the block is copied by the callee for as long as it needs it.
        unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(audio(), &handler) };

        // The handler is dropped unanswered only if macOS gives up on the
        // request.
        answer.recv().unwrap_or(false)
    }

    pub fn check_accessibility() -> bool {
        // SAFETY: takes no arguments and only reads the process's trust.
        unsafe { AXIsProcessTrusted() }
    }

    pub fn request_accessibility() {
        // SAFETY: the key is a constant string that ApplicationServices
        // exports for the lifetime of the process.
        let prompt = unsafe { CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt) };
        let options = CFDictionary::from_CFType_pairs(&[(prompt, CFBoolean::true_value())]);

        // The answer is the current trust, which `check_accessibility` and
        // the accessibility watcher report once the user changes it.
        //
        // SAFETY: `options` is a valid dictionary that outlives the call.
        unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) };
    }
}

#[cfg(target_os = "windows")]
mod windows_impl {
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, REG_SZ, REG_VALUE_TYPE,
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW,
    };
    use windows::core::PCWSTR;

    use crate::backend::PermissionState;

    fn read_registry_string(root: HKEY, subkey: &str, value_name: &str) -> Option<String> {
        let subkey_wide: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
        let value_wide: Vec<u16> = value_name
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        let mut hkey = HKEY::default();
        let open_status = unsafe {
            RegOpenKeyExW(
                root,
                PCWSTR::from_raw(subkey_wide.as_ptr()),
                Some(0),
                KEY_READ,
                &mut hkey,
            )
        };
        if open_status.is_err() {
            return None;
        }

        let mut value_type = REG_VALUE_TYPE::default();
        let mut size: u32 = 0;
        let query_len = unsafe {
            RegQueryValueExW(
                hkey,
                PCWSTR::from_raw(value_wide.as_ptr()),
                None,
                Some(&mut value_type),
                None,
                Some(&mut size),
            )
        };
        if query_len.is_err() || value_type != REG_SZ || size == 0 {
            unsafe {
                let _ = RegCloseKey(hkey);
            }
            return None;
        }

        let mut buf = vec![0u8; size as usize];
        let query = unsafe {
            RegQueryValueExW(
                hkey,
                PCWSTR::from_raw(value_wide.as_ptr()),
                None,
                Some(&mut value_type),
                Some(buf.as_mut_ptr()),
                Some(&mut size),
            )
        };
        unsafe {
            let _ = RegCloseKey(hkey);
        }
        if query.is_err() || value_type != REG_SZ {
            return None;
        }

        let units: Vec<u16> = buf[..size as usize]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let end = units.iter().position(|&c| c == 0).unwrap_or(units.len());
        Some(String::from_utf16_lossy(&units[..end]))
    }

    fn is_denied(root: HKEY, subkey: &str) -> bool {
        read_registry_string(root, subkey, "Value")
            .is_some_and(|v| v.trim().eq_ignore_ascii_case("Deny"))
    }

    pub fn microphone_permission() -> PermissionState {
        const MICROPHONE_KEY: &str =
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";
        const NON_PACKAGED_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone\NonPackaged";

        if is_denied(HKEY_LOCAL_MACHINE, MICROPHONE_KEY)
            || is_denied(HKEY_CURRENT_USER, MICROPHONE_KEY)
            || is_denied(HKEY_CURRENT_USER, NON_PACKAGED_KEY)
        {
            PermissionState::Denied
        } else {
            PermissionState::Granted
        }
    }

    pub fn request_microphone_permission() -> bool {
        microphone_permission() == PermissionState::Granted
    }
}

/// Whether the app may record from the microphone, without asking.
pub fn microphone_permission() -> PermissionState {
    #[cfg(target_os = "macos")]
    return macos::microphone_permission();

    #[cfg(target_os = "windows")]
    return windows_impl::microphone_permission();

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    PermissionState::Granted
}

/// Asks for the microphone if macOS has not asked yet, and waits for the
/// answer. Returns whether the app may record.
pub fn request_microphone_permission() -> bool {
    #[cfg(target_os = "macos")]
    return macos::request_microphone_permission();

    #[cfg(target_os = "windows")]
    return windows_impl::request_microphone_permission();

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    true
}

/// Whether the app may type into other apps.
pub fn check_accessibility() -> bool {
    #[cfg(target_os = "macos")]
    return macos::check_accessibility();

    #[cfg(not(target_os = "macos"))]
    true
}

/// Adds the app to the Accessibility list in System Settings, switched off,
/// and shows the system alert that leads there. macOS has no prompt that
/// grants this permission directly. Does nothing once the app is trusted.
pub fn request_accessibility() {
    #[cfg(target_os = "macos")]
    macos::request_accessibility();
}
