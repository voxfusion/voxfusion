//! Bulk-entry panel for adding a multiline list of dictionary terms with a
//! live preview of valid additions, duplicates, and invalid entries.

use gpui_kit::component::input::{
    Enter, Escape, IndentInline, InputEvent, OutdentInline, Textarea, TextareaState,
};
use gpui_kit::{
    App, AppContext as _, ClickEvent, Context, Div, Entity, EventEmitter, FocusHandle,
    Focusable as _, InteractiveElement as _, IntoElement, KeyDownEvent, ListAlignment, ListState,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, Window, div, list, prelude::*, px,
};
use std::rc::Rc;

use crate::backend::{
    BulkInvalidReason, BulkWordEntry, BulkWordPreview, BulkWordStatus, MAX_WORD_CHARS,
    parse_bulk_dictionary_words,
};
use crate::ui::motion::Transitions as _;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::focus::{focus_ring, focus_visible};
use crate::ui::widgets::word_list::{Notice, Variant, notice};
use crate::ui::widgets::{icon, spinning};
use crate::ui::{t, t_upper, t_with, upper};

/// Below this threshold the preview sizes tightly to its rows; above it the
/// preview switches to a virtualized GPUI `list` so every row stays scrollable
/// even for large pastes.
const VIRTUALIZE_PREVIEW_AFTER: usize = 7;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BulkAddEvent {
    /// Valid, deduplicated, trimmed terms ready to save.
    Submit(Vec<String>),
    /// User cancelled bulk entry.
    Cancel,
}

