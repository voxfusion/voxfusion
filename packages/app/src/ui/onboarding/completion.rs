//! Step 8: the setup is done.

use gpui_kit::{App, IntoElement, ParentElement as _, Styled as _, div};

use super::parts::{card, description, step_column};
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::icon;
use crate::ui::{t, t_upper};

pub fn completion_step(cx: &mut App) -> impl IntoElement + use<> {
    let p = palette(cx);

    let content = div()
        .child(
            div().flex().justify_center().mb_6().child(
                div()
                    .size_20()
                    .border_1()
                    .border_color(p.success)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon("check-circle").size_12().text_color(p.success)),
            ),
        )
        .child(
            div().mb_3().type_2xl().text_color(p.txt_primary).child(
                text(t_upper(cx, "onboarding.completionTitle"))
                    .tracking_wider()
                    .center(),
            ),
        )
        .child(description(t(cx, "onboarding.completionDescription"), cx))
        .child(
            div()
                .mt_8()
                .pt_6()
                .border_t_1()
                .border_color(p.border)
                .type_xs()
                .text_color(p.txt_muted)
                .child(text("> READY_TO_TRANSCRIBE").tracking_wider().center()),
        );

    step_column("[STEP_08] > SETUP_COMPLETE", p.success).child(card(content, cx))
}
