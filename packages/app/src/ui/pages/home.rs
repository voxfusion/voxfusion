//! The Home page: the history of transcriptions, grouped by day, with live
//! search across all saved dictations.

use gpui_kit::{
    App, AppContext as _, ClickEvent, ClipboardItem, Context, Entity, FocusHandle, Focusable,
    FontWeight, Hsla, InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, Task, Window, canvas, div, prelude::*, px,
};
use std::collections::HashSet;
use std::time::Duration;
use unicode_segmentation::UnicodeSegmentation;

use crate::analytics;
use crate::backend::{
    self, AppEvent, Transcription, TranscriptionCursor, command_error, fold_search_char,
    fold_search_text,
};
use crate::events;
use crate::settings::SettingsStore;
use crate::ui::datetime::{self, format_long_date, format_time, same_day};
use crate::ui::grid::above_grid;
use crate::ui::hotkeys::hotkey_display_name;
use crate::ui::motion::Transitions as _;
use crate::ui::text::{Text, TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::focus::{focus_ring, focus_visible, keyed_focus_handle};
use crate::ui::widgets::text_field::{TextField, TextFieldEvent, field_box};
use crate::ui::widgets::{icon, spinning, turning};
use crate::ui::{locale, t, t_upper, t_with, upper};

const PAGE_SIZE: i64 = 20;
const PREVIEW_MAX_CHARS: usize = 150;
const PREVIEW_MAX_LINES: usize = 3;
/// How far below the visible area the end of the list may be before the next
/// page is requested.
const LOAD_MORE_MARGIN: f32 = 100.;
/// Short delay while typing before querying storage.
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(120);
/// Maximum length (in UTF-16 code units) of a dictation preview on Home.
const PREVIEW_MAX_UNITS: usize = 150;
/// Context kept before a match when windowing a long dictation preview.
const SNIPPET_LEADING_UNITS: usize = 40;

pub struct HomePage {
    search_field: Entity<TextField>,
    focus_handle: FocusHandle,
    /// Trimmed query currently entered in the search field.
    query: String,
    /// Trimmed query that `transcriptions`, `total_count`, and `error` belong to.
    results_query: String,
    transcriptions: Vec<Transcription>,
    /// Total matching transcriptions across all pages when `results_query` is
    /// non-empty.
    total_count: Option<usize>,
    loading: bool,
    initial_loading: bool,
    next_cursor: Option<TranscriptionCursor>,
    has_more: bool,
    error: Option<SharedString>,
    hovered: Option<String>,
    retry_hovered: bool,
    copied: HashSet<String>,
    expanded: HashSet<String>,
    /// Transcriptions deleted here. A list fetched before the deletion
    /// reached the database leaves them out.
    deleted: HashSet<String>,
    /// Monotonically increasing request id; stale async responses are dropped.
    request_id: u64,
    /// Counts the times the history was cleared; failed deletions before the
    /// last clear do not restore expanded state.
    clear_generation: u64,
    /// Pending debounced search task; dropped automatically when superseded.
    debounce_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl HomePage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        analytics::capture(cx, "$pageview", &[("$current_url", "/".into())]);

        let search_field = cx.new(|cx| {
            TextField::new(window, cx).placeholder(|cx| t(cx, "home.searchPlaceholder"))
        });

        let subscriptions = vec![
            cx.subscribe_in(&events::hub(cx), window, |this, _, event, window, cx| {
                match event {
                    AppEvent::TranscriptionCreated => {
                        analytics::capture(cx, "transcription_created", &[]);
                        this.start_fetch(None, cx);
                    }
                    AppEvent::TranscriptionsCleared => {
                        this.request_id += 1;
                        this.clear_generation += 1;
                        this.debounce_task = None;
                        this.query.clear();
                        this.results_query.clear();
                        this.search_field
                            .update(cx, |field, cx| field.set_value("", window, cx));
                        this.transcriptions.clear();
                        this.total_count = None;
                        this.deleted.clear();
                        this.copied.clear();
                        this.expanded.clear();
                        this.hovered = None;
                        this.next_cursor = None;
                        this.has_more = false;
                        this.error = None;
                        this.loading = false;
                        this.start_fetch(None, cx);
                    }
                    _ => {}
                }
            }),
            cx.subscribe_in(
                &search_field,
                window,
                |this, _, event: &TextFieldEvent, window, cx| match event {
                    TextFieldEvent::Change => this.handle_search_change(cx),
                    TextFieldEvent::Enter => this.submit_search_now(cx),
                    TextFieldEvent::Escape => this.handle_search_escape(window, cx),
                    TextFieldEvent::Focus | TextFieldEvent::Blur => cx.notify(),
                },
            ),
            cx.observe(&SettingsStore::entity(cx), |_, _, cx| cx.notify()),
        ];

        let mut page = Self {
            search_field,
            focus_handle: cx.focus_handle(),
            query: String::new(),
            results_query: String::new(),
            transcriptions: Vec::new(),
            total_count: None,
            loading: false,
            initial_loading: true,
            next_cursor: None,
            has_more: true,
            error: None,
            hovered: None,
            retry_hovered: false,
            copied: HashSet::new(),
            expanded: HashSet::new(),
            deleted: HashSet::new(),
            request_id: 0,
            clear_generation: 0,
            debounce_task: None,
            _subscriptions: subscriptions,
        };
        page.start_fetch(None, cx);
        page
    }

    /// Focuses the search field and selects any existing text so typing
    /// replaces it right away.
    pub fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_field.update(cx, |field, cx| {
            field.focus(window, cx);
            if !field.value(cx).is_empty() {
                field.select_all(window, cx);
            }
        });
        cx.notify();
    }

    fn handle_search_change(&mut self, cx: &mut Context<Self>) {
        let trimmed = self.search_field.read(cx).value(cx).trim().to_string();
        if trimmed == self.query {
            cx.notify();
            return;
        }

        self.query = trimmed;
        self.error = None;
        self.hovered = None;
        self.debounce_task = None;

        if self.query.is_empty() {
            self.start_fetch(None, cx);
            return;
        }

        // Invalidate any in-flight fetch for an earlier query right away.
        self.request_id += 1;
        self.loading = false;
        let req_id = self.request_id;
        cx.notify();

        self.debounce_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SEARCH_DEBOUNCE).await;
            this.update(cx, |this, cx| {
                if this.request_id != req_id {
                    return;
                }
                this.debounce_task = None;
                this.start_fetch(None, cx);
            })
            .ok();
        }));
    }

    fn submit_search_now(&mut self, cx: &mut Context<Self>) {
        if self.debounce_task.is_some() || self.results_query != self.query || self.error.is_some()
        {
            self.start_fetch(None, cx);
        }
    }

    fn handle_search_escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_search(false, window, cx);
    }

    fn clear_search(&mut self, refocus: bool, window: &mut Window, cx: &mut Context<Self>) {
        let had_raw = !self.search_field.read(cx).value(cx).is_empty();
        let had_query = !self.query.is_empty() || !self.results_query.is_empty();

        if had_raw {
            self.search_field
                .update(cx, |field, cx| field.set_value("", window, cx));
        }

        if refocus {
            self.search_field
                .update(cx, |field, cx| field.focus(window, cx));
        } else {
            window.focus(&self.focus_handle, cx);
        }

        if had_query {
            self.query.clear();
            self.error = None;
            self.hovered = None;
            self.start_fetch(None, cx);
        } else {
            cx.notify();
        }
    }

    fn start_fetch(&mut self, cursor: Option<TranscriptionCursor>, cx: &mut Context<Self>) {
        self.debounce_task = None;
        self.request_id += 1;
        let req_id = self.request_id;
        let query = self.query.clone();
        let is_load_more = cursor.is_some();

        self.loading = true;
        self.error = None;
        cx.notify();

        let command_name = if query.is_empty() {
            "list_transcriptions"
        } else {
            "search_transcriptions"
        };

        let request = backend::call(cx, {
            let query = query.clone();
            let cursor = cursor.clone();
            move |backend| {
                if query.is_empty() {
                    backend.list_transcriptions(PAGE_SIZE, cursor.map(|c| c.created_at))
                } else {
                    backend.search_transcriptions(&query, PAGE_SIZE, cursor)
                }
            }
        });

        cx.spawn(async move |this, cx| {
            let result = request.await;

            this.update(cx, |this, cx| {
                if req_id != this.request_id {
                    return;
                }

                this.loading = false;
                this.initial_loading = false;

                match result {
                    Ok(page) => {
                        this.results_query = query;
                        this.next_cursor =
                            page.transcriptions.last().map(|item| TranscriptionCursor {
                                created_at: item.created_at.clone(),
                                id: item.id.clone(),
                            });
                        this.has_more = page.has_more;

                        let raw_len = page.transcriptions.len();
                        let listed: Vec<Transcription> = page
                            .transcriptions
                            .into_iter()
                            .filter(|item| !this.deleted.contains(&item.id))
                            .collect();
                        let dropped = raw_len - listed.len();

                        if is_load_more {
                            this.transcriptions.extend(listed);
                            if let Some(total) = this.total_count {
                                this.total_count = Some(total.max(this.transcriptions.len()));
                            }
                        } else {
                            this.transcriptions = listed;
                            this.total_count = page.total_count.map(|total| {
                                total
                                    .saturating_sub(dropped)
                                    .max(this.transcriptions.len())
                            });
                        }
                        if !this.has_more
                            && this.results_query.is_empty()
                            && this.debounce_task.is_none()
                        {
                            this.expanded.retain(|id| {
                                this.transcriptions
                                    .iter()
                                    .any(|transcription| transcription.id == *id)
                            });
                        }
                    }
                    Err(error) => {
                        this.results_query = query;
                        if !is_load_more {
                            this.transcriptions.clear();
                            this.total_count = None;
                            this.next_cursor = None;
                            this.has_more = false;
                        }
                        this.error = Some(command_error(command_name, &error).into());
                    }
                }

                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn load_more(&mut self, cx: &mut Context<Self>) {
        if !self.has_more
            || self.loading
            || self.debounce_task.is_some()
            || self.error.is_some()
            || self.results_query != self.query
        {
            return;
        }
        if let Some(cursor) = self.next_cursor.clone() {
            self.start_fetch(Some(cursor), cx);
        }
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
        self.error = None;
        if !self.transcriptions.is_empty()
            && self.has_more
            && self.next_cursor.is_some()
            && self.results_query == self.query
        {
            self.load_more(cx);
        } else {
            self.start_fetch(None, cx);
        }
    }

    fn copy(&mut self, transcription: &Transcription, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(transcription.text.clone()));
        analytics::capture(
            cx,
            "transcription_copied",
            &[
                ("transcription_id", transcription.id.clone().into()),
                (
                    "text_length",
                    transcription.text.encode_utf16().count().into(),
                ),
            ],
        );

        let id = transcription.id.clone();
        self.copied.insert(id.clone());
        cx.notify();

        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(2)).await;
            this.update(cx, |this, cx| {
                this.copied.remove(&id);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn toggle_expanded(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.expanded.remove(id) {
            self.expanded.insert(id.to_string());
        }
        cx.notify();
    }

    fn delete(&mut self, id: &str, cx: &mut Context<Self>) {
        analytics::capture(cx, "transcription_deleted", &[]);
        let before = self.transcriptions.len();
        self.transcriptions
            .retain(|transcription| transcription.id != id);
        if self.transcriptions.len() < before
            && let Some(total) = &mut self.total_count
        {
            *total = total.saturating_sub(1);
        }
        self.copied.remove(id);
        let was_expanded = self.expanded.remove(id);
        self.deleted.insert(id.to_string());
        self.hovered = None;
        cx.notify();

        let id = id.to_string();
        let request_id_at_start = self.request_id;
        let fetch_was_in_flight = self.loading;
        let request = backend::call(cx, {
            let id = id.clone();
            move |backend| backend.delete_transcription(&id)
        });
        let clear_generation = self.clear_generation;

        cx.spawn(async move |this, cx| {
            let result = request.await;

            this.update(cx, |this, cx| {
                // A refused deletion brings the transcription back. With every
                // listed one deleted, or if a search fetch overlapped this
                // deletion, re-fetch to stay in sync with storage.
                if let Err(error) = &result {
                    log::error!(target: "home", "delete_transcription_failed error={error}");
                    this.deleted.remove(&id);
                    if was_expanded && clear_generation == this.clear_generation {
                        this.expanded.insert(id.clone());
                    }
                    this.start_fetch(None, cx);
                } else if (this.transcriptions.is_empty() && this.has_more)
                    || (!this.query.is_empty()
                        && (fetch_was_in_flight || this.request_id != request_id_at_start))
                {
                    this.start_fetch(None, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn date_label(&self, created_at: &str, cx: &Context<Self>) -> SharedString {
        let Some(date) = datetime::parse(created_at) else {
            return created_at.to_string().into();
        };
        let today = datetime::now();
        let yesterday = today - chrono::Duration::days(1);

        if same_day(&date, &today) {
            t(cx, "transcriptionList.today")
        } else if same_day(&date, &yesterday) {
            t(cx, "transcriptionList.yesterday")
        } else {
            format_long_date(&date, locale(cx)).into()
        }
    }

    /// Transcriptions in order, grouped under their day's label.
    fn groups(&self, cx: &Context<Self>) -> Vec<(SharedString, Vec<Transcription>)> {
        let mut groups: Vec<(SharedString, Vec<Transcription>)> = Vec::new();

        for transcription in &self.transcriptions {
            let label = self.date_label(&transcription.created_at, cx);

            match groups.iter_mut().find(|(existing, _)| *existing == label) {
                Some((_, items)) => items.push(transcription.clone()),
                None => groups.push((label, vec![transcription.clone()])),
            }
        }

        groups
    }

    fn search_pending(&self) -> bool {
        self.debounce_task.is_some() || (self.loading && self.results_query != self.query)
    }

    fn showing_search_results(&self) -> bool {
        !self.results_query.is_empty() && self.results_query == self.query
    }

    fn render_search_bar(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let search_focus = self.search_field.focus_handle(cx);
        let focused = search_focus.is_focused(window);
        let has_raw_query = !self.search_field.read(cx).value(cx).is_empty();
        let search_busy = !self.query.is_empty() && self.search_pending();
        let clear_button = has_raw_query.then(|| {
            let clear_focus = keyed_focus_handle("home-clear-search", window, cx);
            let clear_focused = focus_visible(&clear_focus, window);

            // Enter and Space on the focused button arrive as a click.
            div()
                .id("clear-search")
                .transition_colors()
                .track_focus(&clear_focus)
                .relative()
                .mr_2()
                .p_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(p.txt_hint)
                .hover(|button| button.text_color(p.ac))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.clear_search(true, window, cx);
                }))
                .child(icon("x").size_3p5())
                .when(clear_focused, |button| button.child(focus_ring(2., cx)))
        });

        let leading_icon = if search_busy {
            spinning("search-loader", icon("loader").size_4().text_color(p.ac)).into_any_element()
        } else {
            icon("search")
                .size_4()
                .text_color(if focused { p.ac } else { p.txt_hint })
                .into_any_element()
        };

        div()
            .id("home-search-bar")
            .transition_colors()
            .bg(p.surface)
            .border_1()
            .border_color(if focused { p.ac } else { p.border })
            .hover(|bar| bar.border_color(if focused { p.ac } else { p.border_strong }))
            .flex()
            .items_center()
            .child(
                div()
                    .pl_3()
                    .py_2p5()
                    .flex()
                    .items_center()
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        window.focus(&search_focus, cx);
                        window.prevent_default();
                    })
                    .child(leading_icon),
            )
            .child(
                field_box(&self.search_field, cx)
                    .flex_1()
                    .min_w_0()
                    .px_3()
                    .py_2p5()
                    .type_sm()
                    .text_color(p.txt_primary),
            )
            .children(clear_button)
            .when(!has_raw_query && !focused, |bar| {
                bar.child(
                    div()
                        .mr_3()
                        .px_1p5()
                        .py_0p5()
                        .border_1()
                        .border_color(p.border)
                        .bg(p.base)
                        .type_xs()
                        .text_color(p.txt_hint)
                        .child(text("⌘F")),
                )
            })
    }

    fn render_card(
        &self,
        index: usize,
        transcription: &Transcription,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let p = palette(cx);
        let id = transcription.id.clone();
        let hovered = self.hovered.as_deref() == Some(id.as_str());
        let copied = self.copied.contains(&id);
        let normalized = normalize_newlines(&transcription.text);
        let (expandable, collapsed_preview) = if self.results_query.is_empty() {
            let preview = preview_text(&transcription.text, PREVIEW_MAX_CHARS, PREVIEW_MAX_LINES);
            (preview.truncated, Some(preview.text))
        } else {
            (normalized.encode_utf16().count() > PREVIEW_MAX_UNITS, None)
        };
        let expanded = expandable && self.expanded.contains(&id);
        let body_text = if self.results_query.is_empty() {
            if expanded {
                text(normalized)
            } else {
                text(collapsed_preview.unwrap_or(normalized))
            }
        } else if expanded {
            highlighted_text(&normalized, &self.results_query, p.ac_text)
        } else {
            let snippet = snippet_for_query(&normalized, &self.results_query, PREVIEW_MAX_UNITS);
            highlighted_text(&snippet, &self.results_query, p.ac_text)
        };
        let time = datetime::parse(&transcription.created_at)
            .map(|time| format_time(&time, locale(cx)))
            .unwrap_or_default();

        let toggle = expandable.then(|| {
            let toggle_focus = keyed_focus_handle(
                SharedString::from(format!("transcription-toggle-focus-{id}")),
                window,
                cx,
            );
            let toggle_label = t_upper(
                cx,
                if expanded {
                    "transcription.showLess"
                } else {
                    "transcription.showMore"
                },
            );
            let toggle_id = id.clone();

            // Stop mouse down before the card body so clicking the button does
            // not also arm the card's click handler.
            div()
                .flex_shrink_0()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .id(SharedString::from(format!("transcription-toggle-{id}")))
                        .transition_colors()
                        .track_focus(&toggle_focus)
                        .relative()
                        .flex()
                        .items_center()
                        .gap_1()
                        .cursor_pointer()
                        .text_color(p.txt_secondary)
                        .hover(|button| button.text_color(p.txt_primary))
                        // Enter and Space on the focused toggle arrive as a click.
                        .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            if event.click_count() == 1 {
                                this.toggle_expanded(&toggle_id, cx);
                            }
                        }))
                        .child(text(toggle_label).tracking_wider())
                        .child(turning(expanded, icon("chevron-down").size_3p5()))
                        .when(focus_visible(&toggle_focus, window), |button| {
                            button.child(focus_ring(3., cx))
                        }),
                )
        });

        let body = div()
            .p_3()
            .child(
                div()
                    .mb_2()
                    .type_sm()
                    .leading_relaxed(14.)
                    .text_color(p.txt_primary)
                    .child(body_text.pre_wrap().break_words()),
            )
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
                            .gap_3()
                            .text_color(p.txt_muted)
                            .child(text(time))
                            .child(text(format_processing_time(
                                transcription.processing_time_ms,
                            ))),
                    )
                    .children(toggle),
            );

        let body = if expandable {
            let body_id = id.clone();
            body.id(SharedString::from(format!("transcription-body-{id}")))
                .cursor_pointer()
                .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                    // A double click on a long entry opens it once instead of
                    // opening and immediately closing it.
                    if event.click_count() == 1 {
                        this.toggle_expanded(&body_id, cx);
                    }
                }))
                .into_any_element()
        } else {
            body.into_any_element()
        };

        div()
            .id(SharedString::from(format!("transcription-{id}")))
            .relative()
            .when(index > 0, |card| card.border_t_1().border_color(p.border))
            .hover(|card| card.bg(p.hover))
            .when(hovered, |card| card.child(above_grid(cx)))
            .on_hover(cx.listener({
                let id = id.clone();
                move |this, hovered: &bool, _, cx| {
                    if *hovered {
                        this.hovered = Some(id.clone());
                    } else if this.hovered.as_deref() == Some(id.as_str()) {
                        this.hovered = None;
                    }
                    cx.notify();
                }
            }))
            .child(body)
            .when(hovered || copied, |card| {
                let transcription = transcription.clone();
                let button = |id: &'static str| {
                    div()
                        .id(id)
                        .p_1p5()
                        .bg(p.base)
                        .border_1()
                        .border_color(p.border_strong)
                        .transition_colors()
                        .cursor_pointer()
                        .text_color(p.txt_secondary)
                };

                card.child(
                    div()
                        .absolute()
                        .top_2()
                        .right_2()
                        .flex()
                        .gap_1()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .when(hovered, |buttons| {
                            buttons.child(
                                button("delete")
                                    .hover(|button| button.border_color(p.ac).text_color(p.ac))
                                    .on_click(cx.listener(
                                        move |this, event: &ClickEvent, _, cx| {
                                            cx.stop_propagation();
                                            // The next transcription moves up under the
                                            // pointer; a double click deletes only one.
                                            if event.click_count() == 1 {
                                                this.delete(&id, cx);
                                            }
                                        },
                                    ))
                                    .child(icon("trash-2").size_3p5()),
                            )
                        })
                        .child(
                            button("copy")
                                .hover(|button| button.border_color(p.ac))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.copy(&transcription, cx);
                                }))
                                .child(if copied {
                                    icon("check").size_3p5().text_color(p.success)
                                } else {
                                    icon("copy").size_3p5()
                                }),
                        ),
                )
            })
    }

    fn render_list(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let hotkey = hotkey_display_name(&SettingsStore::get(cx).hotkey);
        let has_items = !self.transcriptions.is_empty();
        let search_pending = self.search_pending();
        let showing_search = self.showing_search_results();

        let mut list = div().flex().flex_col().gap_6();

        if self.initial_loading {
            list = list.child(div().flex().items_center().justify_center().py_12().child(
                spinning("initial-loader", icon("loader").size_6().text_color(p.ac)),
            ));
        }

        if let Some(error) = self.error.clone() {
            list = list.child(
                div()
                    .py_8()
                    .type_base()
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(
                        div()
                            .mb_2()
                            .text_color(p.ac)
                            .child(text(format!("[ERROR] {error}")).center()),
                    )
                    .child(
                        div()
                            .id("retry")
                            .text_color(p.ac)
                            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                                this.retry_hovered = *hovered;
                                cx.notify();
                            }))
                            .on_click(cx.listener(|this, _, _, cx| this.retry(cx)))
                            .child(text("[RETRY]").underline(self.retry_hovered)),
                    ),
            );
        }

        if !self.initial_loading && self.error.is_none() && !has_items {
            if search_pending || self.loading || self.has_more {
                list = list.child(div().flex().items_center().justify_center().py_12().child(
                    spinning("search-list-loader", icon("loader").size_5().text_color(p.ac)),
                ));
            } else if showing_search {
                let clear_empty_focus = keyed_focus_handle("home-clear-search-empty", window, cx);
                let clear_empty_focused = focus_visible(&clear_empty_focus, window);

                list = list.child(
                    div()
                        .py_12()
                        .flex()
                        .flex_col()
                        .items_center()
                        .child(
                            div()
                                .mb_2()
                                .type_base()
                                .text_color(p.txt_secondary)
                                .child(text("[INFO] NO_MATCHES").center()),
                        )
                        .child(
                            div().max_w(px(480.)).type_sm().text_color(p.txt_hint).child(
                                text(t_with(
                                    cx,
                                    "home.searchNoMatches",
                                    &[("query", &self.results_query)],
                                ))
                                .center(),
                            ),
                        )
                        .child(
                            // Enter and Space on the focused button arrive as a click.
                            div()
                                .id("clear-search-empty")
                                .transition_colors()
                                .track_focus(&clear_empty_focus)
                                .relative()
                                .mt_4()
                                .px_3()
                                .py_1p5()
                                .type_xs()
                                .border_1()
                                .border_color(p.border_strong)
                                .text_color(p.txt_secondary)
                                .hover(|button| {
                                    button
                                        .border_color(p.ac)
                                        .bg(p.ac_bg)
                                        .text_color(p.txt_primary)
                                })
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.clear_search(false, window, cx);
                                }))
                                .child(text(t_upper(cx, "home.clearSearch")).tracking_wider())
                                .when(clear_empty_focused, |button| {
                                    button.child(focus_ring(3., cx))
                                }),
                        ),
                );
            } else {
                list = list.child(
                    div()
                        .py_12()
                        .flex()
                        .flex_col()
                        .items_center()
                        .child(
                            div()
                                .mb_2()
                                .type_base()
                                .text_color(p.txt_secondary)
                                .child(text("[INFO] NO_TRANSCRIPTIONS").center()),
                        )
                        .child(
                            div().type_sm().text_color(p.txt_muted).child(
                                text(t_with(
                                    cx,
                                    "transcriptionList.useCommandToRecord",
                                    &[("hotkey", &hotkey)],
                                ))
                                .center(),
                            ),
                        ),
                );
            }
        }

        if !self.initial_loading && has_items {
            for (label, items) in self.groups(cx) {
                let mut cards = div().bg(p.surface).border_1().border_color(p.border);
                for (index, transcription) in items.iter().enumerate() {
                    cards = cards.child(self.render_card(index, transcription, window, cx));
                }

                list = list.child(
                    div()
                        .child(
                            div()
                                .mb_2()
                                .px_1()
                                .type_xs()
                                .text_color(p.ac)
                                .child(text(upper(&label)).tracking_wider()),
                        )
                        .child(cards),
                );
            }

            if self.has_more && !search_pending && self.error.is_none() {
                // Requests the next page once the end of the list is near the
                // visible area.
                let page = cx.entity().downgrade();
                list = list.child(
                    canvas(
                        move |bounds, window, cx| {
                            let viewport_bottom = window.viewport_size().height;
                            if bounds.top() < viewport_bottom + px(LOAD_MORE_MARGIN) {
                                let page = page.clone();
                                window.defer(cx, move |_, cx| {
                                    page.update(cx, |page, cx| page.load_more(cx)).ok();
                                });
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .h_4(),
                );
            }

            if self.loading && !search_pending {
                list = list.child(div().flex().items_center().justify_center().py_4().child(
                    spinning("more-loader", icon("loader").size_5().text_color(p.ac)),
                ));
            }

            if !self.has_more && !search_pending {
                let no_more_key = if showing_search {
                    "home.searchNoMore"
                } else {
                    "transcriptionList.noMore"
                };
                list = list.child(
                    div()
                        .py_4()
                        .type_xs()
                        .text_color(if showing_search {
                            p.txt_hint
                        } else {
                            p.txt_muted
                        })
                        .child(text(t(cx, no_more_key)).center()),
                );
            }
        }

        list
    }
}

impl Focusable for HomePage {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for HomePage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let hotkey = hotkey_display_name(&SettingsStore::get(cx).hotkey);

        let result_count = (self.showing_search_results()
            && !self.initial_loading
            && self.error.is_none())
        .then_some(self.total_count)
        .flatten()
        .filter(|&count| count > 0)
        .map(|count| {
            if count == 1 {
                t(cx, "home.searchResultCountOne")
            } else {
                t_with(
                    cx,
                    "home.searchResultCount",
                    &[("count", &count.to_string())],
                )
            }
        });

        div()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape"
                    && !this.search_field.read(cx).value(cx).is_empty()
                {
                    this.clear_search(false, window, cx);
                }
            }))
            .min_h(window.viewport_size().height)
            .px_6()
            .py_8()
            .flex()
            .flex_col()
            .items_center()
            .font_mono()
            .child(
                div()
                    .w_full()
                    .max_w(px(672.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .mb_6()
                            .type_sm()
                            .child(div().text_color(p.ac).child(text("[HOME]")))
                            .child(div().text_color(p.txt_muted).child(text(">")))
                            .child(div().text_color(p.txt_primary).child(
                                text(t_upper(cx, "home.yourTranscriptions")).tracking_wider(),
                            ))
                            .child(div().flex_1().h_px().bg(p.border)),
                    )
                    .child(
                        div()
                            .mb_5()
                            .type_xs()
                            .text_color(p.txt_muted)
                            .child(text(t_with(
                                cx,
                                "home.pressToRecord",
                                &[("hotkey", &hotkey)],
                            ))),
                    )
                    .child(
                        div()
                            .mb_6()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(self.render_search_bar(window, cx))
                            .children(result_count.map(|label| {
                                div()
                                    .px_1()
                                    .type_xs()
                                    .text_color(p.txt_hint)
                                    .child(text(upper(&label)).tracking_wider())
                            })),
                    )
                    .child(self.render_list(window, cx)),
            )
    }
}

