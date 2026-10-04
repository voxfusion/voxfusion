//! A setting that is either on or off: its name and description beside a
//! switch.

use gpui_kit::{
    App, Div, ElementId, InteractiveElement as _, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};

use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::upper;

pub fn toggle_option(
    id: impl Into<ElementId>,
    label: &str,
    description: SharedString,
    enabled: bool,
    on_change: impl Fn(bool, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Div {
    let p = palette(cx);

    let knob = div()
        .size_4()
        .ml(px(if enabled { 24. } else { 4. }))
        .bg(if enabled { p.ac_on } else { p.txt_muted });

    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_4()
        .font_mono()
        .type_xs()
        .child(
            div()
                .flex_1()
                .child(
                    div()
                        .mb_1()
                        .text_color(p.txt_muted)
                        .child(text(upper(label)).tracking_wider()),
                )
                .child(div().text_color(p.txt_faint).child(text(description))),
        )
        .child(
            div()
                .id(id)
                .flex()
                .items_center()
                .h_6()
                .w_11()
                .border_1()
                .border_color(if enabled { p.ac } else { p.border_strong })
                .bg(if enabled { p.ac } else { p.surface })
                .on_click(move |_, window, cx| on_change(!enabled, window, cx))
                .child(knob),
        )
}
