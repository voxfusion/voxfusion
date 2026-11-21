//! The Hotkeys section: the two dictation hotkeys, each with its recorder.

use gpui_kit::{
    App, Context, Div, Entity, InteractiveElement as _, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, prelude::*,
};

use super::SettingsModal;
use crate::settings::SettingsStore;
use crate::ui::hotkey_recorder::HotkeyRecorder;
use crate::ui::hotkeys::hotkey_display_name;
use crate::ui::motion::Transitions as _;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::focus::{focus_ring, focus_visible, keyed_focus_handle};
use crate::ui::{t, t_upper};

impl SettingsModal {
    pub(super) fn render_hotkeys(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let settings = SettingsStore::get(cx).clone();
        let hold_description = if settings.hold_to_speak_hotkey.is_some() {
            t(cx, "settings.holdToSpeakHotkeyDescription")
        } else {
            t(cx, "settings.holdToSpeakHotkeyDisabledDescription")
        };

        div()
            .flex()
            .flex_col()
            .gap_6()
            .child(render_hotkey(
                "HANDS_FREE_TRIGGER",
                Some(&settings.hotkey),
                t(cx, "settings.hotkeyDescription"),
                false,
                &self.hands_free_recorder,
                &self.hold_to_speak_recorder,
                window,
                cx,
            ))
            .child(render_hotkey(
                "HOLD_TO_SPEAK_TRIGGER",
                settings.hold_to_speak_hotkey.as_deref(),
                hold_description,
                true,
                &self.hold_to_speak_recorder,
                &self.hands_free_recorder,
                window,
                cx,
            ))
    }
}

/// One hotkey: what it is, or what has been pressed while `recorder` records
/// a new one. Its actions are off while the `other` hotkey is being recorded
/// or has a problem to show.
#[allow(clippy::too_many_arguments)]
fn render_hotkey(
    label: &'static str,
    hotkey: Option<&str>,
    description: SharedString,
    allow_disable: bool,
    recorder: &Entity<HotkeyRecorder>,
    other: &Entity<HotkeyRecorder>,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    let p = palette(cx);
    let state = recorder.read(cx);
    let recording = state.is_recording();
    let error = state.error();

    let other = other.read(cx);
    let disabled = other.error().is_some() || (other.is_recording() && !recording);

    let (shown, is_unset): (SharedString, bool) = if recording {
        let pending = if state.pending_hotkey().is_empty() {
            "_ WAITING FOR INPUT _".into()
        } else {
            state.pending_hotkey().to_string().into()
        };
        (pending, false)
    } else if let Some(hotkey) = hotkey {
        (hotkey_display_name(hotkey).into(), false)
    } else {
        (t(cx, "settings.hotkeyDisabled"), true)
    };

    let (background, hover_background, label_color) = if recording {
        (p.border, p.border_strong, p.txt_secondary)
    } else {
        (p.ac, p.ac_hover, p.ac_on)
    };
    // Half strength as a whole over the modal background, so the border and
    // fill share one opaque color and the label fades cleanly into the backdrop.
    let (background, label_color) = if disabled {
        (
            p.base.blend(background.opacity(0.5)),
            p.base.blend(label_color.opacity(0.5)),
        )
    } else {
        (background, label_color)
    };

    let primary_id = SharedString::from(format!("{label}-change"));
    let primary_focus = keyed_focus_handle(primary_id.clone(), window, cx);
    let primary_label = format!(
        "[{}]",
        t_upper(
            cx,
            if recording {
                "settings.cancel"
            } else if hotkey.is_some() {
                "settings.changeHotkey"
            } else {
                "settings.setHotkey"
            },
        )
    );

    let primary_recorder = recorder.clone();
    let primary_button = div()
        .id(primary_id)
        .transition_colors()
        .when(!disabled, |button| button.track_focus(&primary_focus))
        .relative()
        .flex_shrink_0()
        .px_4()
        .py_3()
        .border_1()
        .type_xs()
        .bg(background)
        .border_color(background)
        .text_color(label_color)
        .when(!disabled, |button| {
            button
                .hover(move |button| button.bg(hover_background).border_color(hover_background))
                .on_click(move |_, _, cx| {
                    primary_recorder.update(cx, |recorder, cx| recorder.toggle_recording(cx));
                })
        })
        .child(text(primary_label).tracking_wider().nowrap())
        .when(!disabled && focus_visible(&primary_focus, window), |button| {
            button.child(focus_ring(4., cx))
        });

    let disable_button = (allow_disable && hotkey.is_some()).then(|| {
        let disable_id = SharedString::from(format!("{label}-disable"));
        let disable_focus = keyed_focus_handle(disable_id.clone(), window, cx);
        let (disable_border, disable_text) = if disabled {
            (
                p.base.blend(p.border_strong.opacity(0.5)),
                p.base.blend(p.txt_secondary.opacity(0.5)),
            )
        } else {
            (p.border_strong, p.txt_secondary)
        };
        let disable_recorder = recorder.clone();
        let next_focus = primary_focus.clone();

        div()
            .id(disable_id)
            .transition_colors()
            .when(!disabled, |button| button.track_focus(&disable_focus))
            .relative()
            .flex_shrink_0()
            .px_4()
            .py_3()
            .border_1()
            .type_xs()
            .border_color(disable_border)
            .text_color(disable_text)
            .when(!disabled, |button| {
                button
                    .hover(move |button| button.border_color(p.ac).text_color(p.ac))
                    .on_click(move |_, window, cx| {
                        disable_recorder.update(cx, |recorder, cx| recorder.clear(cx));
                        // Removing this button would drop keyboard focus; move
                        // it to [SET] so Tab and Esc keep working.
                        window.focus(&next_focus, cx);
                    })
            })
            .child(
                text(format!("[{}]", t_upper(cx, "settings.disableHotkey")))
                    .tracking_wider()
                    .nowrap(),
            )
            .when(!disabled && focus_visible(&disable_focus, window), |button| {
                button.child(focus_ring(4., cx))
            })
    });

    div()
        .child(
            div()
                .mb_3()
                .type_xs()
                .text_color(p.txt_secondary)
                .child(text(label).tracking_wider()),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .px_4()
                        .py_3()
                        .border_1()
                        .type_sm()
                        .map(|display| {
                            if recording {
                                display.border_color(p.ac).bg(p.ac_bg).text_color(p.ac)
                            } else if is_unset {
                                display
                                    .border_color(p.border)
                                    .bg(p.surface)
                                    .text_color(p.txt_hint)
                            } else {
                                display
                                    .border_color(p.border_strong)
                                    .bg(p.surface)
                                    .text_color(p.txt_primary)
                            }
                        })
                        .child(text(shown).center()),
                )
                .child(primary_button)
                .children(disable_button),
        )
        .children(error.map(|error| div().mt_2().type_xs().text_color(p.error).child(text(error))))
        .child(
            div()
                .mt_3()
                .type_xs()
                .text_color(p.txt_hint)
                .child(text(description)),
        )
}