struct Preview {
    text: String,
    truncated: bool,
}

fn normalize_newlines(text: &str) -> String {
    if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text.to_string()
    }
}

/// Cuts `text` after `max_length` grapheme clusters or `max_lines` lines,
/// whichever comes first, so long or tall dictations stay compact until
/// expanded.
fn preview_text(text: &str, max_length: usize, max_lines: usize) -> Preview {
    let normalized = normalize_newlines(text);
    let mut graphemes = 0usize;
    let mut newlines = 0usize;
    let mut cut_at = None;

    for (index, grapheme) in UnicodeSegmentation::grapheme_indices(normalized.as_str(), true) {
        if graphemes == max_length {
            cut_at = Some(index);
            break;
        }
        if grapheme == "\n" {
            newlines += 1;
            if max_lines > 0 && newlines == max_lines {
                let rest = &normalized[index + grapheme.len()..];
                if !rest.is_empty() {
                    cut_at = Some(index);
                    break;
                }
            }
        }
        graphemes += 1;
    }

    match cut_at {
        Some(index) => {
            let prefix = normalized[..index].trim_end_matches('\n');
            Preview {
                text: format!("{prefix}..."),
                truncated: true,
            }
        }
        None => Preview {
            text: normalized,
            truncated: false,
        },
    }
}

