use rdev::{listen, Event, EventType, Key};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// All rdev keys we consider "modifier" keys.
fn is_modifier_key(key: &Key) -> bool {
    matches!(
        key,
        Key::MetaLeft
            | Key::MetaRight
            | Key::ShiftLeft
            | Key::ShiftRight
            | Key::Alt
            | Key::ControlLeft
            | Key::ControlRight
            | Key::CapsLock
            | Key::Function
    )
}

/// Parse a shortcut name (from the frontend) into a set of rdev::Key values.
///
/// Single modifier: "LeftCommand" -> {MetaLeft}
/// Multi-modifier:  "Control+Shift" -> {ControlLeft, ControlRight, ShiftLeft, ShiftRight}
///   (for generic names we accept *either* left or right being held)
///
/// Returns None if the string is not a recognized modifier shortcut.
fn parse_shortcut(name: &str) -> Option<ShortcutTarget> {
    // Single specific modifier
    let single = match name {
        "LeftCommand" => Some(ShortcutTarget::Single(Key::MetaLeft)),
        "RightCommand" => Some(ShortcutTarget::Single(Key::MetaRight)),
        "LeftShift" => Some(ShortcutTarget::Single(Key::ShiftLeft)),
        "RightShift" => Some(ShortcutTarget::Single(Key::ShiftRight)),
        "LeftOption" | "Option" | "Alt" => Some(ShortcutTarget::Single(Key::Alt)),
        "RightOption" => Some(ShortcutTarget::Single(Key::Alt)),
        "LeftControl" => Some(ShortcutTarget::Single(Key::ControlLeft)),
        "RightControl" => Some(ShortcutTarget::Single(Key::ControlRight)),
        "CapsLock" => Some(ShortcutTarget::Single(Key::CapsLock)),
        "Fn" => Some(ShortcutTarget::Single(Key::Function)),
        _ => None,
    };
    if single.is_some() {
        return single;
    }

    // Multi-modifier combo like "Control+Shift"
    if name.contains('+') {
        let mut groups: Vec<Vec<Key>> = Vec::new();
        for part in name.split('+') {
            let keys = match part.trim() {
                "Command" => vec![Key::MetaLeft, Key::MetaRight],
                "Control" => vec![Key::ControlLeft, Key::ControlRight],
                "Alt" | "Option" => vec![Key::Alt],
                "Shift" => vec![Key::ShiftLeft, Key::ShiftRight],
                "CapsLock" => vec![Key::CapsLock],
                "Fn" => vec![Key::Function],
                _ => return None,
            };
            groups.push(keys);
        }
        if groups.len() >= 2 {
            return Some(ShortcutTarget::Multi(groups));
        }
    }

    None
}

/// Describes what modifier key(s) the user wants as their shortcut.
#[derive(Clone)]
enum ShortcutTarget {
    /// Exactly one specific key (e.g. RightCommand = MetaRight).
    Single(Key),
    /// Multiple modifier groups; at least one key from each group must be held.
    /// E.g. "Control+Shift" = [[ControlLeft,ControlRight],[ShiftLeft,ShiftRight]]
    Multi(Vec<Vec<Key>>),
}

struct ShortcutState {
    /// Current target, None means not active.
    target: Mutex<Option<ShortcutTarget>>,
    /// Whether the listener thread is running.
    running: AtomicBool,
    /// The Tauri app handle for emitting events.
    app_handle: Mutex<Option<tauri::AppHandle>>,
    /// Keys currently held down (only modifier keys).
    held_keys: Mutex<HashSet<Key>>,
    /// Whether a non-modifier key was pressed during the current modifier hold.
    contaminated: AtomicBool,
}

static STATE: OnceLock<Arc<ShortcutState>> = OnceLock::new();

fn get_state() -> &'static Arc<ShortcutState> {
    STATE.get_or_init(|| {
        Arc::new(ShortcutState {
            target: Mutex::new(None),
            running: AtomicBool::new(false),
            app_handle: Mutex::new(None),
            held_keys: Mutex::new(HashSet::new()),
            contaminated: AtomicBool::new(false),
        })
    })
}

