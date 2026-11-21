//! The Privacy section: whether anonymous usage events are sent, whether
//! transcription history is saved, and deleting the history of transcriptions.

use gpui_kit::{
    App, AsyncApp, ClickEvent, Div, Entity, InteractiveElement as _, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, prelude::*,
};

use crate::analytics;
use crate::backend::{self, AppEvent};
use crate::events;
use crate::settings::{self, SettingsStore};
use crate::ui::motion::Transitions as _;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::toggle::toggle_option;
use crate::ui::widgets::{icon, spinning};
use crate::ui::{t, t_upper};

/// Where clearing the history is, for as long as the section is shown.
#[derive(Debug, Clone, Default, PartialEq)]
enum ClearHistory {
    #[default]
    Idle,
    /// Asked to clear; waiting for the user to confirm.
    Confirming,
    Clearing,
    Cleared,
    Failed(SharedString),
}

pub(super) fn render(window: &mut Window, cx: &mut App) -> Div {
    let p = palette(cx);

    div()
        .flex()
        .flex_col()
        .gap_6()
        .child(toggle_option(
            "analytics",
            &t(cx, "settings.analytics"),
            t(cx, "settings.analyticsDescription"),
            SettingsStore::get(cx).analytics_enabled,
            |enabled, _, cx| {
                SettingsStore::update(cx, |settings| settings.analytics_enabled = enabled);
            },
            cx,
        ))
        .child(
            div()
                .type_xs()
                .text_color(p.txt_hint)
                .child(text(t(cx, "settings.analyticsNote"))),
        )
        .child(history_section(window, cx))
}

/// Controlling whether future dictations are saved, and clearing existing
/// saved transcriptions and recordings as a separate action.
fn history_section(window: &mut Window, cx: &mut App) -> Div {
    let p = palette(cx);
    let save_history = SettingsStore::get(cx).save_transcription_history;
    let save_failed = SettingsStore::save_history_save_failed(cx).then(|| {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .type_xs()
            .child(div().flex_1().text_color(p.ac).child(text(format!(
                "[ERROR] {}",
                t(cx, "settings.saveHistorySaveFailed")
            ))))
            .child(
                div()
                    .id("retry-save-history-setting")
                    .transition_colors()
                    .flex_shrink_0()
                    .px_2p5()
                    .py_1()
                    .border_1()
                    .border_color(p.ac)
                    .text_color(p.ac)
                    .hover(|button| button.bg(p.ac).text_color(p.ac_on))
                    .on_click(move |_, _, cx| {
                        settings::set_save_transcription_history(cx, save_history);
                    })
                    .child(text(t_upper(cx, "voiceControl.retry")).tracking_wider()),
            )
    });

    div()
        .pt_6()
        .border_t_1()
        .border_color(p.border)
        .flex()
        .flex_col()
        .gap_6()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(toggle_option(
                    "save-transcription-history",
                    &t(cx, "settings.saveHistory"),
                    t(cx, "settings.saveHistoryDescription"),
                    save_history,
                    |enabled, _, cx| {
                        if settings::set_save_transcription_history(cx, enabled) {
                            analytics::capture(
                                cx,
                                "settings_save_transcription_history_changed",
                                &[("enabled", enabled.into())],
                            );
                        }
                    },
                    cx,
                ))
                .children(save_failed),
        )
        .child(clear_history(window, cx))
}