/// Finds non-overlapping byte ranges in `haystack` whose folded form matches
/// `fold_search_text(needle)`, using the exact same Unicode case folding as
/// the search backends and remaining safe when character byte length changes
/// under folding (such as `İ` or `ß`).
fn find_match_ranges(haystack: &str, needle: &str) -> Vec<(usize, usize)> {
    let needle_folded = fold_search_text(needle);
    if needle_folded.is_empty() {
        return Vec::new();
    }

    let mut folded = String::with_capacity(haystack.len());
    let mut byte_map: Vec<(usize, usize)> = Vec::with_capacity(haystack.len());
    for (orig_start, ch) in haystack.char_indices() {
        let orig_end = orig_start + ch.len_utf8();
        let start_len = folded.len();
        fold_search_char(ch, &mut folded);
        for _ in start_len..folded.len() {
            byte_map.push((orig_start, orig_end));
        }
    }

    let mut ranges = Vec::new();
    let mut search_from = 0;
    while search_from < folded.len() {
        let Some(rel) = folded[search_from..].find(&needle_folded) else {
            break;
        };
        let match_start = search_from + rel;
        let match_end = match_start + needle_folded.len();
        let orig_start = byte_map[match_start].0;
        let orig_end = byte_map[match_end - 1].1;
        if ranges
            .last()
            .is_none_or(|&(_, prev_end)| orig_start >= prev_end)
        {
            ranges.push((orig_start, orig_end));
        }
        search_from = match_end;
    }

    ranges
}

