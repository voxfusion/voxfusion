//! Step 4: the two dictation hotkeys, each with its recorder.

use gpui_kit::{
    AppContext as _, Context, Entity, FocusHandle, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, Subscription, Window, div, prelude::*,
};

use super::parts::{button_label, card, description, faded, icon_box, step_column, title};
use crate::settings::SettingsStore;
use crate::ui::hotkey_recorder::{HotkeyKind, HotkeyRecorder, route_keys};
use crate::ui::hotkeys::hotkey_display_name;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::icon;
use crate::ui::{t, upper};

pub struct HotkeyStep {
    hands_free: Entity<HotkeyRecorder>,
    hold_to_speak: Entity<HotkeyRecorder>,
    /// Holds the keyboard focus while a hotkey is recorded, so the keys
    /// pressed reach the recorders.
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl HotkeyStep {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let hands_free = cx.new(|cx| HotkeyRecorder::new(HotkeyKind::HandsFree, cx));
        let hold_to_speak = cx.new(|cx| HotkeyRecorder::new(HotkeyKind::HoldToSpeak, cx));

        let subscriptions = vec![
            cx.observe(&SettingsStore::entity(cx), |_, _, cx| cx.notify()),
            cx.observe(&hands_free, |_, _, cx| cx.notify()),
            cx.observe(&hold_to_speak, |_, _, cx| cx.notify()),
        ];

        Self {
            hands_free,
            hold_to_speak,
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    /// One hotkey: its name, what it is (or what has been pressed while
    /// `recorder` records a new one), and the button that starts and cancels
    /// recording. The button is off while the `other` hotkey is being
    /// recorded or has a problem to show.
    #[allow(clippy::too_many_arguments)]
    fn render_hotkey(
        &self,
        id: &'static str,
        name: SharedString,
        hotkey: &str,
        record_label: SharedString,
        recorder: &Entity<HotkeyRecorder>,
        other: &Entity<HotkeyRecorder>,
        first: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let p = palette(cx);
        let state = recorder.read(cx);
        let recording = state.is_recording();
        let error = state.error();
        let shown: SharedString = if !recording {
            hotkey_display_name(hotkey).into()
        } else if state.pending_hotkey().is_empty() {
            t(cx, "onboarding.pressKeys")
        } else {
            state.pending_hotkey().to_string().into()
        };

        let other = other.read(cx);
        let disabled = other.error().is_some() || (other.is_recording() && !recording);
        // A disabled button is the whole button at half strength over the card.
        let shade = move |color| {
            if disabled {
                faded(color, p.surface, 0.5)
            } else {
                color
            }
        };

        let button = div()
            .id(id)
            .px_6()
            .py_3()
            .type_sm()
            .font_weight(FontWeight::BOLD)
            .map(|button| {
                if recording {
                    button
                        .border_1()
                        .border_color(shade(p.border_strong))
                        .text_color(shade(p.txt_secondary))
                        .hover(move |button| {
                            button
                                .border_color(shade(p.ac))
                                .text_color(shade(p.txt_primary))
                        })
                } else {
                    button
                        .bg(shade(p.ac))
                        .text_color(shade(p.ac_on))
                        .hover(move |button| button.bg(shade(p.ac_hover)))
                }
            })
            .when(!disabled, |button| {
                let recorder = recorder.clone();
                button.on_click(move |_, _, cx| {
                    recorder.update(cx, |recorder, cx| recorder.toggle_recording(cx));
                })
            })
            .child(button_label(&if recording {
                t(cx, "settings.cancel")
            } else {
                record_label
            }));

        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .when(!first, |label| label.pt_4())
                    .type_xs()
                    .text_color(p.txt_muted)
                    .child(text(upper(&name)).tracking_wider()),
            )
            .child(
                div()
                    .px_6()
                    .py_4()
                    .border_1()
                    .type_lg()
                    .map(|display| {
                        if recording {
                            display.border_color(p.ac).bg(p.ac_bg).text_color(p.ac)
                        } else {
                            display
                                .border_color(p.border_strong)
                                .bg(p.base)
                                .text_color(p.txt_primary)
                        }
                    })
                    .child(text(shown).center()),
            )
            .child(div().flex().justify_center().child(button))
            .children(error.map(|error| {
                div()
                    .type_xs()
                    .text_color(p.error)
                    .child(text(error).center())
            }))
    }
}

impl Render for HotkeyStep {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let settings = SettingsStore::get(cx).clone();

        let hotkeys = div()
            .flex()
            .flex_col()
            .gap_4()
            .child(self.render_hotkey(
                "record-hands-free",
                t(cx, "onboarding.handsFreeHotkey"),
                &settings.hotkey,
                t(cx, "onboarding.recordHotkey"),
                &self.hands_free.clone(),
                &self.hold_to_speak.clone(),
                true,
                cx,
            ))
            .child(self.render_hotkey(
                "record-hold-to-speak",
                t(cx, "onboarding.holdToSpeakHotkey"),
                &settings.hold_to_speak_hotkey,
                t(cx, "onboarding.recordHoldToSpeakHotkey"),
                &self.hold_to_speak.clone(),
                &self.hands_free.clone(),
                false,
                cx,
            ));

        let content = div()
            .child(icon_box(icon("keyboard").text_color(p.ac), cx))
            .child(title(&t(cx, "onboarding.hotkeyTitle"), cx))
            .child(description(t(cx, "onboarding.hotkeyDescription"), cx).mb_8())
            .child(hotkeys);

        let recorders = [self.hands_free.clone(), self.hold_to_speak.clone()];

        // Clicking a record button gives the step the focus, and with it the
        // keys that follow.
        route_keys(
            step_column("[STEP_04] > HOTKEY_CONFIG", p.ac)
                .id("hotkey-step")
                .track_focus(&self.focus_handle),
            &recorders,
        )
        .child(card(content, cx))
    }
}
