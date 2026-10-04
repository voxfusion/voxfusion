//! Step 7: a place to try dictation before finishing. The instructions sit
//! beside a chat-like panel whose input receives the transcription.

use gpui_kit::{
    AppContext as _, Context, Div, Entity, EventEmitter, Focusable as _, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, div, prelude::*, px,
    relative,
};
use std::time::Duration;

use super::parts::faded;
use crate::backend::AppEvent;
use crate::events;
use crate::settings::SettingsStore;
use crate::ui::grid::above_grid;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::icon;
use crate::ui::widgets::text_field::{TextField, TextFieldEvent, field_box};
use crate::ui::{t, upper};

/// How long after a transcription its text is given to arrive in the input.
const TYPING_SETTLE: Duration = Duration::from_millis(100);

/// Raised when a message was sent, which is what lets the wizard move on.
pub struct TranscriptionTried;

pub struct LearningStep {
    messages: Vec<SharedString>,
    /// Created with the first render, which is when there is a window.
    input: Option<Entity<TextField>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<TranscriptionTried> for LearningStep {}

/// The keys of `hotkey` as the labels of key caps.
fn hotkey_parts(hotkey: &str) -> Vec<SharedString> {
    hotkey
        .split('+')
        .map(|part| {
            match part {
                "Command" => "\u{2318}",
                "Control" => "\u{2303}",
                "Alt" => "\u{2325}",
                "Shift" => "\u{21E7}",
                "LeftControl" => "Left \u{2303}",
                "RightControl" => "Right \u{2303}",
                "LeftOption" => "Left \u{2325}",
                "RightOption" => "Right \u{2325}",
                "LeftCommand" => "Left \u{2318}",
                "RightCommand" => "Right \u{2318}",
                other => other,
            }
            .to_string()
            .into()
        })
        .collect()
}

impl LearningStep {
    pub fn new(cx: &mut Context<Self>) -> Self {
        // While this step is shown the dictation hotkeys work, though
        // onboarding is not finished.
        events::emit(cx, AppEvent::LearningStepActive(true));
        cx.on_release(|_, cx| events::emit(cx, AppEvent::LearningStepActive(false)))
            .detach();

        let subscriptions = vec![
            cx.subscribe(&events::hub(cx), |_, _, event, cx| {
                if matches!(event, AppEvent::TranscriptionCreated) {
                    // The text is typed into the input like any other app's;
                    // give the last characters time to arrive.
                    cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(TYPING_SETTLE).await;

                        let Some(window) = cx.update(|cx| cx.windows().first().copied()) else {
                            return;
                        };
                        let _ = window.update(cx, |_, window, cx| {
                            this.update(cx, |this, cx| this.transcription_arrived(window, cx))
                                .ok();
                        });
                    })
                    .detach();
                }
            }),
            cx.observe(&SettingsStore::entity(cx), |_, _, cx| cx.notify()),
        ];

        Self {
            messages: Vec::new(),
            input: None,
            _subscriptions: subscriptions,
        }
    }

    fn input(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<TextField> {
        if let Some(input) = &self.input {
            return input.clone();
        }

        let input = cx.new(|cx| {
            TextField::new(window, cx).placeholder(|cx| t(cx, "onboarding.learningPlaceholder"))
        });
        input.update(cx, |input, cx| input.focus(window, cx));

        self._subscriptions
            .push(cx.subscribe(&input, |_, _, event: &TextFieldEvent, cx| {
                if matches!(
                    event,
                    TextFieldEvent::Change | TextFieldEvent::Focus | TextFieldEvent::Blur
                ) {
                    cx.notify();
                }
            }));

        self.input = Some(input.clone());
        input
    }

    /// Moves what is in the input into the conversation.
    fn add_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(input) = self.input.clone() else {
            return;
        };

        let message = input.read(cx).value(cx).trim().to_string();
        if message.is_empty() {
            return;
        }

        self.messages.push(message.into());
        input.update(cx, |input, cx| input.set_value("", window, cx));
        cx.emit(TranscriptionTried);
        cx.notify();
    }

    /// A dictation ended: what it typed into the input becomes a message,
    /// and the input is ready for the next one.
    fn transcription_arrived(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.add_message(window, cx);

        if let Some(input) = self.input.clone() {
            input.update(cx, |input, cx| input.focus(window, cx));
        }
    }

    fn render_row(number: &str, content: impl IntoElement, cx: &Context<Self>) -> Div {
        let p = palette(cx);

        div()
            .flex()
            .items_start()
            .gap_3()
            .type_sm()
            .child(
                div()
                    .mt_0p5()
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.ac)
                    .child(text(number.to_string())),
            )
            .child(content)
    }

    /// An instruction with key caps in its middle.
    fn render_keys_row(
        number: &str,
        prefix: SharedString,
        keys: &[SharedString],
        suffix: SharedString,
        cx: &Context<Self>,
    ) -> Div {
        let p = palette(cx);
        let words = |label: SharedString| div().text_color(p.txt_secondary).child(text(label));

        let key_caps = keys.iter().map(|key| {
            div()
                .min_w(px(32.))
                .px_2()
                .py_1()
                .flex()
                .items_center()
                .justify_center()
                .border_1()
                .border_color(p.border_strong)
                .rounded(px(4.))
                .bg(p.base)
                .text_color(p.txt_primary)
                .child(text(key.clone()).nowrap())
        });

        Self::render_row(
            number,
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(words(prefix))
                .children(key_caps)
                .child(words(suffix)),
            cx,
        )
    }

    fn render_text_row(number: &str, label: SharedString, cx: &Context<Self>) -> Div {
        let p = palette(cx);

        Self::render_row(
            number,
            div().text_color(p.txt_secondary).child(text(label)),
            cx,
        )
    }
}