/// Returns a preview of `full_text` capped at `max_units` UTF-16 code units,
/// sliding the window onto the first match when it appears past the opening
/// preview and reserving space for the match before adding leading context.
fn snippet_for_query(full_text: &str, query: &str, max_units: usize) -> String {
    if query.is_empty() || full_text.encode_utf16().count() <= max_units {
        return truncate_text(full_text, max_units);
    }

    let ranges = find_match_ranges(full_text, query);
    let Some(&(match_start, match_end)) = ranges.first() else {
        return truncate_text(full_text, max_units);
    };

    if full_text[..match_end].encode_utf16().count() <= max_units {
        return truncate_text(full_text, max_units);
    }

    let match_units = full_text[match_start..match_end].encode_utf16().count();
    let leading_budget = max_units
        .saturating_sub(match_units)
        .min(SNIPPET_LEADING_UNITS);

    let mut slice_start = match_start;
    let mut leading_units = 0usize;
    for (idx, ch) in full_text[..match_start].char_indices().rev() {
        let len = ch.len_utf16();
        if leading_units + len > leading_budget {
            break;
        }
        leading_units += len;
        slice_start = idx;
    }

    if slice_start > 0
        && !full_text[..slice_start]
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace)
        && let Some(ws_rel) = full_text[slice_start..match_start].find(char::is_whitespace)
    {
        let after_ws = slice_start + ws_rel;
        if let Some(non_ws_rel) =
            full_text[after_ws..match_start].find(|c: char| !c.is_whitespace())
        {
            slice_start = after_ws + non_ws_rel;
        }
    }

    let mut slice_end = slice_start;
    let mut units = 0usize;
    for (rel, ch) in full_text[slice_start..].char_indices() {
        let len = ch.len_utf16();
        if units + len > max_units {
            break;
        }
        units += len;
        slice_end = slice_start + rel + ch.len_utf8();
    }

    let prefix = if slice_start > 0 { "..." } else { "" };
    let suffix = if slice_end < full_text.len() {
        "..."
    } else {
        ""
    };
    format!("{prefix}{}{suffix}", &full_text[slice_start..slice_end])
}