/// Check whether the currently-held modifiers exactly match the target.
fn target_is_active(target: &ShortcutTarget, held: &HashSet<Key>) -> bool {
    if held.is_empty() {
        return false;
    }

    match target {
        ShortcutTarget::Single(key) => {
            // Exactly this key and no other modifiers
            held.len() == 1 && held.contains(key)
        }
        ShortcutTarget::Multi(groups) => {
            // At least one key from each group must be held …
            let all_groups_satisfied = groups
                .iter()
                .all(|group| group.iter().any(|k| held.contains(k)));
            if !all_groups_satisfied {
                return false;
            }
            // … and every held key must belong to at least one group
            // (prevents extra modifiers from triggering the shortcut).
            let all_keys: HashSet<&Key> = groups.iter().flat_map(|g| g.iter()).collect();
            held.iter().all(|k| all_keys.contains(k))
        }
    }
}

fn fire_shortcut(state: &ShortcutState) {
    if let Ok(handle) = state.app_handle.lock() {
        if let Some(ref app) = *handle {
            use tauri::Emitter;
            if let Err(e) = app.emit(
                "modifier-shortcut-triggered",
                serde_json::json!({ "state": "Pressed" }),
            ) {
                eprintln!("Failed to emit modifier-shortcut-triggered: {:?}", e);
            }
        }
    }
}

/// Start the rdev listener on a background thread (only once).
fn start_listener(state: Arc<ShortcutState>) -> Result<(), String> {
    if state.running.swap(true, Ordering::SeqCst) {
        return Ok(()); // already running
    }

    let state_clone = state.clone();

    std::thread::spawn(move || {
        let st = state_clone.clone();
        let st_err = state_clone;

        if let Err(e) = listen(move |event: Event| {
            match event.event_type {
                EventType::KeyPress(key) => {
                    if is_modifier_key(&key) {
                        let mut held = st.held_keys.lock().unwrap();
                        held.insert(key);
                    } else {
                        // Non-modifier key pressed while modifiers held -> contaminate
                        let held = st.held_keys.lock().unwrap();
                        if !held.is_empty() {
                            st.contaminated.store(true, Ordering::SeqCst);
                        }
                    }
                }
                EventType::KeyRelease(key) => {
                    if is_modifier_key(&key) {
                        let was_contaminated = st.contaminated.load(Ordering::SeqCst);

                        // Check if the target was satisfied BEFORE removing this key.
                        let was_active = {
                            let held = st.held_keys.lock().unwrap();
                            if let Ok(target_guard) = st.target.lock() {
                                if let Some(ref target) = *target_guard {
                                    target_is_active(target, &held)
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        };

                        // Remove the released key
                        {
                            let mut held = st.held_keys.lock().unwrap();
                            held.remove(&key);

                            // If all modifiers are released, reset contamination
                            if held.is_empty() {
                                st.contaminated.store(false, Ordering::SeqCst);
                            }
                        }

                        // Fire if the shortcut was active and not contaminated
                        if was_active && !was_contaminated {
                            fire_shortcut(&st);
                        }
                    }
                }
                _ => {}
            }
        }) {
            eprintln!("[VoxFusion] rdev listen error: {:?}", e);
            st_err.running.store(false, Ordering::SeqCst);
        }
    });

    Ok(())
}

fn register(app_handle: tauri::AppHandle, shortcut: &str) -> Result<(), String> {
    let target = parse_shortcut(shortcut)
        .ok_or_else(|| format!("Unknown modifier shortcut: {}", shortcut))?;

    let state = get_state();

    // Store the app handle
    if let Ok(mut handle) = state.app_handle.lock() {
        *handle = Some(app_handle);
    }

    // Set the target
    if let Ok(mut t) = state.target.lock() {
        *t = Some(target);
    }

    // Clear held keys and contamination for a clean start
    if let Ok(mut held) = state.held_keys.lock() {
        held.clear();
    }
    state.contaminated.store(false, Ordering::SeqCst);

    // Start the listener if not already running
    start_listener(state.clone())
}

fn unregister() {
    let state = get_state();
    if let Ok(mut t) = state.target.lock() {
        *t = None;
    }
    if let Ok(mut held) = state.held_keys.lock() {
        held.clear();
    }
    state.contaminated.store(false, Ordering::SeqCst);
}

#[tauri::command]
pub fn register_modifier_shortcut(
    app_handle: tauri::AppHandle,
    shortcut: String,
) -> Result<(), String> {
    register(app_handle, &shortcut)
}

#[tauri::command]
pub fn unregister_modifier_shortcut() -> Result<(), String> {
    unregister();
    Ok(())
}
