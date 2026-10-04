//! The dropdown that picks a transcription style for an app or a site.

use gpui_kit::{
    App, Bounds, ClickEvent, Context, EventEmitter, FocusHandle, InteractiveElement as _,
    IntoElement, KeyDownEvent, MouseDownEvent, ParentElement as _, Pixels, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, canvas, deferred, div, prelude::*,
    relative,
};
use std::cell::Cell;
use std::rc::Rc;

use crate::settings::STYLE_LIST;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::icon;
use crate::ui::{t, upper};

/// The name of a style in the interface language.
pub fn style_label(style: &str, cx: &App) -> SharedString {
    match style {
        "professional" => t(cx, "appInstructions.styles.professional"),
        "casual" => t(cx, "appInstructions.styles.casual"),
        "agents" => t(cx, "appInstructions.styles.agents"),
        _ => t(cx, "appInstructions.styles.default"),
    }
}

/// The style the user picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StyleSelected(pub &'static str);

pub struct StyleSelect {
    value: SharedString,
    open: bool,
    highlighted: usize,
    /// Whether the pointer is over the list, which hangs outside the row the
    /// dropdown is in.
    list_hovered: bool,
    list_bounds: Rc<Cell<Bounds<Pixels>>>,
    focus_handle: FocusHandle,
}

impl EventEmitter<StyleSelected> for StyleSelect {}

impl StyleSelect {
    pub fn new(value: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        Self {
            value: value.into(),
            open: false,
            highlighted: 0,
            list_hovered: false,
            list_bounds: Rc::default(),
            focus_handle: cx.focus_handle().tab_stop(true),
        }
    }

    pub fn set_value(&mut self, value: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.value = value.into();
        cx.notify();
    }

    /// Whether the pointer is over the open list. The row that holds the
    /// dropdown counts as hovered then, though the list lies outside it.
    pub fn list_hovered(&self) -> bool {
        self.open && self.list_hovered
    }

    /// Opens the list with the current style highlighted.
    fn open(&mut self, cx: &mut Context<Self>) {
        self.open = true;
        self.highlighted = STYLE_LIST
            .iter()
            .position(|style| *style == self.value.as_ref())
            .unwrap_or(0);
        cx.notify();
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        self.list_hovered = false;
        cx.notify();
    }

    fn choose(&mut self, index: usize, cx: &mut Context<Self>) {
        cx.emit(StyleSelected(STYLE_LIST[index]));
        self.close(cx);
    }

    fn handle_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "down" | "up" | "enter" | "space" if !self.open => self.open(cx),
            "down" => {
                self.highlighted = (self.highlighted + 1).min(STYLE_LIST.len() - 1);
                cx.notify();
            }
            "up" => {
                self.highlighted = self.highlighted.saturating_sub(1);
                cx.notify();
            }
            "enter" | "space" => self.choose(self.highlighted, cx),
            "escape" if self.open => self.close(cx),
            _ => {}
        }
    }

    fn render_list(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let p = palette(cx);
        let list_bounds = self.list_bounds.clone();

        let options = STYLE_LIST.iter().enumerate().map(|(index, style)| {
            let selected = *style == self.value.as_ref();

            div()
                .id(index)
                .w_full()
                .px_3()
                .py_1p5()
                .flex()
                .items_center()
                .justify_between()
                .type_xs()
                .text_color(if selected { p.ac } else { p.txt_primary })
                .when(index == self.highlighted, |option| option.bg(p.hover))
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    if *hovered {
                        this.highlighted = index;
                        cx.notify();
                    }
                }))
                .on_click(cx.listener(move |this, _, _, cx| this.choose(index, cx)))
                .child(
                    text(upper(&style_label(style, cx)))
                        .tracking_wider()
                        .truncate(),
                )
                .when(selected, |option| {
                    option.child(div().text_right().child(text("[*]").tracking_wider()))
                })
        });

        div()
            .id("list")
            .absolute()
            .top(relative(1.))
            .right_0()
            .w_full()
            .mt_1()
            .occlude()
            .bg(p.surface)
            .border_1()
            .border_color(p.border_strong)
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                this.list_hovered = *hovered;
                cx.notify();
            }))
            .children(options)
            // Remembers where the list is, to tell the clicks outside it.
            .child(
                canvas(move |bounds, _, _| list_bounds.set(bounds), |_, _, _, _| {})
                    .absolute()
                    .inset_0(),
            )
    }
}

impl Render for StyleSelect {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);

        div()
            .relative()
            .w_40()
            .flex_shrink_0()
            .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                // The list hangs below the button, outside its bounds.
                if this.open && !this.list_bounds.get().contains(&event.position) {
                    this.close(cx);
                }
            }))
            .child(
                div()
                    .id("button")
                    .track_focus(&self.focus_handle)
                    .on_key_down(cx.listener(Self::handle_key_down))
                    .on_click(cx.listener(|this, event: &ClickEvent, _, cx| {
                        // Releasing Enter or Space on the focused button
                        // counts as a click; those keys were already handled
                        // when pressed, and a second toggle would undo that.
                        if matches!(event, ClickEvent::Keyboard(_)) {
                            return;
                        }

                        if this.open {
                            this.close(cx);
                        } else {
                            this.open(cx);
                        }
                    }))
                    .w_full()
                    .px_3()
                    .py_1p5()
                    .flex()
                    .items_center()
                    .justify_between()
                    .bg(p.input)
                    .border_1()
                    .border_color(p.border_strong)
                    .hover(|button| button.border_color(p.ac))
                    .focus(|button| button.border_color(p.ac))
                    .type_xs()
                    .text_color(p.txt_primary)
                    .child(
                        text(upper(&style_label(&self.value, cx)))
                            .tracking_wider()
                            .truncate(),
                    )
                    .child(
                        icon("chevron-down")
                            .size_3()
                            .ml_2()
                            .text_color(p.txt_muted)
                            .flipped(self.open),
                    ),
            )
            .when(self.open, |select| {
                select.child(deferred(self.render_list(cx)))
            })
    }
}
