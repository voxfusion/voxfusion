//! The windows and everything drawn in them.

pub mod apps_cache;
pub mod datetime;
pub mod download_format;
pub mod grid;
pub mod hotkey_recorder;
pub mod hotkeys;
pub mod main_window;
pub mod motion;
pub mod onboarding;
pub mod pages;
pub mod settings_modal;
pub mod text;
pub mod theme;
pub mod update_notification;
pub mod voice_control;
pub mod widgets;

use gpui_kit::{App, SharedString};

use crate::i18n::{self, Locale};
use crate::settings::SettingsStore;

/// The interface language.
pub fn locale(cx: &App) -> Locale {
    SettingsStore::get(cx).language
}

/// The interface string for `key`.
pub fn t(cx: &App, key: &str) -> SharedString {
    i18n::translate(locale(cx), key)
}

/// The interface string for `key`, with its placeholders filled in.
pub fn t_with(cx: &App, key: &str, params: &[(&str, &str)]) -> SharedString {
    i18n::translate_with(locale(cx), key, params)
}

/// The interface string for `key`, in capitals. CSS `uppercase` is applied
/// here because GPUI has no text transform.
pub fn t_upper(cx: &App, key: &str) -> SharedString {
    upper(&t(cx, key))
}

pub fn upper(text: &str) -> SharedString {
    text.to_uppercase().into()
}
