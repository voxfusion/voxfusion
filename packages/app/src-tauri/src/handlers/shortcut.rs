#[cfg(target_os = "macos")]
mod macos {
    use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
    use core_graphics::event::{
        CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
        CGEventType, EventField,
    };
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Arc, Mutex, OnceLock};

    // macOS virtual keycodes for modifier keys
    pub const KC_LEFT_COMMAND: u16 = 55;
    pub const KC_RIGHT_COMMAND: u16 = 54;
    pub const KC_LEFT_SHIFT: u16 = 56;
    pub const KC_RIGHT_SHIFT: u16 = 60;
    pub const KC_LEFT_OPTION: u16 = 58;
    pub const KC_RIGHT_OPTION: u16 = 61;
    pub const KC_LEFT_CONTROL: u16 = 59;
    pub const KC_RIGHT_CONTROL: u16 = 62;
    pub const KC_CAPS_LOCK: u16 = 57;
    pub const KC_FN: u16 = 63;

    /// Parse a shortcut string into target flag mask and optional specific keycode.
    ///
    /// Single modifier shortcuts like "RightCommand" target a specific keycode.
    /// Multi-modifier shortcuts like "Control+Shift" target a combined flag mask.
    ///
    /// Returns (target_flags_mask, optional_specific_keycode)
    fn parse_shortcut(name: &str) -> Option<(u64, u16)> {
        // Try single modifier first
        match name {
            "LeftCommand" => Some((CGEventFlags::CGEventFlagCommand.bits(), KC_LEFT_COMMAND)),
            "RightCommand" => Some((CGEventFlags::CGEventFlagCommand.bits(), KC_RIGHT_COMMAND)),
            "LeftShift" => Some((CGEventFlags::CGEventFlagShift.bits(), KC_LEFT_SHIFT)),
            "RightShift" => Some((CGEventFlags::CGEventFlagShift.bits(), KC_RIGHT_SHIFT)),
            "LeftOption" => Some((CGEventFlags::CGEventFlagAlternate.bits(), KC_LEFT_OPTION)),
            "RightOption" => Some((CGEventFlags::CGEventFlagAlternate.bits(), KC_RIGHT_OPTION)),
            "LeftControl" => Some((CGEventFlags::CGEventFlagControl.bits(), KC_LEFT_CONTROL)),
            "RightControl" => Some((CGEventFlags::CGEventFlagControl.bits(), KC_RIGHT_CONTROL)),
            "CapsLock" => Some((CGEventFlags::CGEventFlagAlphaShift.bits(), KC_CAPS_LOCK)),
            "Fn" => Some((CGEventFlags::CGEventFlagSecondaryFn.bits(), KC_FN)),
            _ => {
                // Try multi-modifier combo like "Control+Shift"
                if name.contains('+') {
                    let mut mask: u64 = 0;
                    for part in name.split('+') {
                        let flag = match part.trim() {
                            "Command" => CGEventFlags::CGEventFlagCommand.bits(),
                            "Control" => CGEventFlags::CGEventFlagControl.bits(),
                            "Alt" | "Option" => CGEventFlags::CGEventFlagAlternate.bits(),
                            "Shift" => CGEventFlags::CGEventFlagShift.bits(),
                            "CapsLock" => CGEventFlags::CGEventFlagAlphaShift.bits(),
                            "Fn" => CGEventFlags::CGEventFlagSecondaryFn.bits(),
                            _ => return None,
                        };
                        mask |= flag;
                    }
                    // For multi-modifier, keycode 0 means "don't check specific keycode"
                    Some((mask, 0))
                } else {
                    None
                }
            }
        }
    }

    fn is_modifier_keycode(keycode: u16) -> bool {
        matches!(
            keycode,
            KC_LEFT_COMMAND
                | KC_RIGHT_COMMAND
                | KC_LEFT_SHIFT
                | KC_RIGHT_SHIFT
                | KC_LEFT_OPTION
                | KC_RIGHT_OPTION
                | KC_LEFT_CONTROL
                | KC_RIGHT_CONTROL
                | KC_CAPS_LOCK
                | KC_FN
        )
    }

    /// All modifier flag bits combined, used to mask out non-modifier flags.
    const ALL_MODIFIER_FLAGS: u64 = 0x00010000  // AlphaShift (CapsLock)
        | 0x00020000  // Shift
        | 0x00040000  // Control
        | 0x00080000  // Alternate (Option)
        | 0x00100000  // Command
        | 0x00800000; // SecondaryFn

    struct ShortcutState {
        /// The flag mask we're watching for (0 means not active)
        target_flags: AtomicU64,
        /// Specific keycode for single-modifier shortcuts (0 means multi-modifier)
        target_keycode: AtomicU64,
        /// Whether the target modifiers are all currently held down RIGHT NOW
        modifiers_active: AtomicBool,
        /// Whether the full combo was active at some point during this press cycle.
        /// Survives partial releases so we can fire when all keys are fully released.
        was_fully_active: AtomicBool,
        /// Whether any non-target key was pressed while the modifiers were held
        contaminated: AtomicBool,
        /// Whether the event tap thread is running
        running: AtomicBool,
        /// The Tauri app handle for emitting events
        app_handle: Mutex<Option<tauri::AppHandle>>,
    }

    static STATE: OnceLock<Arc<ShortcutState>> = OnceLock::new();

    fn get_state() -> &'static Arc<ShortcutState> {
        STATE.get_or_init(|| {
            Arc::new(ShortcutState {
                target_flags: AtomicU64::new(0),
                target_keycode: AtomicU64::new(0),
                modifiers_active: AtomicBool::new(false),
                was_fully_active: AtomicBool::new(false),
                contaminated: AtomicBool::new(false),
                running: AtomicBool::new(false),
                app_handle: Mutex::new(None),
            })
        })
    }

    fn start_event_tap(state: Arc<ShortcutState>) {
        if state.running.swap(true, Ordering::SeqCst) {
            return; // Already running
        }

        std::thread::spawn(move || {
            let state_for_tap = state.clone();

            // Listen for FlagsChanged (modifier press/release) and KeyDown (contamination)
            let tap_result = CGEventTap::new(
                CGEventTapLocation::HID,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::ListenOnly,
                vec![CGEventType::FlagsChanged, CGEventType::KeyDown],
                move |_proxy, event_type, event| {
                    let keycode =
                        event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;

                    match event_type {
                        CGEventType::FlagsChanged => {
                            let target_mask = state_for_tap.target_flags.load(Ordering::SeqCst);
                            if target_mask == 0 {
                                return Some(event.clone());
                            }

                            let target_kc =
                                state_for_tap.target_keycode.load(Ordering::SeqCst) as u16;

                            let flags = event.get_flags();
                            let raw_flags = flags.bits();
                            // Only look at modifier bits
                            let current_mods = raw_flags & ALL_MODIFIER_FLAGS;

                            if target_kc != 0 {
                                // Single-modifier mode: check specific keycode
                                if keycode == target_kc {
                                    let has_target = (current_mods & target_mask) == target_mask;
                                    // Also check no OTHER modifiers are held
                                    let only_target =
                                        has_target && (current_mods & !target_mask) == 0;

                                    if only_target {
                                        // Target modifier pressed (alone)
                                        state_for_tap
                                            .modifiers_active
                                            .store(true, Ordering::SeqCst);
                                        state_for_tap.contaminated.store(false, Ordering::SeqCst);
                                    } else if !has_target
                                        && state_for_tap.modifiers_active.load(Ordering::SeqCst)
                                    {
                                        // Target modifier released
                                        let was_active = state_for_tap
                                            .modifiers_active
                                            .swap(false, Ordering::SeqCst);
                                        let was_contaminated =
                                            state_for_tap.contaminated.load(Ordering::SeqCst);

                                        if was_active && !was_contaminated {
                                            fire_shortcut(&state_for_tap);
                                        }
                                    }
                                } else if state_for_tap.modifiers_active.load(Ordering::SeqCst)
                                    && is_modifier_keycode(keycode)
                                {
                                    // A different modifier changed — contaminate
                                    state_for_tap.contaminated.store(true, Ordering::SeqCst);
                                }
                            } else {
                                // Multi-modifier mode: check if all target flags are set
                                let all_target_set = (current_mods & target_mask) == target_mask;
                                // Ensure ONLY the target modifiers are held (no extras)
                                let only_target =
                                    all_target_set && (current_mods & !target_mask) == 0;

                                if only_target {
                                    // All target modifiers are held simultaneously (no extras)
                                    state_for_tap.modifiers_active.store(true, Ordering::SeqCst);
                                    state_for_tap.was_fully_active.store(true, Ordering::SeqCst);
                                    state_for_tap.contaminated.store(false, Ordering::SeqCst);
                                } else if current_mods == 0 {
                                    // All modifiers fully released — check if we should fire
                                    state_for_tap
                                        .modifiers_active
                                        .store(false, Ordering::SeqCst);
                                    let was_active = state_for_tap
                                        .was_fully_active
                                        .swap(false, Ordering::SeqCst);
                                    let was_contaminated =
                                        state_for_tap.contaminated.load(Ordering::SeqCst);

                                    if was_active && !was_contaminated {
                                        fire_shortcut(&state_for_tap);
                                    }
                                } else {
                                    // Partial state: some modifiers held but not the right combo
                                    state_for_tap
                                        .modifiers_active
                                        .store(false, Ordering::SeqCst);
                                    if state_for_tap.was_fully_active.load(Ordering::SeqCst) {
                                        // Extra modifier added or wrong combo while releasing
                                        if (current_mods & !target_mask) != 0 {
                                            state_for_tap
                                                .contaminated
                                                .store(true, Ordering::SeqCst);
                                        }
                                    }
                                }
                            }
                        }
                        CGEventType::KeyDown => {
                            // Any regular key pressed while modifiers are held — contaminate
                            if state_for_tap.modifiers_active.load(Ordering::SeqCst) {
                                state_for_tap.contaminated.store(true, Ordering::SeqCst);
                            }
                        }
                        _ => {}
                    }

                    Some(event.clone())
                },
            );

            match tap_result {
                Ok(tap) => unsafe {
                    // CRITICAL: Add the tap's mach port as a run loop source.
                    // Without this, the run loop has no sources and events are
                    // never delivered to our callback.
                    let loop_source = tap
                        .mach_port
                        .create_runloop_source(0)
                        .expect("Failed to create run loop source for CGEventTap");
                    CFRunLoop::get_current().add_source(&loop_source, kCFRunLoopCommonModes);
                    tap.enable();
                    CFRunLoop::run_current();
                },
                Err(e) => {
                    eprintln!(
                        "Failed to create CGEventTap for modifier shortcuts: {:?}",
                        e
                    );
                    state.running.store(false, Ordering::SeqCst);
                }
            }
        });
    }

    fn fire_shortcut(state: &ShortcutState) {
        if let Ok(handle) = state.app_handle.lock() {
            if let Some(ref app) = *handle {
                use tauri::Emitter;
                let _ = app.emit(
                    "modifier-shortcut-triggered",
                    serde_json::json!({ "state": "Pressed" }),
                );
            }
        }
    }

    pub fn register(app_handle: tauri::AppHandle, shortcut: &str) -> Result<(), String> {
        let (flags_mask, keycode) = parse_shortcut(shortcut)
            .ok_or_else(|| format!("Unknown modifier shortcut: {}", shortcut))?;

        let state = get_state();

        // Store the app handle
        if let Ok(mut handle) = state.app_handle.lock() {
            *handle = Some(app_handle);
        }

        // Set the targets
        state.target_flags.store(flags_mask, Ordering::SeqCst);
        state.target_keycode.store(keycode as u64, Ordering::SeqCst);
        state.modifiers_active.store(false, Ordering::SeqCst);
        state.was_fully_active.store(false, Ordering::SeqCst);
        state.contaminated.store(false, Ordering::SeqCst);

        // Start the event tap if not already running
        start_event_tap(state.clone());

        Ok(())
    }

    pub fn unregister() {
        let state = get_state();
        state.target_flags.store(0, Ordering::SeqCst);
        state.target_keycode.store(0, Ordering::SeqCst);
        state.modifiers_active.store(false, Ordering::SeqCst);
        state.was_fully_active.store(false, Ordering::SeqCst);
        state.contaminated.store(false, Ordering::SeqCst);
    }
}

#[tauri::command]
pub fn register_modifier_shortcut(
    app_handle: tauri::AppHandle,
    shortcut: String,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::register(app_handle, &shortcut)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app_handle;
        let _ = shortcut;
        Err("Modifier-only shortcuts are only supported on macOS".to_string())
    }
}

#[tauri::command]
pub fn unregister_modifier_shortcut() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::unregister();
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(())
    }
}