/// The row under a single-word form that switches into bulk entry.
pub fn bulk_add_trigger(
    variant: Variant,
    focus_handle: &FocusHandle,
    disabled: bool,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    window: &Window,
    cx: &App,
) -> Div {
    let p = palette(cx);
    let focused = !disabled && focus_visible(focus_handle, window);

    let button = div()
        .id("open-bulk-add")
        .transition_colors()
        .relative()
        .flex_shrink_0()
        .type_xs()
        .border_1()
        .border_color(p.border_strong)
        .text_color(p.txt_secondary)
        .map(|button| match variant {
            Variant::Page => button.px_3().py_1p5(),
            Variant::Nested => button.px_2p5().py_1(),
        });

    let button = if disabled {
        button.opacity(0.5)
    } else {
        button
            .track_focus(focus_handle)
            .hover(|button| button.border_color(p.ac).text_color(p.ac))
            .on_click(on_open)
    };

    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_3()
        .map(|row| match variant {
            Variant::Page => row.mt_3().pt_3().border_t_1().border_color(p.border),
            Variant::Nested => row.pt_1(),
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .type_xs()
                .text_color(p.txt_hint)
                .child(text(t(cx, "dictionary.bulkEntryHint"))),
        )
        .child(
            button
                .child(text(t_upper(cx, "dictionary.bulkAddButton")).tracking_wider())
                .when(focused, |button| button.child(focus_ring(2., cx))),
        )
}

/// Escapes control characters for preview display so hidden invalid characters
/// remain visible and safe to shape.
fn display_preview_term(term: &str) -> String {
    if !term.chars().any(char::is_control) {
        return term.to_string();
    }

    let mut out = String::with_capacity(term.len());
    for ch in term.chars() {
        if ch.is_control() {
            for escaped in ch.escape_default() {
                out.push(escaped);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn render_preview_row(index: usize, entry: &BulkWordEntry, cx: &App) -> Div {
    let p = palette(cx);

    let (badge_label, badge_color, term_color, reason) = match entry.status {
        BulkWordStatus::Valid => (
            t_upper(cx, "dictionary.bulkStatusNew"),
            p.success,
            p.txt_primary,
            None,
        ),
        BulkWordStatus::DuplicateExisting => (
            t_upper(cx, "dictionary.bulkStatusExisting"),
            p.txt_hint,
            p.txt_secondary,
            None,
        ),
        BulkWordStatus::DuplicateInInput => (
            t_upper(cx, "dictionary.bulkStatusDuplicate"),
            p.txt_hint,
            p.txt_secondary,
            None,
        ),
        BulkWordStatus::Invalid(reason) => {
            let message = match reason {
                BulkInvalidReason::TooLong { count } => t_with(
                    cx,
                    "dictionary.wordTooLong",
                    &[
                        ("max", &MAX_WORD_CHARS.to_string()),
                        ("count", &count.to_string()),
                    ],
                ),
                BulkInvalidReason::ControlCharacters => t(cx, "dictionary.wordInvalidChars"),
            };
            (
                t_upper(cx, "dictionary.bulkStatusInvalid"),
                p.ac,
                p.txt_primary,
                Some(message),
            )
        }
    };

    let line_label = format!("L{}", entry.line);
    let term = display_preview_term(&entry.term);

    div()
        .px_3()
        .py_1p5()
        .when(index > 0, |row| row.border_t_1().border_color(p.border))
        .flex()
        .flex_col()
        .gap_0p5()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .type_xs()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_color(p.txt_hint)
                                .child(text(line_label).nowrap()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_color(term_color)
                                .child(text(term).truncate()),
                        ),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_color(badge_color)
                        .child(text(format!("[{badge_label}]")).tracking_wider().nowrap()),
                ),
        )
        .children(reason.map(|message| div().type_xs().text_color(p.ac).child(text(message))))
}

pub struct BulkAddPanel {
    variant: Variant,
    textarea: Entity<TextareaState>,
    existing_words: Vec<String>,
    preview: Rc<BulkWordPreview>,
    preview_list: ListState,
    saving: bool,
    submit_blocked: bool,
    error: Option<Notice>,
    focus_handle: FocusHandle,
    cancel_focus: FocusHandle,
    save_focus: FocusHandle,
    _subscription: Subscription,
}

impl EventEmitter<BulkAddEvent> for BulkAddPanel {}

impl BulkAddPanel {
    pub fn new(
        variant: Variant,
        existing_words: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let textarea = cx.new(|cx| {
            let state = TextareaState::new(window, cx)
                .auto_grow(4, 8)
                .soft_wrap(true);
            state.focus(window, cx);
            state
        });

        let subscription = cx.subscribe(&textarea, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.error = None;
                this.recompute_preview(cx);
                cx.notify();
            }
        });

        Self {
            variant,
            textarea,
            existing_words,
            preview: Rc::new(BulkWordPreview::default()),
            preview_list: ListState::new(0, ListAlignment::Top, px(120.)),
            saving: false,
            submit_blocked: false,
            error: None,
            focus_handle: cx.focus_handle(),
            cancel_focus: cx.focus_handle().tab_stop(true),
            save_focus: cx.focus_handle().tab_stop(true),
            _subscription: subscription,
        }
    }

    /// Updates the set of words already stored in this dictionary and refreshes
    /// the live preview.
    pub fn set_existing_words(&mut self, existing_words: Vec<String>, cx: &mut Context<Self>) {
        self.existing_words = existing_words;
        self.recompute_preview(cx);
        cx.notify();
    }

    pub fn set_saving(&mut self, saving: bool, cx: &mut Context<Self>) {
        self.saving = saving;
        cx.notify();
    }

    /// Blocks submitting while earlier optimistic dictionary edits or
    /// deletions are still settling in storage.
    pub fn set_submit_blocked(&mut self, blocked: bool, cx: &mut Context<Self>) {
        if self.submit_blocked != blocked {
            self.submit_blocked = blocked;
            cx.notify();
        }
    }

    pub fn set_error(&mut self, error: Option<Notice>, cx: &mut Context<Self>) {
        self.error = error;
        cx.notify();
    }

    /// Whether keyboard focus currently sits inside this panel, including a
    /// button temporarily detached from the focus tree while saving.
    pub fn contains_focus(&self, window: &Window, cx: &App) -> bool {
        self.focus_handle.contains_focused(window, cx)
            || self.textarea.focus_handle(cx).is_focused(window)
            || self.cancel_focus.is_focused(window)
            || self.save_focus.is_focused(window)
    }

    fn recompute_preview(&mut self, cx: &App) {
        let value = self.textarea.read(cx).value();
        let preview =
            parse_bulk_dictionary_words(&value, self.existing_words.iter().map(String::as_str));
        self.preview_list.reset(preview.entries.len());
        self.preview = Rc::new(preview);
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        cx.emit(BulkAddEvent::Cancel);
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.saving || self.submit_blocked || self.preview.valid_terms.is_empty() {
            return;
        }
        self.error = None;
        cx.emit(BulkAddEvent::Submit(self.preview.valid_terms.clone()));
    }

    fn save_label(&self, cx: &App) -> SharedString {
        match self.preview.valid_terms.len() {
            0 => t(cx, "dictionary.bulkSaveEmpty"),
            1 => t(cx, "dictionary.bulkSaveOne"),
            count => t_with(
                cx,
                "dictionary.bulkSaveCount",
                &[("count", &count.to_string())],
            ),
        }
    }

    fn render_preview(&self, cx: &App) -> Option<Div> {
        if self.preview.entries.is_empty() {
            return None;
        }

        let p = palette(cx);
        let valid_count = self.preview.valid_terms.len();
        let existing_count = self.preview.duplicate_existing_count;
        let dup_input_count = self.preview.duplicate_input_count;
        let invalid_count = self.preview.invalid_count;

        let mut summary_items = div().flex().flex_wrap().items_center().gap_3().type_xs();
        summary_items = summary_items.child(
            div()
                .text_color(if valid_count > 0 {
                    p.success
                } else {
                    p.txt_hint
                })
                .child(text(t_with(
                    cx,
                    "dictionary.bulkSummaryToAdd",
                    &[("count", &valid_count.to_string())],
                ))),
        );
        if existing_count > 0 {
            summary_items = summary_items.child(div().text_color(p.txt_hint).child(text(t_with(
                cx,
                "dictionary.bulkSummaryExisting",
                &[("count", &existing_count.to_string())],
            ))));
        }
        if dup_input_count > 0 {
            summary_items = summary_items.child(div().text_color(p.txt_hint).child(text(t_with(
                cx,
                "dictionary.bulkSummaryDuplicate",
                &[("count", &dup_input_count.to_string())],
            ))));
        }
        if invalid_count > 0 {
            summary_items = summary_items.child(div().text_color(p.ac).child(text(t_with(
                cx,
                "dictionary.bulkSummaryInvalid",
                &[("count", &invalid_count.to_string())],
            ))));
        }

        let list_bg = match self.variant {
            Variant::Page => p.base,
            Variant::Nested => p.input,
        };

        let preview_body = if self.preview.entries.len() <= VIRTUALIZE_PREVIEW_AFTER {
            let rows = self
                .preview
                .entries
                .iter()
                .enumerate()
                .map(|(index, entry)| render_preview_row(index, entry, cx));

            div()
                .id("bulk-preview-list")
                .max_h(px(200.))
                .overflow_y_scroll()
                .children(rows)
                .into_any_element()
        } else {
            let preview = self.preview.clone();
            list(self.preview_list.clone(), move |index, _, cx| {
                render_preview_row(index, &preview.entries[index], cx).into_any_element()
            })
            .h(px(200.))
            .w_full()
            .into_any_element()
        };

        Some(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(div().type_xs().text_color(p.txt_secondary).child(
                            text(t_upper(cx, "dictionary.bulkPreviewHeading")).tracking_wider(),
                        ))
                        .child(summary_items),
                )
                // Border stays on the outer wrapper so the inner scroll range
                // stays accurate.
                .child(
                    div()
                        .bg(list_bg)
                        .border_1()
                        .border_color(p.border)
                        .child(preview_body),
                ),
        )
    }
}

