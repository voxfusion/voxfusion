//! The pieces the wizard's steps are built from.

use gpui_kit::{
    App, Div, ElementId, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Rgba, SharedString, Stateful, Styled as _, div, px,
};

use crate::ui::grid::above_grid;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::upper;
use crate::ui::widgets::{Icon, icon, spinning};

/// Raised by a permission step each time it learns whether access is granted.
pub struct PermissionChanged(pub bool);

/// A step's column: its terminal-style label over its content.
pub fn step_column(label: &'static str, label_color: Hsla) -> Div {
    div().w_full().max_w(px(448.)).child(
        div()
            .mb_8()
            .type_sm()
            .text_color(label_color)
            .child(text(label).tracking_wider().center()),
    )
}

/// Turns `content` into the bordered panel a step's controls sit in.
pub fn card(content: Div, cx: &mut App) -> impl IntoElement + use<> {
    let p = palette(cx);

    // The grid is kept off by a wrapper, not by the panel itself: an
    // absolute child spans only the padding box, which would leave the
    // panel's border under the grid.
    div().relative().child(above_grid(cx)).child(
        content
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .p_8(),
    )
}

/// The framed icon at the top of a card.
pub fn icon_box(icon: Icon, cx: &App) -> impl IntoElement + use<> {
    let p = palette(cx);

    div().flex().justify_center().mb_6().child(
        div()
            .size_16()
            .border_1()
            .border_color(p.border_strong)
            .flex()
            .items_center()
            .justify_center()
            .child(icon.size_8()),
    )
}

pub fn title(label: &str, cx: &App) -> impl IntoElement + use<> {
    div()
        .mb_3()
        .type_xl()
        .text_color(palette(cx).txt_primary)
        .child(text(upper(label)).tracking_wider().center())
}

/// The paragraph under a card's title. The margin below it varies by step.
pub fn description(label: SharedString, cx: &App) -> Div {
    div()
        .type_sm()
        .text_color(palette(cx).txt_secondary)
        .child(text(label).center())
}

/// A centered line saying where the step stands: an icon and a few words.
pub fn status_row(icon: impl IntoElement, label: SharedString, color: Hsla) -> Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .gap_2()
        .type_sm()
        .text_color(color)
        .child(icon)
        .child(text(label).center())
}

/// The status line shown while a permission is being looked up.
pub fn checking_row(label: SharedString, cx: &App) -> Div {
    status_row(
        spinning("checking", icon("spinner-ring").size_4()),
        label,
        palette(cx).txt_muted,
    )
}

/// Small print set off by a rule on its left. The margins vary by step.
pub fn side_note(label: SharedString, cx: &App) -> Div {
    let p = palette(cx);

    div()
        .border_l_2()
        .border_color(p.border_strong)
        .pl_3()
        .type_xs()
        .text_color(p.txt_muted)
        .child(text(label))
}

/// The filled button of the wizard, without its horizontal padding and
/// label. A disabled one is drawn at 30% over `backdrop` and still shows its
/// hover color, as the original's does.
pub fn accent_button(
    id: impl Into<ElementId>,
    enabled: bool,
    backdrop: Hsla,
    cx: &App,
) -> Stateful<Div> {
    let p = palette(cx);
    let shade = move |color: Hsla| {
        if enabled {
            color
        } else {
            faded(color, backdrop, 0.3)
        }
    };

    div()
        .id(id)
        .flex()
        .items_center()
        .py_3()
        .type_sm()
        .font_weight(FontWeight::BOLD)
        .bg(shade(p.ac))
        .text_color(shade(p.ac_on))
        .hover(move |button| button.bg(shade(p.ac_hover)))
}

/// A button's label: capitals, letter-spaced, centered if it wraps.
pub fn button_label(label: &str) -> impl IntoElement + use<> {
    text(upper(label)).tracking_wider().center()
}

/// `color` at `opacity` over an opaque `backdrop`, as one opaque color. CSS
/// fades an element as a whole, while GPUI's `opacity` fades each layer on its
/// own, which would let a button's fill show through its label.
pub fn faded(color: Hsla, backdrop: Hsla, opacity: f32) -> Hsla {
    let (color, backdrop) = (Rgba::from(color), Rgba::from(backdrop));
    let mix = |over: f32, under: f32| over * opacity + under * (1. - opacity);

    Rgba {
        r: mix(color.r, backdrop.r),
        g: mix(color.g, backdrop.g),
        b: mix(color.b, backdrop.b),
        a: 1.,
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::rgb;

    #[test]
    fn a_faded_color_is_mixed_with_its_backdrop() {
        let mixed = Rgba::from(faded(rgb(0xff3e00).into(), rgb(0xffffff).into(), 0.3));

        // 30% of #ff3e00 over white.
        assert!((mixed.r - 1.).abs() < 0.005);
        assert!((mixed.g - (0.3 * 62. + 0.7 * 255.) / 255.).abs() < 0.005);
        assert!((mixed.b - 0.7).abs() < 0.005);
        assert_eq!(mixed.a, 1.);
    }
}
