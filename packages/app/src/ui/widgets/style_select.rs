//! The dropdown that picks a transcription style for an app or a site.

use gpui_kit::{
    App, Bounds, ClickEvent, Context, EventEmitter, FocusHandle, InteractiveElement as _,
    IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, ParentElement as _, Pixels, Render,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Window,
    canvas, deferred, div, prelude::*, px, relative,
};
use std::cell::Cell;
use std::rc::Rc;

use crate::settings::{STYLE_LIST, SettingsStore};
use crate::ui::motion::Transitions as _;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::{icon, turning};
use crate::ui::{t, t_upper, t_with, upper};

/// The dropdown option index that removes the override and restores inheritance.
const INHERIT_INDEX: usize = STYLE_LIST.len();
/// Margin kept between the floating menu and the viewport edge.
const MENU_MARGIN: f32 = 12.;
/// Space needed below the trigger to open downward comfortably without flipping.
const MENU_PREFERRED_HEIGHT: f32 = 240.;
/// Maximum inner scroll height for the dropdown menu.
const MENU_MAX_HEIGHT: f32 = 320.;
/// Top and bottom 1px borders on the outer menu container.
const MENU_BORDER_HEIGHT: f32 = 2.;

/// The name of a style in the interface language.
pub fn style_label(style: &str, cx: &App) -> SharedString {
    match style {
        "professional" => t(cx, "appInstructions.styles.professional"),
        "casual" => t(cx, "appInstructions.styles.casual"),
        "agents" => t(cx, "appInstructions.styles.agents"),
        _ => t(cx, "appInstructions.styles.default"),
    }
}

/// What removing a rule from this dropdown falls back to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InheritTarget {
    /// Per-app rules fall back to the global default style.
    GlobalDefault,
    /// Per-site rules fall back to the browser's per-app style, then the global default.
    AppOrGlobalDefault,
}

/// What the user picked in the style dropdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleSelected {
    /// Apply this specific style, overriding inherited defaults.
    Style(&'static str),
    /// Remove the override so this app or site inherits again.
    Inherit,
}

pub struct StyleSelect {
    value: SharedString,
    inherit: InheritTarget,
    disabled: bool,
    open: bool,
    highlighted: usize,
    /// Whether the pointer is over the list, which hangs outside the row the
    /// dropdown is in.
    list_hovered: bool,
    button_bounds: Rc<Cell<Bounds<Pixels>>>,
    list_bounds: Rc<Cell<Bounds<Pixels>>>,
    list_scroll: ScrollHandle,
    focus_handle: FocusHandle,
    _subscriptions: [Subscription; 2],
}

impl EventEmitter<StyleSelected> for StyleSelect {}

impl StyleSelect {
    pub fn new(
        value: impl Into<SharedString>,
        inherit: InheritTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle().tab_stop(true);
        let subscriptions = [
            cx.observe(&SettingsStore::entity(cx), |_, _, cx| cx.notify()),
            cx.on_blur(&focus_handle, window, |this, _, cx| {
                if this.open {
                    this.close(cx);
                }
            }),
        ];

        Self {
            value: value.into(),
            inherit,
            disabled: false,
            open: false,
            highlighted: 0,
            list_hovered: false,
            button_bounds: Rc::default(),
            list_bounds: Rc::default(),
            list_scroll: ScrollHandle::new(),
            focus_handle,
            _subscriptions: subscriptions,
        }
    }

