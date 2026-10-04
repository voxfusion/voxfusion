//! The Home page: the history of transcriptions, grouped by day.

use gpui_kit::{
    ClipboardItem, Context, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
    Subscription, Window, canvas, div, prelude::*, px,
};
use std::collections::HashSet;
use std::time::Duration;

use crate::analytics;
use crate::backend::{self, AppEvent, Transcription, command_error};
use crate::events;
use crate::settings::SettingsStore;
use crate::ui::datetime::{self, format_long_date, format_time, same_day};
use crate::ui::grid::above_grid;
use crate::ui::hotkeys::hotkey_display_name;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::{icon, spinning};
use crate::ui::{locale, t, t_upper, t_with, upper};

const PAGE_SIZE: i64 = 20;
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
    _subscriptions: Vec<Subscription>,
}

impl HomePage {
    pub fn new(cx: &mut Context<Self>) -> Self {
        analytics::capture(cx, "$pageview", &[("$current_url", "/".into())]);

        let subscriptions = vec![
            cx.subscribe(&events::hub(cx), |this, _, event, cx| {
                if matches!(event, AppEvent::TranscriptionCreated) {
                    analytics::capture(cx, "transcription_created", &[]);
                    this.fetch(None, cx);
                }
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
            _subscriptions: subscriptions,
        };
        page.fetch(None, cx);
        page
    }

    fn fetch(&mut self, cursor: Option<String>, cx: &mut Context<Self>) {
        if self.loading {
            return;
        }

        self.loading = true;
        self.error = None;
        cx.notify();

        let request = backend::call(cx, {
            let cursor = cursor.clone();
            move |backend| backend.list_transcriptions(PAGE_SIZE, cursor)
        });

        cx.spawn(async move |this, cx| {
            let result = request.await;

            this.update(cx, |this, cx| {
                this.loading = false;
                this.initial_loading = false;

                match result {
                    Ok(page) => {
                        this.next_cursor = page
                            .transcriptions
                            .last()
                            .map(|item| item.created_at.clone());
                        this.has_more = page.has_more;

                        if cursor.is_some() {
                            this.transcriptions.extend(page.transcriptions);
                        } else {
                            this.transcriptions = page.transcriptions;
                        }
                    }
                    Err(error) => {
                        this.error = Some(command_error("list_transcriptions", &error).into());
                    }
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
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let p = palette(cx);
        let id = transcription.id.clone();
        let hovered = self.hovered.as_deref() == Some(id.as_str());
        let copied = self.copied.contains(&id);
        let time = datetime::parse(&transcription.created_at)
            .map(|time| format_time(&time, locale(cx)))
            .unwrap_or_default();

        div()
            .id(SharedString::from(format!("transcription-{id}")))
            .relative()
            .p_3()
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
            .child(
                div()
                    .mb_2()
                    .type_sm()
                    .leading_relaxed(14.)
                    .text_color(p.txt_primary)
                    .child(text(truncate_text(&transcription.text, 150))),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .type_xs()
                    .text_color(p.txt_muted)
                    .child(text(time))
                    .child(text(format_processing_time(
                        transcription.processing_time_ms,
                    ))),
            )
            .when(hovered || copied, |card| {
                let transcription = transcription.clone();

                card.child(
                    div()
                        .id("copy")
                        .absolute()
                        .top_2()
                        .right_2()
                        .p_1p5()
                        .bg(p.base)
                        .border_1()
                        .border_color(p.border_strong)
                        .hover(|button| button.border_color(p.ac))
                        .text_color(p.txt_secondary)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.copy(&transcription, cx);
                        }))
                        .child(if copied {
                            icon("check").size_3p5().text_color(p.success)
                        } else {
                            icon("copy").size_3p5()
                        }),
                )
            })
    }

    fn render_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
                    cards = cards.child(self.render_card(index, transcription, cx));
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
                    .child(self.render_list(cx)),
            )
    }
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
        let truncated = truncate_text(&long, 150);

        assert_eq!(truncated.len(), 153);
        assert!(truncated.ends_with("..."));
        assert_eq!(truncate_text("short", 150), "short");
    }
}
