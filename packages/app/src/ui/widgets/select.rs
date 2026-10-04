//! A dropdown for choosing one of a few options.

use gpui_kit::{
    App, Div, ElementId, InteractiveElement as _, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, deferred, div, prelude::*, px,
};
use std::rc::Rc;

use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::icon;

#[derive(Debug, Clone, PartialEq)]
pub struct SelectOption {
    pub value: SharedString,
    pub label: SharedString,
}

impl SelectOption {
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

/// What the dropdown remembers between frames, for as long as it is shown.
#[derive(Default)]
struct SelectState {
    open: bool,
    /// Whether the last press landed on the button, which keeps the accent
    /// border the way a focused button does.
    focused: bool,
}

/// The button showing the option whose value is `value`, and the list that
/// opens under it. `on_change` receives the value of the option picked.
pub fn select(
    id: impl Into<ElementId>,
    value: &str,
    options: Vec<SelectOption>,
    on_change: impl Fn(&SharedString, &mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    let p = palette(cx);
    let state = window.use_keyed_state(id, cx, |_, _| SelectState::default());
    let SelectState { open, focused } = *state.read(cx);
    let on_change = Rc::new(on_change);

    let selected_label = options
        .iter()
        .find(|option| option.value.as_ref() == value)
        .map(|option| option.label.clone())
        .unwrap_or_default();

    let button = div()
        .id("button")
        .flex()
        .items_center()
        .justify_between()
        .w_full()
        .px_4()
        .py_2p5()
        .bg(p.surface)
        .border_1()
        .border_color(if focused { p.ac } else { p.border_strong })
        .hover(|button| button.border_color(p.ac))
        .text_color(p.txt_primary)
        .capture_any_mouse_down({
            let state = state.clone();
            move |_, _, cx| {
                state.update(cx, |state, cx| {
                    state.focused = true;
                    cx.notify();
                });
            }
        })
        .on_mouse_down_out({
            let state = state.clone();
            move |_, _, cx| {
                state.update(cx, |state, cx| {
                    state.focused = false;
                    cx.notify();
                });
            }
        })
        .on_click({
            let state = state.clone();
            move |_, _, cx| {
                state.update(cx, |state, cx| {
                    state.open = !state.open;
                    cx.notify();
                });
            }
        })
        // An empty label is an empty inline box in the original: it has no
        // line, so the button is only as tall as the chevron.
        .when(!selected_label.is_empty(), |button| {
            button.child(text(selected_label).truncate())
        })
        .child(
            icon("chevron-down")
                .size_4()
                .ml_2()
                .text_color(p.txt_muted)
                .flipped(open),
        );

    let menu = open.then(|| {
        let items = options.into_iter().enumerate().map(|(index, option)| {
            let selected = option.value.as_ref() == value;
            let state = state.clone();
            let on_change = on_change.clone();

            div()
                .id(("option", index))
                .flex()
                .items_center()
                .justify_between()
                .w_full()
                .px_4()
                .py_2p5()
                .hover(|item| item.bg(p.hover))
                .map(|item| {
                    if selected {
                        item.text_color(p.ac).bg(p.hover)
                    } else {
                        item.text_color(p.txt_primary)
                    }
                })
                .on_click(move |_, window, cx| {
                    on_change(&option.value, window, cx);
                    state.update(cx, |state, cx| {
                        state.open = false;
                        cx.notify();
                    });
                })
                .child(text(option.label.clone()).truncate())
                .when(selected, |item| item.child(text("[*]").nowrap()))
        });

        // Drawn after everything else, so the controls below the button do
        // not cover the list. The settings modal is itself deferred at this
        // priority; of two equal priorities the later one is on top.
        // The border sits outside the scrolling area, as in CSS: the list
        // scrolls within the 240px box less its border.
        deferred(
            div()
                .absolute()
                .top_full()
                .left_0()
                .w_full()
                .mt_1()
                .occlude()
                .bg(p.surface)
                .border_1()
                .border_color(p.border_strong)
                // The button's listeners run first, the menu being drawn
                // later: by now `focused` tells whether this press landed on
                // the button, which toggles the list itself.
                .on_mouse_down_out({
                    let state = state.clone();
                    move |_, _, cx| {
                        state.update(cx, |state, cx| {
                            if !state.focused {
                                state.open = false;
                                cx.notify();
                            }
                        });
                    }
                })
                .child(
                    div()
                        .id("menu")
                        .max_h(px(238.))
                        .overflow_y_scroll()
                        .children(items),
                ),
        )
        .with_priority(1)
    });

    div()
        .relative()
        .font_mono()
        .type_sm()
        .child(button)
        .children(menu)
}
