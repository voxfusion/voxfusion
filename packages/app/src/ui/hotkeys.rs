//! Hotkey strings: how they are stored, validated and shown.
//!
//! A hotkey is either modifier-only (`LeftControl+LeftOption`), which the
//! native key watcher observes, or a combination ending in a regular key
//! (`Command+Shift+K`), which is registered as a global shortcut.

use crate::backend::SystemKey;
use crate::i18n::{self, Locale};

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

fn format_hotkey_part(part: &str, locale: Locale) -> String {
    let trimmed = part.trim();
    let upper = trimmed.to_uppercase();

    let numpad = |key: &str| {
        i18n::translate_with(locale, "settings.hotkeyNumpadKey", &[("key", key)]).to_string()
    };

    match upper.as_str() {
        "LEFTCONTROL" => {
            i18n::translate_with(locale, "settings.hotkeyLeftKey", &[("key", "\u{2303}")])
                .to_string()
        }
        "RIGHTCONTROL" => {
            i18n::translate_with(locale, "settings.hotkeyRightKey", &[("key", "\u{2303}")])
                .to_string()
        }
        "LEFTOPTION" => {
            i18n::translate_with(locale, "settings.hotkeyLeftKey", &[("key", "\u{2325}")])
                .to_string()
        }
        "RIGHTOPTION" => {
            i18n::translate_with(locale, "settings.hotkeyRightKey", &[("key", "\u{2325}")])
                .to_string()
        }
        "LEFTSHIFT" => {
            i18n::translate_with(locale, "settings.hotkeyLeftKey", &[("key", "\u{21E7}")])
                .to_string()
        }
        "RIGHTSHIFT" => {
            i18n::translate_with(locale, "settings.hotkeyRightKey", &[("key", "\u{21E7}")])
                .to_string()
        }
        "LEFTCOMMAND" => {
            i18n::translate_with(locale, "settings.hotkeyLeftKey", &[("key", "\u{2318}")])
                .to_string()
        }
        "RIGHTCOMMAND" => {
            i18n::translate_with(locale, "settings.hotkeyRightKey", &[("key", "\u{2318}")])
                .to_string()
        }
        "FN" => "Fn".to_string(),
        "COMMAND" | "CMD" | "SUPER" => "\u{2318}".to_string(),
        "COMMANDORCONTROL" | "COMMANDORCTRL" | "CMDORCTRL" | "CMDORCONTROL" => {
            #[cfg(target_os = "macos")]
            {
                "\u{2318}".to_string()
            }
            #[cfg(not(target_os = "macos"))]
            {
                "\u{2303}".to_string()
            }
        }
        "CONTROL" | "CTRL" => "\u{2303}".to_string(),
        "OPTION" | "ALT" => "\u{2325}".to_string(),
        "SHIFT" => "\u{21E7}".to_string(),
        "CAPSLOCK" => "\u{21EA}".to_string(),
        "BACKQUOTE" => "`".to_string(),
        "BACKSLASH" => "\\".to_string(),
        "BRACKETLEFT" => "[".to_string(),
        "BRACKETRIGHT" => "]".to_string(),
        "COMMA" => ",".to_string(),
        "EQUAL" => "=".to_string(),
        "MINUS" => "-".to_string(),
        "PERIOD" => ".".to_string(),
        "QUOTE" => "'".to_string(),
        "SEMICOLON" => ";".to_string(),
        "SLASH" => "/".to_string(),
        "SPACE" => i18n::translate(locale, "settings.hotkeySpace").to_string(),
        "TAB" => "\u{21E5}".to_string(),
        "ENTER" | "RETURN" => "\u{21A9}".to_string(),
        "ESCAPE" | "ESC" => "\u{238B}".to_string(),
        "BACKSPACE" => "\u{232B}".to_string(),
        "DELETE" => "\u{2326}".to_string(),
        "ARROWUP" | "UP" => "\u{2191}".to_string(),
        "ARROWDOWN" | "DOWN" => "\u{2193}".to_string(),
        "ARROWLEFT" | "LEFT" => "\u{2190}".to_string(),
        "ARROWRIGHT" | "RIGHT" => "\u{2192}".to_string(),
        "HOME" => "\u{2196}".to_string(),
        "END" => "\u{2198}".to_string(),
        "PAGEUP" => "\u{21DE}".to_string(),
        "PAGEDOWN" => "\u{21DF}".to_string(),
        "INSERT" => i18n::translate(locale, "settings.hotkeyInsert").to_string(),
        "PAUSE" | "PAUSEBREAK" | "MEDIAPAUSE" => {
            i18n::translate(locale, "settings.hotkeyPause").to_string()
        }
        "PRINTSCREEN" => i18n::translate(locale, "settings.hotkeyPrintScreen").to_string(),
        "SCROLLLOCK" => i18n::translate(locale, "settings.hotkeyScrollLock").to_string(),
        "NUMLOCK" => i18n::translate(locale, "settings.hotkeyNumLock").to_string(),
        "NUMPAD0" | "NUM0" => numpad("0"),
        "NUMPAD1" | "NUM1" => numpad("1"),
        "NUMPAD2" | "NUM2" => numpad("2"),
        "NUMPAD3" | "NUM3" => numpad("3"),
        "NUMPAD4" | "NUM4" => numpad("4"),
        "NUMPAD5" | "NUM5" => numpad("5"),
        "NUMPAD6" | "NUM6" => numpad("6"),
        "NUMPAD7" | "NUM7" => numpad("7"),
        "NUMPAD8" | "NUM8" => numpad("8"),
        "NUMPAD9" | "NUM9" => numpad("9"),
        "NUMPADADD" | "NUMADD" | "NUMPADPLUS" | "NUMPLUS" => numpad("+"),
        "NUMPADDECIMAL" | "NUMDECIMAL" => numpad("."),
        "NUMPADDIVIDE" | "NUMDIVIDE" => numpad("/"),
        "NUMPADENTER" | "NUMENTER" => numpad("\u{21A9}"),
        "NUMPADEQUAL" | "NUMEQUAL" => numpad("="),
        "NUMPADMULTIPLY" | "NUMMULTIPLY" => numpad("*"),
        "NUMPADSUBTRACT" | "NUMSUBTRACT" => numpad("-"),
        "AUDIOVOLUMEDOWN" | "VOLUMEDOWN" => {
            i18n::translate(locale, "settings.hotkeyVolumeDown").to_string()
        }
        "AUDIOVOLUMEUP" | "VOLUMEUP" => {
            i18n::translate(locale, "settings.hotkeyVolumeUp").to_string()
        }
        "AUDIOVOLUMEMUTE" | "VOLUMEMUTE" => {
            i18n::translate(locale, "settings.hotkeyVolumeMute").to_string()
        }
        "MEDIAPLAY" => i18n::translate(locale, "settings.hotkeyMediaPlay").to_string(),
        "MEDIAPLAYPAUSE" => i18n::translate(locale, "settings.hotkeyMediaPlayPause").to_string(),
        "MEDIASTOP" => i18n::translate(locale, "settings.hotkeyMediaStop").to_string(),
        "MEDIATRACKNEXT" => i18n::translate(locale, "settings.hotkeyMediaNext").to_string(),
        "MEDIATRACKPREV" | "MEDIATRACKPREVIOUS" => {
            i18n::translate(locale, "settings.hotkeyMediaPrev").to_string()
        }
        _ => {
            if let Some(letter) = upper.strip_prefix("KEY")
                && letter.len() == 1
                && letter.as_bytes()[0].is_ascii_alphabetic()
            {
                return letter.to_string();
            }
            if let Some(digit) = upper.strip_prefix("DIGIT")
                && digit.len() == 1
                && digit.as_bytes()[0].is_ascii_digit()
            {
                return digit.to_string();
            }
            if upper.len() == 1 && upper.as_bytes()[0].is_ascii_alphabetic() {
                return upper;
            }
            if let Some(number) = upper.strip_prefix('F')
                && !number.is_empty()
                && number.bytes().all(|byte| byte.is_ascii_digit())
            {
                return upper;
            }
            trimmed.to_string()
        }
    }
}

