//! The Privacy section: whether anonymous usage events are sent.

use gpui_kit::{App, Div, ParentElement as _, Styled as _, div};

use crate::settings::SettingsStore;
use crate::ui::t;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::toggle::toggle_option;

pub(super) fn render(cx: &App) -> Div {
    let p = palette(cx);

    div()
        .flex()
        .flex_col()
        .gap_6()
        .child(toggle_option(
            "analytics",
            &t(cx, "settings.analytics"),
            t(cx, "settings.analyticsDescription"),
            SettingsStore::get(cx).analytics_enabled,
            |enabled, _, cx| {
                SettingsStore::update(cx, |settings| settings.analytics_enabled = enabled);
            },
            cx,
        ))
        .child(
            div()
                .type_xs()
                .text_color(p.txt_faint)
                .child(text(t(cx, "settings.analyticsNote"))),
        )
}
