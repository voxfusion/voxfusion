//! What a speech-to-text model is: its name and badges, its size and
//! languages, and what it does not do. Settings → Models and the first-run
//! setup describe models with it.

use gpui_kit::{App, Div, Hsla, ParentElement as _, SharedString, Styled as _, div, prelude::*};

use crate::backend::ModelInfo;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::{t, t_upper, upper};

pub fn model_summary(model: &ModelInfo, cx: &App) -> Div {
    let p = palette(cx);

    let name = div()
        .flex()
        .items_center()
        .gap_2()
        .flex_wrap()
        .child(
            div()
                .type_sm()
                .text_color(p.txt_primary)
                .child(text(upper(&model.name)).tracking_wider().truncate()),
        )
        .when(model.recommended, |row| {
            row.child(badge(t_upper(cx, "settings.modelRecommended"), p.ac, p.ac))
        })
        .when(model.experimental, |row| {
            row.child(badge(
                t_upper(cx, "settings.modelExperimental"),
                p.txt_secondary,
                p.border_strong,
            ))
        });

    let details = |label: SharedString| {
        div()
            .mt_1p5()
            .type_xs()
            .text_color(p.txt_hint)
            .child(text(label))
    };

    div()
        .min_w_0()
        .child(name)
        .child(details(
            format!("{} · {}", model.size_label, model.languages).into(),
        ))
        // Styles are prompts, which only Whisper takes.
        .when(model.engine != "whisper", |summary| {
            summary.child(details(t(cx, "settings.modelStylesWhisperOnly")))
        })
}

/// A small framed label after a model's name.
fn badge(label: SharedString, color: Hsla, border: Hsla) -> Div {
    div()
        .px_1p5()
        .py_0p5()
        .border_1()
        .border_color(border)
        .type_px(10.)
        .text_color(color)
        .child(text(label).tracking_wider())
}
