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

/// Whether the app may record from the microphone, without asking.
pub fn microphone_permission() -> PermissionState {
    #[cfg(target_os = "macos")]
    return macos::microphone_permission();

    #[cfg(not(target_os = "macos"))]
    PermissionState::Granted
}

/// Asks for the microphone if macOS has not asked yet, and waits for the
/// answer. Returns whether the app may record.
pub fn request_microphone_permission() -> bool {
    #[cfg(target_os = "macos")]
    return macos::request_microphone_permission();

    #[cfg(not(target_os = "macos"))]
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
