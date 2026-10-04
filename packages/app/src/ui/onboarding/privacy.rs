//! Step 5: whether anonymous usage events may be sent.

use gpui_kit::{App, IntoElement, ParentElement as _, Styled as _, div};

use super::parts::{card, description, icon_box, step_column, title};
use crate::settings::SettingsStore;
use crate::ui::t;
use crate::ui::theme::palette;
use crate::ui::widgets::icon;
use crate::ui::widgets::toggle::toggle_option;

pub fn privacy_step(cx: &mut App) -> impl IntoElement + use<> {
    let p = palette(cx);

    let choice = div()
        .border_1()
        .border_color(p.border_strong)
        .bg(p.base)
        .p_4()
        .child(toggle_option(
            "analytics",
            &t(cx, "onboarding.privacyToggleLabel"),
            t(cx, "onboarding.privacyToggleDescription"),
            SettingsStore::get(cx).analytics_enabled,
            |enabled, _, cx| {
                SettingsStore::update(cx, |settings| settings.analytics_enabled = enabled);
            },
            cx,
        ));

    let content = div()
        .child(icon_box(icon("shield-check").text_color(p.ac), cx))
        .child(title(&t(cx, "onboarding.privacyTitle"), cx))
        .child(description(t(cx, "onboarding.privacyDescription"), cx).mb_8())
        .child(choice);

    step_column("[STEP_05] > PRIVACY_CHOICE", p.ac).child(card(content, cx))
}
