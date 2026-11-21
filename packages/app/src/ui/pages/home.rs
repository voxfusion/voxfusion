//! The Home page: the history of transcriptions, grouped by day.

use gpui_kit::{
    ClickEvent, ClipboardItem, Context, IntoElement, MouseButton, ParentElement as _, Render,
    SharedString, Styled as _, Subscription, Window, canvas, div, prelude::*, px,
};
use std::collections::HashSet;
use std::time::Duration;
use unicode_segmentation::UnicodeSegmentation;

use crate::analytics;
use crate::backend::{self, AppEvent, Transcription, command_error};
use crate::events;
use crate::settings::SettingsStore;
use crate::ui::datetime::{self, format_long_date, format_time, same_day};
use crate::ui::grid::above_grid;
use crate::ui::hotkeys::hotkey_display_name;
use crate::ui::motion::Transitions as _;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::focus::{focus_ring, focus_visible, keyed_focus_handle};
use crate::ui::widgets::{icon, spinning, turning};
use crate::ui::{locale, t, t_upper, t_with, upper};

const PAGE_SIZE: i64 = 20;
const PREVIEW_MAX_CHARS: usize = 150;
const PREVIEW_MAX_LINES: usize = 3;
/// How far below the visible area the end of the list may be before the next
/// page is requested.
const LOAD_MORE_MARGIN: f32 = 100.;

pub struct HomePage {
    transcriptions: Vec<Transcription>,
    loading: bool,
    initial_loading: bool,
    next_cursor: Option<String>,
    has_more: bool,
    error: Option<SharedString>,
    hovered: Option<String>,
    retry_hovered: bool,
    copied: HashSet<String>,
    expanded: HashSet<String>,
    /// Transcriptions deleted here. A list fetched before the deletion
    /// reached the database leaves them out.
    deleted: HashSet<String>,
    /// Counts the times the history was cleared; a list fetched before the
    /// last time is dropped.
    generation: u64,
    /// A reload asked for while a fetch was running, done after it.
    reload_pending: bool,
    _subscriptions: Vec<Subscription>,
}

impl HomePage {
    pub fn new(cx: &mut Context<Self>) -> Self {
        analytics::capture(cx, "$pageview", &[("$current_url", "/".into())]);

        let subscriptions = vec![
            cx.subscribe(&events::hub(cx), |this, _, event, cx| match event {
                AppEvent::TranscriptionCreated => {
                    analytics::capture(cx, "transcription_created", &[]);
                    this.fetch(None, cx);
                }
                AppEvent::TranscriptionsCleared => {
                    this.generation += 1;
                    this.transcriptions.clear();
                    this.deleted.clear();
                    this.copied.clear();
                    this.expanded.clear();
                    this.hovered = None;
                    this.next_cursor = None;
                    this.has_more = false;
                    this.fetch(None, cx);
                }
                _ => {}
            }),
            cx.observe(&SettingsStore::entity(cx), |_, _, cx| cx.notify()),
        ];

        let mut page = Self {
            transcriptions: Vec::new(),
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
            generation: 0,
            reload_pending: false,
            _subscriptions: subscriptions,
        };
        page.fetch(None, cx);
        page
    }

    fn fetch(&mut self, cursor: Option<String>, cx: &mut Context<Self>) {
        if self.loading {
            if cursor.is_none() {
                self.reload_pending = true;
            }
            return;
        }

        self.loading = true;
        self.error = None;
        cx.notify();

        let request = backend::call(cx, {
            let cursor = cursor.clone();
            move |backend| backend.list_transcriptions(PAGE_SIZE, cursor)
        });

        let generation = self.generation;

        cx.spawn(async move |this, cx| {
            let result = request.await;

            this.update(cx, |this, cx| {
                this.loading = false;
                if generation != this.generation {
                    this.reload_pending = false;
                    this.fetch(None, cx);
                    return;
                }
                this.initial_loading = false;

                match result {
                    Ok(page) => {
                        this.next_cursor = page
                            .transcriptions
                            .last()
                            .map(|item| item.created_at.clone());
                        this.has_more = page.has_more;

                        let listed = page
                            .transcriptions
                            .into_iter()
                            .filter(|item| !this.deleted.contains(&item.id));
                        if cursor.is_some() {
                            this.transcriptions.extend(listed);
                        } else {
                            this.transcriptions = listed.collect();
                        }
                        if !this.has_more && !this.reload_pending {
                            this.expanded.retain(|id| {
                                this.transcriptions
                                    .iter()
                                    .any(|transcription| transcription.id == *id)
                            });
                        }
                    }
                    Err(error) => {
                        this.error = Some(command_error("list_transcriptions", &error).into());
                    }
                }

                if std::mem::take(&mut this.reload_pending) {
                    this.fetch(None, cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn load_more(&mut self, cx: &mut Context<Self>) {
        if !self.has_more || self.loading {
            return;
        }
        if let Some(cursor) = self.next_cursor.clone() {
            self.fetch(Some(cursor), cx);
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
        self.transcriptions
            .retain(|transcription| transcription.id != id);
        self.copied.remove(id);
        let was_expanded = self.expanded.remove(id);
        self.deleted.insert(id.to_string());
        self.hovered = None;
        cx.notify();

        let id = id.to_string();
        let request = backend::call(cx, {
            let id = id.clone();
            move |backend| backend.delete_transcription(&id)
        });
        let generation = self.generation;

        cx.spawn(async move |this, cx| {
            let result = request.await;

            this.update(cx, |this, cx| {
                // A refused deletion brings the transcription back. With every
                // listed one deleted, the next page is fetched.
                if let Err(error) = &result {
                    log::error!(target: "home", "delete_transcription_failed error={error}");
                    this.deleted.remove(&id);
                    if was_expanded && generation == this.generation {
                        this.expanded.insert(id.clone());
                    }
                    this.fetch(None, cx);
                } else if this.transcriptions.is_empty() && this.has_more {
                    this.fetch(None, cx);
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
        let preview = preview_text(&transcription.text, PREVIEW_MAX_CHARS, PREVIEW_MAX_LINES);
        let expandable = preview.truncated;
        let expanded = expandable && self.expanded.contains(&id);
        let display_text = if expanded {
            normalize_newlines(&transcription.text)
        } else {
            preview.text
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
                    .child(text(display_text).pre_wrap().break_words()),
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
                            .on_click(cx.listener(|this, _, _, cx| this.fetch(None, cx)))
                            .child(text("[RETRY]").underline(self.retry_hovered)),
                    ),
            );
        }

        if !self.initial_loading && self.error.is_none() && !has_items {
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

            if self.loading {
                list = list.child(div().flex().items_center().justify_center().py_4().child(
                    spinning("more-loader", icon("loader").size_5().text_color(p.ac)),
                ));
            }

            if !self.has_more {
                list = list.child(
                    div()
                        .py_4()
                        .type_xs()
                        .text_color(p.txt_muted)
                        .child(text(t(cx, "transcriptionList.noMore")).center()),
                );
            }
        }

        list
    }
}

impl Render for HomePage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let hotkey = hotkey_display_name(&SettingsStore::get(cx).hotkey);

        div()
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
                            .mb_8()
                            .type_xs()
                            .text_color(p.txt_muted)
                            .child(text(t_with(
                                cx,
                                "home.pressToRecord",
                                &[("hotkey", &hotkey)],
                            ))),
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
