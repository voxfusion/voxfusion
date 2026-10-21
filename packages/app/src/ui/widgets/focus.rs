//! Keyboard focus: the controls Tab stops at, and the mark on the one that
//! has the focus.

use gpui_kit::{App, Div, ElementId, FocusHandle, Styled as _, Window, div, px};

use crate::ui::theme::palette;

/// A focus handle that Tab stops at, kept for as long as `id` is shown, for
/// controls that are not views of their own.
pub fn keyed_focus_handle(
    id: impl Into<ElementId>,
    window: &mut Window,
    cx: &mut App,
) -> FocusHandle {
    window
        .use_keyed_state(id, cx, |_, cx| cx.focus_handle().tab_stop(true))
        .read(cx)
        .clone()
}

/// Whether `handle` has the focus and the keyboard brought it there, as CSS
/// `:focus-visible` has it: a click focuses a control without marking it.
pub fn focus_visible(handle: &FocusHandle, window: &Window) -> bool {
    handle.is_focused(window) && window.last_input_was_keyboard()
}

/// The mark around the control with the keyboard focus: an accent hairline
/// whose outer edge is `outset` px outside the control's padding box. The
/// control must be `relative`.
pub fn focus_ring(outset: f32, cx: &App) -> Div {
    let outset = px(-outset);

    div()
        .absolute()
        .top(outset)
        .right(outset)
        .bottom(outset)
        .left(outset)
        .border_1()
        .border_color(palette(cx).ac)
}