    pub fn set_value(&mut self, value: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.value = value.into();
        cx.notify();
    }

    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        self.disabled = disabled;
        if disabled && self.open {
            self.open = false;
            self.list_hovered = false;
        }
        cx.notify();
    }

    /// Whether the pointer is over the open list. The row that holds the
    /// dropdown counts as hovered then, though the list lies outside it.
    pub fn list_hovered(&self) -> bool {
        self.open && self.list_hovered
    }

    /// Whether the selector button currently holds keyboard focus.
    pub fn is_focused(&self, window: &Window) -> bool {
        self.focus_handle.is_focused(window)
    }

    /// Opens the list with the current style highlighted.
    fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        self.open = true;
        self.highlighted = STYLE_LIST
            .iter()
            .position(|style| *style == self.value.as_ref())
            .unwrap_or(STYLE_LIST.len() - 1);
        self.list_scroll.scroll_to_item(self.highlighted);
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        self.list_hovered = false;
        cx.notify();
    }

    fn choose(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.disabled {
            self.close(cx);
            return;
        }
        let choice = match STYLE_LIST.get(index) {
            Some(&style) => StyleSelected::Style(style),
            None => StyleSelected::Inherit,
        };
        cx.emit(choice);
        self.close(cx);
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.disabled {
            return;
        }
        match event.keystroke.key.as_str() {
            "down" | "up" | "enter" | "space" if !self.open => self.open(window, cx),
            "down" => {
                self.highlighted = (self.highlighted + 1).min(INHERIT_INDEX);
                self.list_scroll.scroll_to_item(self.highlighted);
                cx.notify();
            }
            "up" => {
                self.highlighted = self.highlighted.saturating_sub(1);
                self.list_scroll.scroll_to_item(self.highlighted);
                cx.notify();
            }
            "enter" | "space" => self.choose(self.highlighted, cx),
            "escape" if self.open => self.close(cx),
            _ => {}
        }
    }

    fn render_list(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let p = palette(cx);
        let list_bounds = self.list_bounds.clone();
        let global_default = style_label(&SettingsStore::get(cx).default_style, cx);

        let viewport_height = window.viewport_size().height;
        let button = self.button_bounds.get();
        let has_bounds = button.size.height > px(0.);
        let space_below = (viewport_height - button.bottom() - px(MENU_MARGIN)).max(px(0.));
        let space_above = (button.top() - px(MENU_MARGIN)).max(px(0.));
        let open_upward =
            has_bounds && space_below < px(MENU_PREFERRED_HEIGHT) && space_above > space_below;
        let available = if has_bounds {
            if open_upward {
                space_above
            } else {
                space_below
            }
        } else {
            px(MENU_MAX_HEIGHT)
        };
        let scroll_max_h = (available - px(MENU_BORDER_HEIGHT))
            .min(px(MENU_MAX_HEIGHT))
            .max(px(64.));

        let options = STYLE_LIST.iter().enumerate().map(|(index, style)| {
            let selected = *style == self.value.as_ref();
            let neutral = *style == "default";

            div()
                .id(index)
                .w_full()
                .px_3()
                .py_1p5()
                .flex()
                .flex_col()
                .gap_0p5()
                .type_xs()
                .when(index == self.highlighted, |option| option.bg(p.hover))
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    if *hovered {
                        this.highlighted = index;
                        cx.notify();
                    }
                }))
                .on_click(cx.listener(move |this, _, _, cx| this.choose(index, cx)))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .text_color(p.txt_primary)
                        .child(
                            text(upper(&style_label(style, cx)))
                                .tracking_wider()
                                .truncate(),
                        )
                        .when(selected, |row| {
                            row.child(
                                div()
                                    .flex_shrink_0()
                                    .text_right()
                                    .child(text("[*]").tracking_wider()),
                            )
                        }),
                )
                .when(neutral, |option| {
                    option.child(
                        div()
                            .text_color(if index == self.highlighted {
                                p.txt_primary
                            } else {
                                p.txt_hint
                            })
                            .child(text(t(cx, "style.neutralOptionHint"))),
                    )
                })
        });

        let (inherit_label, inherit_hint) = match self.inherit {
            InheritTarget::GlobalDefault => (
                t_upper(cx, "style.useGlobalDefault"),
                t_with(
                    cx,
                    "style.useGlobalDefaultHint",
                    &[("style", global_default.as_ref())],
                ),
            ),
            InheritTarget::AppOrGlobalDefault => (
                t_upper(cx, "style.useInheritedDefault"),
                t_with(
                    cx,
                    "style.useInheritedDefaultHint",
                    &[("style", global_default.as_ref())],
                ),
            ),
        };

        let inherit_option = div()
            .id(INHERIT_INDEX)
            .w_full()
            .px_3()
            .py_2()
            .border_t_1()
            .border_color(p.border)
            .flex()
            .flex_col()
            .gap_0p5()
            .type_xs()
            .when(self.highlighted == INHERIT_INDEX, |option| {
                option.bg(p.hover)
            })
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if *hovered {
                    this.highlighted = INHERIT_INDEX;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(|this, _, _, cx| this.choose(INHERIT_INDEX, cx)))
            .child(
                div()
                    .text_color(p.txt_primary)
                    .child(text(inherit_label).tracking_wider()),
            )
            .child(
                div()
                    .text_color(if self.highlighted == INHERIT_INDEX {
                        p.txt_primary
                    } else {
                        p.txt_hint
                    })
                    .child(text(inherit_hint)),
            );

        div()
            .id("list")
            .absolute()
            .right_0()
            .w(px(268.))
            .map(|list| {
                if open_upward {
                    list.bottom(relative(1.)).mb_1()
                } else {
                    list.top(relative(1.)).mt_1()
                }
            })
            .occlude()
            .bg(p.surface)
            .border_1()
            .border_color(p.border_strong)
            // Keep focus on the trigger button during mouse-down inside the
            // menu so `on_blur` does not dismiss the menu before `on_click`.
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                this.list_hovered = *hovered;
                cx.notify();
            }))
            .child(
                div()
                    .id("list-options")
                    .max_h(scroll_max_h)
                    .overflow_y_scroll()
                    .track_scroll(&self.list_scroll)
                    .children(options)
                    .child(inherit_option),
            )
            // Remembers where the list is, to tell the clicks outside it.
            .child(
                canvas(move |bounds, _, _| list_bounds.set(bounds), |_, _, _, _| {})
                    .absolute()
                    .inset_0(),
            )
    }
}

