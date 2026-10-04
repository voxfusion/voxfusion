//! The Hotkeys section: the two dictation hotkeys, each with its recorder.

use gpui_kit::{
    App, Context, Div, Entity, InteractiveElement as _, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, div, prelude::*,
};

use super::SettingsModal;
use crate::settings::SettingsStore;
use crate::ui::hotkey_recorder::HotkeyRecorder;
use crate::ui::hotkeys::hotkey_display_name;
use crate::ui::t;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;

impl SettingsModal {
    pub(super) fn render_hotkeys(&self, cx: &mut Context<Self>) -> Div {
        let settings = SettingsStore::get(cx);

        div()
            .flex()
            .flex_col()
            .gap_6()
            .child(render_hotkey(
                "HANDS_FREE_TRIGGER",
                &settings.hotkey,
                t(cx, "settings.hotkeyDescription"),
                &self.hands_free_recorder,
                &self.hold_to_speak_recorder,
                cx,
            ))
            .child(render_hotkey(
                "HOLD_TO_SPEAK_TRIGGER",
                &settings.hold_to_speak_hotkey,
                t(cx, "settings.holdToSpeakHotkeyDescription"),
                &self.hold_to_speak_recorder,
                &self.hands_free_recorder,
                cx,
            ))
    }
}

/// One hotkey: what it is, or what has been pressed while `recorder` records
/// a new one. The button is off while the `other` hotkey is being recorded
/// or has a problem to show.
fn render_hotkey(
    label: &'static str,
    hotkey: &str,
    description: SharedString,
    recorder: &Entity<HotkeyRecorder>,
    other: &Entity<HotkeyRecorder>,
    cx: &App,
) -> Div {
    let p = palette(cx);
    let state = recorder.read(cx);
    let recording = state.is_recording();
    let error = state.error();

    let other = other.read(cx);
    let disabled = other.error().is_some() || (other.is_recording() && !recording);

    let shown = if !recording {
        hotkey_display_name(hotkey)
    } else if state.pending_hotkey().is_empty() {
        "_ WAITING FOR INPUT _".to_string()
    } else {
        state.pending_hotkey().to_string()
    };

    let (background, hover_background, label_color) = if recording {
        (p.border, p.border_strong, p.txt_secondary)
    } else {
        (p.ac, p.ac_hover, p.ac_on)
    };
    // Half transparent as a whole: the label fades towards what is behind
    // the button, not towards the button's own color.
    let (background, hover_background, label_color) = if disabled {
        (
            background.opacity(0.5),
            hover_background.opacity(0.5),
            p.base.blend(label_color.opacity(0.5)),
        )
    } else {
        (background, hover_background, label_color)
    };

    let recorder = recorder.clone();
    let button = div()
        .id(label)
        .px_4()
        .py_3()
        .type_xs()
        .bg(background)
        .hover(|button| button.bg(hover_background))
        .text_color(label_color)
        .when(!disabled, |button| {
            button.on_click(move |_, _, cx| {
                recorder.update(cx, |recorder, cx| recorder.toggle_recording(cx));
            })
        })
        .child(text(if recording { "[CANCEL]" } else { "[CHANGE]" }).tracking_wider());

    div()
        .child(
            div()
                .mb_3()
                .type_xs()
                .text_color(p.txt_muted)
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
                        .px_4()
                        .py_3()
                        .border_1()
                        .type_sm()
                        .map(|display| {
                            if recording {
                                display.border_color(p.ac).bg(p.ac_bg).text_color(p.ac)
                            } else {
                                display
                                    .border_color(p.border_strong)
                                    .bg(p.surface)
                                    .text_color(p.txt_primary)
                            }
                        })
                        .child(text(shown).center()),
                )
                .child(button),
        )
        .children(error.map(|error| div().type_xs().text_color(p.error).child(text(error))))
        .child(
            div()
                .mt_3()
                .type_xs()
                .text_color(p.txt_faint)
                .child(text(description)),
        )
}