/// The history's description, the button that clears it, and the question
/// that button asks before anything is deleted.
fn clear_history(window: &mut Window, cx: &mut App) -> Div {
    let p = palette(cx);
    let state = window.use_keyed_state("clear-history", cx, |_, _| ClearHistory::default());
    let current = state.read(cx).clone();
    let asking = matches!(current, ClearHistory::Confirming | ClearHistory::Clearing);

    let set = |state: &Entity<ClearHistory>, next: ClearHistory| {
        let state = state.clone();
        move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
            state.update(cx, |state, cx| {
                *state = next.clone();
                cx.notify();
            });
        }
    };

    let heading = div()
        .flex()
        .items_center()
        .justify_between()
        .gap_4()
        .type_xs()
        .child(
            div()
                .flex_1()
                .child(
                    div()
                        .mb_1()
                        .text_color(p.txt_secondary)
                        .child(text(t_upper(cx, "settings.history")).tracking_wider()),
                )
                .child(
                    div()
                        .text_color(p.txt_hint)
                        .child(text(t(cx, "settings.historyDescription"))),
                ),
        )
        .when(!asking, |heading| {
            heading.child(
                div()
                    .id("clear-history")
                    .transition_colors()
                    .flex_shrink_0()
                    .px_3()
                    .py_1p5()
                    .border_1()
                    .border_color(p.border_strong)
                    .text_color(p.txt_secondary)
                    .hover(|button| button.border_color(p.ac).text_color(p.ac))
                    .on_click(set(&state, ClearHistory::Confirming))
                    .child(text(t_upper(cx, "settings.clearHistory")).tracking_wider()),
            )
        });

    let confirmation = asking.then(|| {
        let clearing = current == ClearHistory::Clearing;

        div()
            .p_4()
            .border_1()
            .border_color(p.ac)
            .bg(p.ac_bg)
            .type_xs()
            .child(
                div()
                    .mb_4()
                    .leading_relaxed(12.)
                    .text_color(p.txt_primary)
                    .child(text(t(cx, "settings.clearHistoryConfirm"))),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        div()
                            .id("cancel-clear-history")
                            .transition_colors()
                            .px_3()
                            .py_1p5()
                            .border_1()
                            .border_color(p.border_strong)
                            .text_color(p.txt_secondary)
                            .when(!clearing, |button| {
                                button
                                    .hover(|button| {
                                        button.border_color(p.txt_muted).text_color(p.txt_primary)
                                    })
                                    .on_click(set(&state, ClearHistory::Idle))
                            })
                            .when(clearing, |button| button.opacity(0.5))
                            .child(text(t_upper(cx, "settings.cancel")).tracking_wider()),
                    )
                    .child(
                        div()
                            .id("confirm-clear-history")
                            .transition_colors()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .px_3()
                            .py_1p5()
                            .border_1()
                            .border_color(p.ac)
                            .bg(p.ac)
                            .text_color(p.ac_on)
                            .map(|button| {
                                if clearing {
                                    button.opacity(0.5).child(spinning(
                                        "clearing-spinner",
                                        icon("loader").size_3p5(),
                                    ))
                                } else {
                                    let state = state.clone();
                                    button
                                        .hover(|button| {
                                            button.bg(p.ac_hover).border_color(p.ac_hover)
                                        })
                                        .on_click(move |_, _, cx| clear(&state, cx))
                                }
                            })
                            .child(
                                text(t_upper(cx, "settings.clearHistoryConfirmButton"))
                                    .tracking_wider(),
                            ),
                    ),
            )
    });

    let outcome = match current {
        ClearHistory::Cleared => Some(
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .type_xs()
                .text_color(p.success)
                .child(icon("check").size_3p5())
                .child(text(t_upper(cx, "settings.historyCleared")).tracking_wider()),
        ),
        ClearHistory::Failed(error) => Some(
            div()
                .type_xs()
                .text_color(p.ac)
                .child(text(format!("[ERROR] {error}"))),
        ),
        _ => None,
    };

    div()
        .flex()
        .flex_col()
        .gap_4()
        .child(heading)
        .children(confirmation)
        .children(outcome)
}

/// Deletes every transcription and recording, then tells the pages.
fn clear(state: &Entity<ClearHistory>, cx: &mut App) {
    state.update(cx, |state, cx| {
        *state = ClearHistory::Clearing;
        cx.notify();
    });

    let request = backend::call(cx, |backend| backend.clear_transcriptions());
    let state = state.downgrade();

    cx.spawn(async move |cx: &mut AsyncApp| {
        let result = request.await;

        cx.update(|cx| {
            let next = match result {
                Ok(()) => {
                    analytics::capture(cx, "transcription_history_cleared", &[]);
                    events::emit(cx, AppEvent::TranscriptionsCleared);
                    ClearHistory::Cleared
                }
                Err(error) => {
                    log::error!(target: "settings", "clear_transcriptions_failed error={error}");
                    ClearHistory::Failed(error.into())
                }
            };

            state
                .update(cx, |state, cx| {
                    *state = next;
                    cx.notify();
                })
                .ok();
        });
    })
    .detach();
}
