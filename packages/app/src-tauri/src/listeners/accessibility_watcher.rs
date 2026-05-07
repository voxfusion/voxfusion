use std::os::raw::c_void;
use std::sync::OnceLock;

use core_foundation::base::TCFType;
use core_foundation::string::CFString;
use core_foundation_sys::dictionary::CFDictionaryRef;
use core_foundation_sys::notification_center::{
    CFNotificationCenterAddObserver, CFNotificationCenterGetDistributedCenter,
    CFNotificationCenterRef, CFNotificationName,
    CFNotificationSuspensionBehaviorDeliverImmediately,
};
use tauri::Emitter;

static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

extern "C" fn on_accessibility_changed(
    _center: CFNotificationCenterRef,
    _observer: *mut c_void,
    _name: CFNotificationName,
    _object: *const c_void,
    _user_info: CFDictionaryRef,
) {
    if let Some(handle) = APP_HANDLE.get() {
        let _ = handle.emit("accessibility-changed", ());
    }
}

pub fn setup(app_handle: &tauri::AppHandle) {
    APP_HANDLE.set(app_handle.clone()).ok();

    unsafe {
        let center = CFNotificationCenterGetDistributedCenter();
        let name = CFString::new("com.apple.accessibility.api");
        CFNotificationCenterAddObserver(
            center,
            std::ptr::null(),
            on_accessibility_changed,
            name.as_concrete_TypeRef(),
            std::ptr::null(),
            CFNotificationSuspensionBehaviorDeliverImmediately,
        );
        // The observer retains the name, but we must keep it alive
        std::mem::forget(name);
    }
}
