//! The Language section: the language of the interface.

use gpui_kit::{App, Div, ParentElement as _, SharedString, Styled as _, Window, div};

use crate::analytics;
use crate::i18n::Locale;
use crate::settings::SettingsStore;
use crate::ui::locale;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::select::{SelectOption, select};

/// Each language under its own name, as its speakers look for it.
fn language_name(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "ENGLISH",
        Locale::Ru => "RUSSIAN",
        Locale::Es => "ESPAÑOL",
        Locale::Zh => "中文",
        Locale::De => "DEUTSCH",
        Locale::Fr => "FRANÇAIS",
        Locale::It => "ITALIANO",
    }
}

pub(super) fn render(window: &mut Window, cx: &mut App) -> Div {
    let p = palette(cx);
    let options = Locale::ALL
        .map(|locale| SelectOption::new(locale.code(), language_name(locale)))
        .to_vec();

    div()
        .child(
            div()
                .mb_3()
                .type_xs()
                .text_color(p.txt_muted)
                .child(text("INTERFACE_LANGUAGE").tracking_wider()),
        )
        .child(select(
            "language",
            locale(cx).code(),
            options,
            |value: &SharedString, _, cx| {
                analytics::capture(
                    cx,
                    "settings_language_changed",
                    &[("language", value.to_string().into())],
                );

                if let Some(language) = Locale::from_code(value) {
                    SettingsStore::update(cx, |settings| settings.language = language);
                }
            },
            window,
            cx,
        ))
}
