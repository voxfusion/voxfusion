//! A setting that is either on or off: its name and description beside a
//! switch, which slides and changes color over 150ms. Tab reaches the
//! switch, and Enter or Space flips it.

use gpui_kit::{
    App, Div, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, prelude::*, px,
};

use crate::ui::motion::{Transitions as _, gliding};
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::upper;
use crate::ui::widgets::focus::{focus_ring, focus_visible, keyed_focus_handle};

pub fn toggle_option(
    id: impl Into<ElementId>,
    label: &str,
    description: SharedString,
    enabled: bool,
    on_change: impl Fn(bool, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Div {
    let p = palette(cx);

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
                        .text_color(p.txt_secondary)
                        .child(text(upper(label)).tracking_wider()),
                )
                .child(div().text_color(p.txt_hint).child(text(description))),
        )
        .child(Switch {
            id: id.into(),
            enabled,
            on_change: Box::new(on_change),
        })
}

/// The switch is built once it is laid out, where it can keep the focus
/// handle that Tab moves to.
#[derive(IntoElement)]
struct Switch {
    id: ElementId,
    enabled: bool,
    on_change: Box<dyn Fn(bool, &mut Window, &mut App)>,
}

impl RenderOnce for Switch {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = palette(cx);
        let Switch {
            id,
            enabled,
            on_change,
        } = self;
        let focus_handle = keyed_focus_handle(id.clone(), window, cx);

        // `transition-transform` slides the knob; its color changes at once.
        let knob = gliding("knob", if enabled { 24. } else { 4. }, move |offset| {
            div()
                .size_4()
                .ml(px(offset))
                .bg(if enabled { p.ac_on } else { p.txt_muted })
        });

        // Enter and Space on the focused switch arrive as a click.
        div()
            .id(id)
            .transition_colors()
            .track_focus(&focus_handle)
            .relative()
            .flex()
            .items_center()
            .h_6()
            .w_11()
            .border_1()
            .border_color(if enabled { p.ac } else { p.border_strong })
            .bg(if enabled { p.ac } else { p.surface })
            .on_click(move |_, window, cx| on_change(!enabled, window, cx))
            .child(knob)
            .when(focus_visible(&focus_handle, window), |switch| {
                switch.child(focus_ring(4., cx))
            })
    }
}