impl Render for LearningStep {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let input = self.input(window, cx);
        let settings = SettingsStore::get(cx);
        let toggle_keys = hotkey_parts(&settings.hotkey);
        let hold_keys = hotkey_parts(&settings.hold_to_speak_hotkey);

        let manual = div()
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .p_6()
            .flex()
            .flex_col()
            .gap_6()
            .child(Self::render_keys_row(
                "1.",
                t(cx, "onboarding.learningStep1Prefix"),
                &toggle_keys,
                t(cx, "onboarding.learningStep1Suffix"),
                cx,
            ))
            .child(Self::render_text_row(
                "2.",
                t(cx, "onboarding.learningStep2"),
                cx,
            ))
            .child(Self::render_keys_row(
                "3.",
                t(cx, "onboarding.learningStep3Prefix"),
                &toggle_keys,
                t(cx, "onboarding.learningStep3Suffix"),
                cx,
            ))
            .child(Self::render_text_row(
                "4.",
                t(cx, "onboarding.learningStep4"),
                cx,
            ))
            .child(Self::render_keys_row(
                "5.",
                t(cx, "onboarding.learningHoldToSpeakPrefix"),
                &hold_keys,
                t(cx, "onboarding.learningHoldToSpeakSuffix"),
                cx,
            ));

        let messages = if self.messages.is_empty() {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .type_sm()
                .text_color(p.txt_muted)
                .child(text(t(cx, "onboarding.learningPlaceholder")))
        } else {
            div()
                .flex()
                .flex_col()
                .gap_3()
                .children(self.messages.iter().map(|message| {
                    div().flex().justify_end().child(
                        div()
                            .max_w(relative(0.8))
                            .px_4()
                            .py_2()
                            .rounded(px(16.))
                            .rounded_br(px(2.))
                            .bg(p.ac)
                            .type_sm()
                            .text_color(p.ac_on)
                            .child(text(message.clone())),
                    )
                }))
        };

        let has_text = !input.read(cx).value(cx).is_empty();
        let focused = input.read(cx).focus_handle(cx).is_focused(window);
        // A disabled button is the whole button at 30% over the panel.
        let shade = move |color| {
            if has_text {
                color
            } else {
                faded(color, p.surface, 0.3)
            }
        };

        let send = div()
            .id("send")
            .size_9()
            .flex_shrink_0()
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(shade(p.ac))
            .text_color(shade(p.ac_on))
            .hover(move |button| button.bg(shade(p.ac_hover)))
            .when(has_text, |button| {
                button.on_click(cx.listener(|this, _, window, cx| {
                    this.add_message(window, cx);
                    // Pressing a button takes the focus from the input, as
                    // it does in the original.
                    window.blur(cx);
                }))
            })
            .child(icon("send").size_4());

        let chat = div()
            .h(px(340.))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .flex()
            .flex_col()
            .child(
                div()
                    .id("messages")
                    .flex_1()
                    .min_h_0()
                    .p_4()
                    .overflow_y_scroll()
                    .child(messages),
            )
            .child(
                div()
                    .border_t_1()
                    .border_color(p.border)
                    .p_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        field_box(&input, cx)
                            .flex_1()
                            .min_w_0()
                            .px_4()
                            .py_2()
                            .rounded_full()
                            .bg(p.base)
                            .border_1()
                            .border_color(if focused { p.ac } else { p.border })
                            .type_sm()
                            .text_color(p.txt_primary),
                    )
                    .child(send),
            );

        // Each panel sits above the grid, as everything in a step does. The
        // wrapper spans the panel's border, which an inner layer would not.
        //
        // The two share the row's free space equally, but each starts from
        // its own padding and border (`flex: 1 1 0%` cannot go below them),
        // so the padded instructions end up the wider of the two.
        let above = |panel: Div, padding_and_border: f32, cx: &mut Context<Self>| {
            div()
                .relative()
                .flex_grow(1.)
                .flex_shrink(1.)
                .flex_basis(px(padding_and_border))
                .min_w_0()
                .child(above_grid(cx))
                .child(panel)
        };

        div()
            .w_full()
            .max_w(px(896.))
            .child(
                div()
                    .mb_8()
                    .type_sm()
                    .text_color(p.ac)
                    .child(text("[STEP_07] > TRY_IT_OUT").tracking_wider().center()),
            )
            .child(
                div().mb_2().type_xl().text_color(p.txt_primary).child(
                    text(upper(&t(cx, "onboarding.learningTitle")))
                        .tracking_wider()
                        .center(),
                ),
            )
            .child(
                div()
                    .mb_8()
                    .type_sm()
                    .text_color(p.txt_secondary)
                    .child(text(t(cx, "onboarding.learningDescription")).center()),
            )
            .child(
                div()
                    .flex()
                    .gap_6()
                    // The instructions stretch to the height of the chat panel.
                    .child(above(manual.h_full(), 50., cx))
                    .child(above(chat, 2., cx)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkeys_split_into_key_caps() {
        assert_eq!(
            hotkey_parts("LeftControl+LeftOption"),
            ["Left \u{2303}", "Left \u{2325}"]
        );
        assert_eq!(
            hotkey_parts("Command+Shift+K"),
            ["\u{2318}", "\u{21E7}", "K"]
        );
        assert_eq!(hotkey_parts("RightCommand"), ["Right \u{2318}"]);
        assert_eq!(hotkey_parts("Alt+Space"), ["\u{2325}", "Space"]);
    }
}
