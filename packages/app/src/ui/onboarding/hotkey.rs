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
use crate::ui::motion::Transitions as _;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::focus::{focus_ring, focus_visible, keyed_focus_handle};
use crate::ui::widgets::icon;
use crate::ui::{t, t_upper, upper};

pub struct HotkeyStep {
    hands_free: Entity<HotkeyRecorder>,
    hold_to_speak: Entity<HotkeyRecorder>,
    /// Holds the keyboard focus while a hotkey is recorded, so the keys
    /// pressed reach the recorders.
    focus_handle: FocusHandle,
    /// Window-bound recorder observers are attached on the first render.
    observing_recorders: bool,
    _subscriptions: Vec<Subscription>,
}

impl HotkeyStep {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let hands_free = cx.new(|cx| HotkeyRecorder::new(HotkeyKind::HandsFree, cx));
        let hold_to_speak = cx.new(|cx| HotkeyRecorder::new(HotkeyKind::HoldToSpeak, cx));

        let subscriptions = vec![cx.observe(&SettingsStore::entity(cx), |_, _, cx| cx.notify())];

        Self {
            hands_free,
            hold_to_speak,
            focus_handle: cx.focus_handle(),
            observing_recorders: false,
            _subscriptions: subscriptions,
        }
    }

    /// While a hotkey is recorded every key belongs to it: the focus leaves
    /// whatever control had it (even if Tab moved it while the watcher was
    /// starting), so keys reach `route_keys` and Enter or Space cannot press
    /// a button.
    fn recorder_changed(
        &mut self,
        recorder: Entity<HotkeyRecorder>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if recorder.read(cx).is_recording() {
            window.focus(&self.focus_handle, cx);
        }
        cx.notify();
    }

    fn ensure_recorder_observers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.observing_recorders {
            return;
        }
        self.observing_recorders = true;
        self._subscriptions.push(cx.observe_in(
            &self.hands_free,
            window,
            Self::recorder_changed,
        ));
        self._subscriptions.push(cx.observe_in(
            &self.hold_to_speak,
            window,
            Self::recorder_changed,
        ));
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
        hotkey: Option<&str>,
        record_label: SharedString,
        allow_disable: bool,
        recorder: &Entity<HotkeyRecorder>,
        other: &Entity<HotkeyRecorder>,
        first: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let p = palette(cx);
        let state = recorder.read(cx);
        let recording = state.is_recording();
        let error = state.error();
        let (shown, is_unset): (SharedString, bool) = if recording {
            let pending = if state.pending_hotkey().is_empty() {
                t(cx, "onboarding.pressKeys")
            } else {
                state.pending_hotkey().to_string().into()
            };
            (pending, false)
        } else if let Some(hotkey) = hotkey {
            (hotkey_display_name(hotkey).into(), false)
        } else {
            (t(cx, "settings.hotkeyDisabled"), true)
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

        let button_focus = keyed_focus_handle(id, window, cx);
        let disable_action = (allow_disable && hotkey.is_some()).then(|| {
            let disable_focus = keyed_focus_handle("disable-hold-to-speak", window, cx);
            let disable_recorder = recorder.clone();
            let next_focus = button_focus.clone();

            div()
                .id("disable-hold-to-speak")
                .transition_colors()
                .when(!disabled, |button| button.track_focus(&disable_focus))
                .relative()
                .type_xs()
                .text_color(shade(p.txt_secondary))
                .when(!disabled, |button| {
                    button
                        .hover(move |button| button.text_color(p.ac))
                        .on_click(move |_, window, cx| {
                            disable_recorder.update(cx, |recorder, cx| recorder.clear(cx));
                            // Removing this button would drop keyboard focus; move
                            // it to the shortcut button so Tab keeps working.
                            window.focus(&next_focus, cx);
                        })
                })
                .child(
                    text(t_upper(cx, "settings.disableHotkey"))
                        .tracking_wider()
                        .nowrap(),
                )
                .when(!disabled && focus_visible(&disable_focus, window), |button| {
                    button.child(focus_ring(4., cx))
                })
        });

        let button = div()
            .id(id)
            .transition_colors()
            .when(!disabled, |button| button.track_focus(&button_focus))
            .relative()
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
                        .when(!disabled, |button| {
                            button.hover(move |button| {
                                button
                                    .border_color(shade(p.ac))
                                    .text_color(shade(p.txt_primary))
                            })
                        })
                } else {
                    button
                        .bg(shade(p.ac))
                        .text_color(shade(p.ac_on))
                        .when(!disabled, |button| {
                            button.hover(move |button| button.bg(shade(p.ac_hover)))
                        })
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
            }))
            .when(!disabled && focus_visible(&button_focus, window), |button| {
                button.child(focus_ring(4., cx))
            });

        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .when(!first, |label| label.pt_4())
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .type_xs()
                            .text_color(p.txt_muted)
                            .child(text(upper(&name)).tracking_wider()),
                    )
                    .children(disable_action),
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
                        } else if is_unset {
                            display
                                .border_color(p.border)
                                .bg(p.base)
                                .text_color(p.txt_hint)
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_recorder_observers(window, cx);
        let p = palette(cx);
        let settings = SettingsStore::get(cx).clone();
        let recording = self.hands_free.read(cx).is_recording()
            || self.hold_to_speak.read(cx).is_recording();

        let hotkeys = div()
            .flex()
            .flex_col()
            .gap_4()
            .child(self.render_hotkey(
                "record-hands-free",
                t(cx, "onboarding.handsFreeHotkey"),
                Some(&settings.hotkey),
                t(cx, "onboarding.recordHotkey"),
                false,
                &self.hands_free.clone(),
                &self.hold_to_speak.clone(),
                true,
                window,
                cx,
            ))
            .child(self.render_hotkey(
                "record-hold-to-speak",
                t(cx, "onboarding.holdToSpeakHotkey"),
                settings.hold_to_speak_hotkey.as_deref(),
                t(cx, "onboarding.recordHoldToSpeakHotkey"),
                true,
                &self.hold_to_speak.clone(),
                &self.hands_free.clone(),
                false,
                window,
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
                .track_focus(&self.focus_handle)
                .when(recording, |step| step.key_context("HotkeyRecorder")),
            &recorders,
        )
        .child(card(content, cx))
    }
}