/// Renders `snippet` with every occurrence of `query` highlighted in
/// `highlight` and bold weight.
fn highlighted_text(snippet: &str, query: &str, highlight: Hsla) -> Text {
    let ranges = find_match_ranges(snippet, query);
    if ranges.is_empty() {
        return text(snippet.to_string());
    }

    let mut cursor = 0usize;
    let mut built: Option<Text> = None;

    let mut push_segment = |segment: &str, color: Option<Hsla>| {
        if segment.is_empty() {
            return;
        }
        built = Some(match (built.take(), color) {
            (None, Some(color)) => text(segment.to_string())
                .color(color)
                .weight(FontWeight::BOLD),
            (None, None) => text(segment.to_string()),
            (Some(existing), Some(color)) => {
                existing.span_weight(segment.to_string(), color, FontWeight::BOLD)
            }
            (Some(existing), None) => existing.plain(segment.to_string()),
        });
    };

    for (start, end) in ranges {
        if start > cursor {
            push_segment(&snippet[cursor..start], None);
        }
        push_segment(&snippet[start..end], Some(highlight));
        cursor = end;
    }
    if cursor < snippet.len() {
        push_segment(&snippet[cursor..], None);
    }

    built.unwrap_or_else(|| text(snippet.to_string()))
}

/// The first `max_length` UTF-16 units of `text`, as the original measured.
fn truncate_text(text: &str, max_length: usize) -> String {
    let units: Vec<u16> = text.encode_utf16().collect();
    if units.len() <= max_length {
        return text.to_string();
    }

    format!("{}...", String::from_utf16_lossy(&units[..max_length]))
}

