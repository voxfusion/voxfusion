//! How the app presents itself to macOS: as a menu bar app without a Dock
//! icon, whose windows still come to the front when asked.

/// Keeps VoxFusion out of the Dock and the app switcher.
///
/// `LSUIElement` in Info.plist asks for this, but GPUI sets the regular
/// activation policy when the app finishes launching, which overrides it.
/// Call this from the launch callback, right after GPUI has done so: the
/// Dock then never gets to show the icon.
#[cfg(target_os = "macos")]
pub fn become_menu_bar_app() {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

    let Some(main_thread) = MainThreadMarker::new() else {
        log::error!(target: "runtime", "activation_policy_not_on_main_thread");
        return;
    };

    let app = NSApplication::sharedApplication(main_thread);
    if !app.setActivationPolicy(NSApplicationActivationPolicy::Accessory) {
        log::warn!(target: "runtime", "activation_policy_rejected");
    }
}

#[cfg(not(target_os = "macos"))]
pub fn become_menu_bar_app() {}