/// A hotkey with its modifier names replaced by their symbols, and
/// left/right modifier labels localized for `locale`.
pub fn hotkey_display_name_for_locale(hotkey: &str, locale: Locale) -> String {
    hotkey
        .split('+')
        .map(|part| format_hotkey_part(part, locale))
        .collect::<Vec<_>>()
        .join("+")
}

/// A hotkey with its modifier names replaced by their symbols.
pub fn hotkey_display_name(hotkey: &str) -> String {
    hotkey_display_name_for_locale(hotkey, Locale::En)
}

/// Why a hotkey cannot be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyValidationError {
    InvalidCombination,
    FnNotAllowedForHoldToSpeak,
}

impl HotkeyValidationError {
    pub fn translation_key(self) -> &'static str {
        match self {
            Self::InvalidCombination => "settings.hotkeyInvalidCombination",
            Self::FnNotAllowedForHoldToSpeak => "settings.hotkeyFnNotAllowed",
        }
    }
}

/// The reason `hotkey` cannot be the hands-free hotkey, if any.
pub fn validate_hands_free_hotkey(hotkey: &str) -> Option<HotkeyValidationError> {
    (!is_valid_hotkey(hotkey)).then_some(HotkeyValidationError::InvalidCombination)
}

/// The reason `hotkey` cannot be the hold-to-speak hotkey, if any. Fn alone
/// conflicts with what macOS uses it for.
pub fn validate_hold_to_speak_hotkey(hotkey: &str) -> Option<HotkeyValidationError> {
    if hotkey == "Fn" {
        return Some(HotkeyValidationError::FnNotAllowedForHoldToSpeak);
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

    #[test]
    fn display_names_use_modifier_symbols() {
        assert_eq!(hotkey_display_name("LeftControl+LeftOption"), "Left⌃+Left⌥");
        assert_eq!(hotkey_display_name("Command+Shift+K"), "⌘+⇧+K");
        assert_eq!(hotkey_display_name("RightCommand"), "Right⌘");
    }

    #[test]
    fn fn_alone_cannot_be_held_to_speak() {
        assert!(validate_hold_to_speak_hotkey("Fn").is_some());
        assert!(validate_hands_free_hotkey("Fn").is_none());
        assert!(validate_hold_to_speak_hotkey("RightCommand").is_none());
    }
}