fn format_processing_time(milliseconds: i64) -> String {
    if milliseconds < 1000 {
        return format!("{milliseconds}ms");
    }

    // One decimal, halves rounded up, as `toFixed(1)` rounds them.
    let tenths = (milliseconds + 50) / 100;
    format!("{}.{}s", tenths / 10, tenths % 10)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn processing_time_switches_to_seconds_at_one_second() {
        assert_eq!(format_processing_time(640), "640ms");
        assert_eq!(format_processing_time(999), "999ms");
        assert_eq!(format_processing_time(1000), "1.0s");
        assert_eq!(format_processing_time(1250), "1.3s");
        assert_eq!(format_processing_time(1840), "1.8s");
        assert_eq!(format_processing_time(12_949), "12.9s");
    }

    #[test]
    fn long_text_is_cut_at_150_units() {
        let long = "a".repeat(200);
        let truncated = preview_text(&long, PREVIEW_MAX_CHARS, PREVIEW_MAX_LINES);

        assert!(truncated.truncated);
        assert_eq!(truncated.text.len(), 153);
        assert!(truncated.text.ends_with("..."));
        let short = preview_text("short", PREVIEW_MAX_CHARS, PREVIEW_MAX_LINES);
        assert!(!short.truncated);
        assert_eq!(short.text, "short");
    }
}