impl Render for StyleSelect {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let button_bounds = self.button_bounds.clone();

        div()
            .relative()
            .w_40()
            .flex_shrink_0()
            .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                // The list hangs outside the button's bounds.
                if this.open && !this.list_bounds.get().contains(&event.position) {
                    this.close(cx);
                }
            }))
            .child(
                div()
                    .id("button")
                    .transition_colors()
                    .track_focus(&self.focus_handle)
                    .focus(|button| button.border_color(p.ac))
                    .relative()
                    .on_key_down(cx.listener(Self::handle_key_down))
                    .when(!self.disabled, |button| {
                        button
                            .hover(|button| button.border_color(p.ac))
                            .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                                // Releasing Enter or Space on the focused button
                                // counts as a click; those keys were already handled
                                // when pressed, and a second toggle would undo that.
                                if matches!(event, ClickEvent::Keyboard(_)) {
                                    return;
                                }

                                if this.open {
                                    this.close(cx);
                                } else {
                                    this.open(window, cx);
                                }
                            }))
                    })
                    .w_full()
                    .px_3()
                    .py_1p5()
                    .bg(p.input)
                    .border_1()
                    .border_color(if self.open { p.ac } else { p.border_strong })
                    .type_xs()
                    .text_color(p.txt_primary)
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .justify_between()
                            .when(self.disabled, |inner| inner.opacity(0.5))
                            .child(
                                text(upper(&style_label(&self.value, cx)))
                                    .tracking_wider()
                                    .truncate(),
                            )
                            .child(turning(
                                self.open,
                                icon("chevron-down").size_3().ml_2().text_color(p.txt_muted),
                            )),
                    )
                    .child(
                        canvas(
                            move |bounds, _, _| button_bounds.set(bounds),
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .inset_0(),
                    ),
            )
            .when(self.open && !self.disabled, |select| {
                select.child(deferred(self.render_list(window, cx)))
            })
    }
}
