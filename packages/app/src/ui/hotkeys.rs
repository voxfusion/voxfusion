//! Hotkey strings: how they are stored, validated and shown.
//!
//! A hotkey is either modifier-only (`LeftControl+LeftOption`), which the
//! native key watcher observes, or a combination ending in a regular key
//! (`Command+Shift+K`), which is registered as a global shortcut.

use crate::backend::SystemKey;

/// Modifier-only hotkey parts, in the order they are written.
const SYSTEM_HOTKEYS: [(&str, SystemKey); 9] = [
    ("Fn", SystemKey::Fn),
    ("LeftControl", SystemKey::LeftControl),
    ("RightControl", SystemKey::RightControl),
    ("LeftOption", SystemKey::LeftOption),
    ("RightOption", SystemKey::RightOption),
    ("LeftShift", SystemKey::LeftShift),
    ("RightShift", SystemKey::RightShift),
    ("LeftCommand", SystemKey::LeftCommand),
    ("RightCommand", SystemKey::RightCommand),
];

const COMBO_MODIFIER_NAMES: [&str; 13] = [
    "Command",
    "Cmd",
    "CommandOrControl",
    "CommandOrCtrl",
    "CmdOrCtrl",
    "CmdOrControl",
    "Control",
    "Ctrl",
    "Alt",
    "Option",
    "Shift",
    "CapsLock",
    "Fn",
];

/// The hotkey string for a set of held modifier keys.
pub fn system_hotkey_from_keys(keys: &[SystemKey]) -> String {
    SYSTEM_HOTKEYS
        .iter()
        .filter(|(_, key)| keys.contains(key))
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join("+")
}

/// The modifier keys of a modifier-only hotkey, or `None` for a combination.
pub fn system_keys_from_hotkey(hotkey: &str) -> Option<Vec<SystemKey>> {
    hotkey
        .split('+')
        .map(|part| {
            SYSTEM_HOTKEYS
                .iter()
                .find(|(name, _)| *name == part)
                .map(|(_, key)| *key)
        })
        .collect()
}

pub fn is_system_only_hotkey(hotkey: &str) -> bool {
    system_keys_from_hotkey(hotkey).is_some()
}

/// Whether one of the two backends can register `hotkey`.
pub fn is_valid_hotkey(hotkey: &str) -> bool {
    if is_system_only_hotkey(hotkey) {
        return true;
    }

    let parts: Vec<&str> = hotkey
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();

    if parts.len() < 2 {
        return false;
    }
    if parts
        .iter()
        .any(|part| SYSTEM_HOTKEYS.iter().any(|(name, _)| name == part))
    {
        return false;
    }

    let (key, modifiers) = parts.split_last().expect("at least two parts");
    if COMBO_MODIFIER_NAMES.contains(key) {
        return false;
    }

    modifiers
        .iter()
        .all(|modifier| COMBO_MODIFIER_NAMES.contains(modifier))
}

/// A hotkey with its modifier names replaced by their symbols.
#[cfg(target_os = "macos")]
pub fn hotkey_display_name(hotkey: &str) -> String {
    hotkey
        .replace("Command", "\u{2318}")
        .replace("Control", "\u{2303}")
        .replace("Option", "\u{2325}")
        .replace("Alt", "\u{2325}")
        .replace("Shift", "\u{21E7}")
}

/// One key of a hotkey as PC keyboards label it: Command is the Super key
/// there, and Option is Alt.
#[cfg(not(target_os = "macos"))]
pub fn pc_key_name(part: &str) -> String {
    let modifier = |name: &str| match name {
        "Command" | "Cmd" => Some("Super"),
        "Control" | "Ctrl" => Some("Ctrl"),
        "Option" | "Alt" => Some("Alt"),
        "Shift" => Some("Shift"),
        _ => None,
    };

    for side in ["Left", "Right"] {
        if let Some(name) = part.strip_prefix(side).and_then(modifier) {
            return format!("{side} {name}");
        }
    }
    modifier(part).unwrap_or(part).to_string()
}

/// A hotkey with its modifiers named as PC keyboards label them.
#[cfg(not(target_os = "macos"))]
pub fn hotkey_display_name(hotkey: &str) -> String {
    hotkey
        .split('+')
        .map(pc_key_name)
        .collect::<Vec<_>>()
        .join("+")
}

/// The reason `hotkey` cannot be the hands-free hotkey, if any.
pub fn validate_hands_free_hotkey(hotkey: &str) -> Option<&'static str> {
    (!is_valid_hotkey(hotkey)).then_some("Use modifier keys plus one regular key.")
}

/// The reason `hotkey` cannot be the hold-to-speak hotkey, if any. Fn alone
/// conflicts with what macOS uses it for.
pub fn validate_hold_to_speak_hotkey(hotkey: &str) -> Option<&'static str> {
    if hotkey == "Fn" {
        return Some("FN cannot be used as hold-to-speak hotkey");
    }

    validate_hands_free_hotkey(hotkey)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_only_hotkeys_are_written_in_a_fixed_order() {
        assert_eq!(
            system_hotkey_from_keys(&[SystemKey::LeftOption, SystemKey::LeftControl]),
            "LeftControl+LeftOption"
        );
        assert_eq!(
            system_keys_from_hotkey("LeftControl+LeftOption"),
            Some(vec![SystemKey::LeftControl, SystemKey::LeftOption])
        );
        assert_eq!(system_keys_from_hotkey("Command+K"), None);
    }

    #[test]
    fn combinations_need_modifiers_and_one_regular_key() {
        assert!(is_valid_hotkey("RightCommand"));
        assert!(is_valid_hotkey("Command+Shift+K"));
        assert!(!is_valid_hotkey("K"));
        assert!(!is_valid_hotkey("Command+Shift"));
        assert!(!is_valid_hotkey("LeftControl+K"));
        assert!(!is_valid_hotkey(""));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn display_names_use_modifier_symbols() {
        assert_eq!(hotkey_display_name("LeftControl+LeftOption"), "Left⌃+Left⌥");
        assert_eq!(hotkey_display_name("Command+Shift+K"), "⌘+⇧+K");
        assert_eq!(hotkey_display_name("RightCommand"), "Right⌘");
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn display_names_use_pc_key_labels() {
        assert_eq!(
            hotkey_display_name("LeftControl+LeftOption"),
            "Left Ctrl+Left Alt"
        );
        assert_eq!(hotkey_display_name("Command+Shift+K"), "Super+Shift+K");
        assert_eq!(hotkey_display_name("RightCommand"), "Right Super");
        assert_eq!(hotkey_display_name("Alt+ArrowLeft"), "Alt+ArrowLeft");
    }

    #[test]
    fn fn_alone_cannot_be_held_to_speak() {
        assert!(validate_hold_to_speak_hotkey("Fn").is_some());
        assert!(validate_hands_free_hotkey("Fn").is_none());
        assert!(validate_hold_to_speak_hotkey("RightCommand").is_none());
    }
}