impl Render for BulkAddPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let placeholder = t(cx, "dictionary.bulkAddPlaceholder");
        if self.textarea.read(cx).presentation().placeholder() != &placeholder {
            self.textarea.update(cx, |input, cx| {
                input.set_placeholder(placeholder, window, cx);
            });
        }

        let p = palette(cx);
        let textarea_focused = self.textarea.focus_handle(cx).is_focused(window);
        let save_disabled =
            self.preview.valid_terms.is_empty() || self.saving || self.submit_blocked;
        let cancel_focused = !self.saving && focus_visible(&self.cancel_focus, window);
        let save_focused = !save_disabled && focus_visible(&self.save_focus, window);

        let cancel_button = div()
            .id("cancel-bulk-add")
            .transition_colors()
            .relative()
            .px_3()
            .py_1p5()
            .type_xs()
            .border_1()
            .border_color(p.border_strong)
            .text_color(p.txt_secondary)
            .map(|button| {
                if self.saving {
                    button.opacity(0.5)
                } else {
                    button
                        .track_focus(&self.cancel_focus)
                        .hover(|button| button.border_color(p.txt_muted).text_color(p.txt_primary))
                        .on_click(cx.listener(|this, _, _, cx| this.cancel(cx)))
                }
            })
            .child(text(t_upper(cx, "dictionary.cancel")).tracking_wider())
            .when(cancel_focused, |button| button.child(focus_ring(2., cx)));

        let save_label = upper(&self.save_label(cx));
        let save_button = div()
            .id("save-bulk-add")
            .transition_colors()
            .relative()
            .flex()
            .items_center()
            .gap_1p5()
            .px_3()
            .py_1p5()
            .type_xs()
            .map(|button| {
                if save_disabled {
                    button
                        .bg(p.ac.opacity(0.5))
                        .hover(|button| button.bg(p.ac_hover.opacity(0.5)))
                        .text_color(p.surface.blend(p.ac_on.opacity(0.5)))
                } else {
                    button
                        .track_focus(&self.save_focus)
                        .bg(p.ac)
                        .hover(|button| button.bg(p.ac_hover))
                        .text_color(p.ac_on)
                        .on_click(cx.listener(|this, _, _, cx| this.submit(cx)))
                }
            })
            .when(self.saving, |button| {
                button.child(spinning("bulk-saving-spinner", icon("loader").size_3p5()))
            })
            .when(!self.saving, |button| {
                button.child(text("+").tracking_wider())
            })
            .child(text(save_label).tracking_wider())
            .when(save_focused, |button| button.child(focus_ring(2., cx)));

        div()
            .id("bulk-add-panel")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &Escape, _, cx| {
                this.cancel(cx);
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    this.cancel(cx);
                    cx.stop_propagation();
                }
            }))
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .type_xs()
                            .text_color(p.txt_secondary)
                            .child(text(t_upper(cx, "dictionary.bulkAddTitle")).tracking_wider()),
                    )
                    .child(
                        div()
                            .type_xs()
                            .text_color(p.txt_hint)
                            .child(text(t(cx, "dictionary.bulkAddInstructions"))),
                    ),
            )
            .child(
                div()
                    .id("bulk-words-input")
                    .transition_colors()
                    .w_full()
                    .bg(p.input)
                    .border_1()
                    .border_color(if textarea_focused {
                        p.ac
                    } else {
                        p.border_strong
                    })
                    .text_color(p.txt_primary)
                    .font_mono()
                    .type_sm()
                    .capture_action(cx.listener(|this, action: &Enter, _, cx| {
                        if action.secondary && !action.shift {
                            cx.stop_propagation();
                            this.submit(cx);
                        }
                    }))
                    .on_action(|_: &IndentInline, window, cx| {
                        window.focus_next(cx);
                    })
                    .on_action(|_: &OutdentInline, window, cx| {
                        window.focus_prev(cx);
                    })
                    .child(
                        Textarea::new(&self.textarea)
                            .appearance(false)
                            .disabled(self.saving)
                            .text_size(px(14.))
                            .line_height(px(20.)),
                    ),
            )
            .children(self.render_preview(cx))
            .children(self.error.as_ref().map(|error| notice(error, cx)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .pt_1()
                    .child(
                        div().type_xs().text_color(p.txt_hint).child(
                            text(t_upper(cx, "dictionary.bulkShortcutHint")).tracking_wide(),
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(cancel_button)
                            .child(save_button),
                    ),
            )
    }
}
