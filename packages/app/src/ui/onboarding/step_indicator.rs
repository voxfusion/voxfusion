//! The row of step numbers across the top of the wizard.

use gpui_kit::{
    App, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _, div,
    prelude::*,
};

use crate::settings::ONBOARDING_STEP_COUNT;
use crate::ui::motion::Transitions as _;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;

/// A step number as the wizard writes it: two digits.
pub fn format_step(step: u32) -> String {
    format!("{step:02}")
}

pub fn step_indicator(current_step: u32, cx: &App) -> impl IntoElement + use<> {
    let p = palette(cx);

    div()
        .flex()
        .items_center()
        .gap_4()
        .children((1..=ONBOARDING_STEP_COUNT).map(|step| {
            let number = div()
                .id("number")
                .transition_all()
                .type_sm()
                .text_color(if step <= current_step {
                    p.ac
                } else {
                    p.txt_muted
                })
                .when(step == current_step, |number| {
                    number.font_weight(FontWeight::BOLD)
                })
                .child(text(format_step(step)).tracking_wider());

            div()
                .id(step as usize)
                .flex()
                .items_center()
                .child(number)
                .when(step < ONBOARDING_STEP_COUNT, |item| {
                    item.child(
                        div()
                            .id("connector")
                            .transition_colors()
                            .w_8()
                            .h_px()
                            .mx_3()
                            .bg(if step < current_step {
                                p.ac
                            } else {
                                p.border_strong
                            }),
                    )
                })
        }))
}
