//! The first-run wizard: permissions, the microphone, hotkeys, privacy, the
//! choice and download of a model, and a try-out, one step at a time.

mod accessibility_permission;
mod completion;
mod hotkey;
mod learning;
mod microphone;
mod microphone_permission;
mod model_download;
mod parts;
mod privacy;
mod step_indicator;
mod transition;

use gpui_kit::{
    AnimationExt as _, App, AppContext as _, Context, Div, Entity, FocusHandle,
    InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, div, prelude::*, px,
    relative,
};
use std::time::Duration;

use self::accessibility_permission::AccessibilityPermissionStep;
use self::completion::completion_step;
use self::hotkey::HotkeyStep;
use self::learning::{LearningStep, TranscriptionTried};
use self::microphone::MicrophoneStep;
use self::microphone_permission::MicrophonePermissionStep;
use self::model_download::{ModelDownloadStep, ModelReady};
use self::parts::{PermissionChanged, accent_button, button_label};
use self::privacy::privacy_step;
use self::step_indicator::{format_step, step_indicator};
use self::transition::Tween;
use crate::actions::{NextControl, PreviousControl};
use crate::analytics;
use crate::settings::{self, MODEL_DOWNLOAD_STEP, ONBOARDING_STEP_COUNT, SettingsStore};
use crate::ui::motion::{Faded, Transitions as _};
use crate::ui::t;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::focus::{focus_ring, focus_visible};

/// How long a step takes to fade out, and the next one to fade in.
const STEP_FADE: Duration = Duration::from_millis(200);
/// How far a step slides while it fades.
const SLIDE_DISTANCE: f32 = 32.;
/// How long the layout takes to widen for the try-out step and to narrow back.
const WIDTH_TRANSITION: Duration = Duration::from_millis(300);

const MICROPHONE_PERMISSION_STEP: u32 = 1;
const ACCESSIBILITY_PERMISSION_STEP: u32 = 2;
const MICROPHONE_STEP: u32 = 3;
const HOTKEY_STEP: u32 = 4;
const LEARNING_STEP: u32 = 7;

/// The width the step content and the navigation may take.
fn layout_width(step: u32) -> f32 {
    if step == LEARNING_STEP { 896. } else { 672. }
}

#[derive(Clone, Copy)]
enum Direction {
    Forward,
    Backward,
}

enum StepView {
    MicrophonePermission(Entity<MicrophonePermissionStep>),
    AccessibilityPermission(Entity<AccessibilityPermissionStep>),
    Microphone(Entity<MicrophoneStep>),
    ModelDownload(Entity<ModelDownloadStep>),
    Hotkey(Entity<HotkeyStep>),
    Privacy,
    Learning(Entity<LearningStep>),
    Completion,
}

pub struct OnboardingWizard {
    current_step: u32,
    step: StepView,
    /// Set while the outgoing step fades away; navigation waits for it.
    animating: bool,
    opacity: Tween,
    offset: Tween,
    max_width: Tween,
    mic_permission_granted: bool,
    accessibility_permission_granted: bool,
    model_ready: bool,
    learning_completed: bool,
    /// Holds focus during step transitions so outgoing controls do not stay focused.
    focus_handle: FocusHandle,
    back_focus: FocusHandle,
    skip_focus: FocusHandle,
    forward_focus: FocusHandle,
    step_events: Option<Subscription>,
}

impl OnboardingWizard {
    pub fn new(initial_step: u32, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut wizard = Self {
            current_step: initial_step,
            step: StepView::Completion,
            animating: false,
            opacity: Tween::resting(1., STEP_FADE),
            offset: Tween::resting(0., STEP_FADE),
            max_width: Tween::resting(layout_width(initial_step), WIDTH_TRANSITION),
            mic_permission_granted: false,
            accessibility_permission_granted: false,
            model_ready: false,
            learning_completed: false,
            focus_handle: cx.focus_handle(),
            back_focus: cx.focus_handle().tab_stop(true),
            skip_focus: cx.focus_handle().tab_stop(true),
            forward_focus: cx.focus_handle().tab_stop(true),
            step_events: None,
        };
        window.focus(&wizard.focus_handle, cx);
        wizard.mount_step(window, cx);
        wizard
    }

    /// Moves the keyboard focus to the next control in the wizard, or the
    /// previous one.
    fn move_focus(&self, forward: bool, window: &mut Window, cx: &mut App) {
        let mut first_outside = None;
        loop {
            if forward {
                window.focus_next(cx);
            } else {
                window.focus_prev(cx);
            }

            let Some(focused) = window.focused(cx) else {
                return;
            };
            if self.focus_handle.contains_focused(window, cx) {
                return;
            }
            match &first_outside {
                Some(first) if *first == focused => return,
                Some(_) => {}
                None => first_outside = Some(focused),
            }
        }
    }

    /// Replaces the step shown with the one for the current step number: a
    /// fresh one, except the model step, which keeps its download going.
    fn mount_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.step_events = None;

        self.step = match self.current_step {
            MICROPHONE_PERMISSION_STEP => {
                let step = cx.new(MicrophonePermissionStep::new);
                self.step_events = Some(cx.subscribe(
                    &step,
                    |this, _, event: &PermissionChanged, cx| {
                        this.mic_permission_granted = event.0;
                        cx.notify();
                    },
                ));
                StepView::MicrophonePermission(step)
            }
            ACCESSIBILITY_PERMISSION_STEP => {
                let step = cx.new(AccessibilityPermissionStep::new);
                self.step_events = Some(cx.subscribe(
                    &step,
                    |this, _, event: &PermissionChanged, cx| {
                        this.accessibility_permission_granted = event.0;
                        cx.notify();
                    },
                ));
                StepView::AccessibilityPermission(step)
            }
            MICROPHONE_STEP => StepView::Microphone(cx.new(MicrophoneStep::new)),
            HOTKEY_STEP => StepView::Hotkey(cx.new(HotkeyStep::new)),
            MODEL_DOWNLOAD_STEP => {
                let step = ModelDownloadStep::shared(cx);
                self.model_ready = step.read(cx).is_ready();
                self.step_events = Some(cx.subscribe(&step, |this, _, event: &ModelReady, cx| {
                    this.model_ready = event.0;
                    cx.notify();
                }));
                StepView::ModelDownload(step)
            }
            LEARNING_STEP => {
                let step = cx.new(|cx| LearningStep::new(window, cx));
                self.step_events =
                    Some(cx.subscribe(&step, |this, _, _: &TranscriptionTried, cx| {
                        this.learning_completed = true;
                        cx.notify();
                    }));
                StepView::Learning(step)
            }
            ONBOARDING_STEP_COUNT => StepView::Completion,
            _ => StepView::Privacy,
        };
    }

    fn can_proceed(&self) -> bool {
        match self.current_step {
            MICROPHONE_PERMISSION_STEP => self.mic_permission_granted,
            ACCESSIBILITY_PERMISSION_STEP => self.accessibility_permission_granted,
            MODEL_DOWNLOAD_STEP => self.model_ready,
            LEARNING_STEP => self.learning_completed,
            _ => true,
        }
    }

    fn go_to_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.animating || self.current_step >= ONBOARDING_STEP_COUNT {
            return;
        }

        analytics::capture(
            cx,
            "onboarding_step_completed",
            &[("step", self.current_step.into())],
        );
        self.change_step(Direction::Forward, window, cx);
    }

    fn skip_learning(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.animating || self.current_step != LEARNING_STEP {
            return;
        }

        analytics::capture(
            cx,
            "onboarding_step_skipped",
            &[("step", self.current_step.into())],
        );
        self.change_step(Direction::Forward, window, cx);
    }

    fn go_to_previous(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.animating || self.current_step <= 1 {
            return;
        }

        self.change_step(Direction::Backward, window, cx);
    }

    /// Fades the current step out towards `direction`, then brings the
    /// neighbouring step in from the same side.
    fn change_step(&mut self, direction: Direction, window: &mut Window, cx: &mut Context<Self>) {
        self.animating = true;
        // Stop trial dictation hotkeys and ignore late practice events as soon
        // as the user leaves step 7, before the fade-out finishes dropping the
        // step entity.
        if let StepView::Learning(step) = &self.step {
            self.step_events = None;
            step.update(cx, |step, cx| step.deactivate(cx));
        }
        // Take focus off outgoing controls so typing or repeated Enter during
        // the transition does not act on them.
        window.focus(&self.focus_handle, cx);
        self.opacity.retarget(0.);
        self.offset.retarget(match direction {
            Direction::Forward => SLIDE_DISTANCE,
            Direction::Backward => -SLIDE_DISTANCE,
        });
        cx.notify();

        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(STEP_FADE).await;

            this.update_in(cx, |this, window, cx| {
                this.current_step = match direction {
                    Direction::Forward => this.current_step + 1,
                    Direction::Backward => this.current_step - 1,
                };
                window.focus(&this.focus_handle, cx);
                this.mount_step(window, cx);

                this.animating = false;
                this.opacity.retarget(1.);
                this.offset.retarget(0.);
                this.max_width.retarget(layout_width(this.current_step));

                let step = this.current_step;
                SettingsStore::update(cx, |settings| settings.onboarding_step = step);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn complete(&mut self, cx: &mut Context<Self>) {
        if self.animating {
            return;
        }
        analytics::capture(cx, "onboarding_completed", &[]);
        ModelDownloadStep::release(cx);
        settings::mark_onboarding_complete(cx);
    }

    fn render_secondary_button(
        &self,
        id: &'static str,
        label: &str,
        focus_handle: &FocusHandle,
        on_click: fn(&mut Self, &mut Window, &mut Context<Self>),
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Faded {
        let p = palette(cx);

        div()
            .id(id)
            .transition_colors()
            .track_focus(focus_handle)
            .relative()
            .px_6()
            .py_3()
            .type_sm()
            .border_1()
            .border_color(p.border_strong)
            .text_color(p.txt_secondary)
            .hover(|button| button.text_color(p.txt_primary).border_color(p.ac))
            .on_click(cx.listener(move |this, _, window, cx| on_click(this, window, cx)))
            .child(button_label(label))
            .when(focus_visible(focus_handle, window), |button| {
                button.child(focus_ring(4., cx))
            })
    }

    fn render_navigation(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let p = palette(cx);
        let show_skip = self.current_step == LEARNING_STEP && !self.learning_completed;

        let back = (self.current_step > 1).then(|| {
            self.render_secondary_button(
                "back",
                &t(cx, "onboarding.back"),
                &self.back_focus,
                Self::go_to_previous,
                window,
                cx,
            )
        });

        let skip = show_skip.then(|| {
            self.render_secondary_button(
                "skip-learning",
                &t(cx, "onboarding.skipForNow"),
                &self.skip_focus,
                Self::skip_learning,
                window,
                cx,
            )
        });

        // None of the navigation buttons call `above_grid`, so the grid runs over them.
        let forward = if self.current_step < ONBOARDING_STEP_COUNT {
            let enabled = self.can_proceed();

            accent_button("next", enabled, p.base, cx)
                .when(enabled, |button| {
                    button
                        .track_focus(&self.forward_focus)
                        .on_click(cx.listener(|this, _, window, cx| this.go_to_next(window, cx)))
                })
                .relative()
                .px_8()
                .child(button_label(&t(cx, "onboarding.next")))
                .when(
                    enabled && focus_visible(&self.forward_focus, window),
                    |button| button.child(focus_ring(4., cx)),
                )
        } else {
            accent_button("get-started", true, p.base, cx)
                .track_focus(&self.forward_focus)
                .relative()
                .px_8()
                .on_click(cx.listener(|this, _, _, cx| this.complete(cx)))
                .child(button_label(&t(cx, "onboarding.getStarted")))
                .when(focus_visible(&self.forward_focus, window), |button| {
                    button.child(focus_ring(4., cx))
                })
        };

        let actions = div()
            .flex()
            .items_center()
            .gap_3()
            .children(skip)
            .child(forward);

        div()
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .mt_8()
            .child(match back {
                Some(back) => back.into_any_element(),
                None => div().into_any_element(),
            })
            .child(actions)
    }
}

impl Render for OnboardingWizard {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let (opacity, offset, max_width) = (self.opacity, self.offset, self.max_width);

        let step = match &self.step {
            StepView::MicrophonePermission(step) => step.clone().into_any_element(),
            StepView::AccessibilityPermission(step) => step.clone().into_any_element(),
            StepView::Microphone(step) => step.clone().into_any_element(),
            StepView::ModelDownload(step) => step.clone().into_any_element(),
            StepView::Hotkey(step) => step.clone().into_any_element(),
            StepView::Privacy => privacy_step(cx).into_any_element(),
            StepView::Learning(step) => step.clone().into_any_element(),
            StepView::Completion => completion_step(cx).into_any_element(),
        };

        let content = div()
            .relative()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .child(step)
            .with_animation(
                opacity.animation_id("step-fade"),
                opacity.animation(),
                move |content, progress| {
                    content
                        .opacity(opacity.frame(progress))
                        .left(px(offset.frame(progress)))
                },
            );

        // A step taller than the window scrolls between the step indicator
        // and the navigation, which stay where they are.
        let content_area = div()
            .id("step-area")
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_y_scroll()
            .child(
                div()
                    .min_h(relative(1.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(content),
            )
            .with_animation(
                max_width.animation_id("content-width"),
                max_width.animation(),
                move |area, progress| area.max_w(px(max_width.frame(progress))),
            );

        let navigation = self.render_navigation(window, cx).with_animation(
            max_width.animation_id("navigation-width"),
            max_width.animation(),
            move |navigation, progress| navigation.max_w(px(max_width.frame(progress))),
        );

        div()
            .relative()
            .key_context("Onboarding")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &NextControl, window, cx| {
                this.move_focus(true, window, cx);
            }))
            .on_action(cx.listener(|this, _: &PreviousControl, window, cx| {
                this.move_focus(false, window, cx);
            }))
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .p_8()
            .bg(p.base)
            .font_mono()
            .child(
                div()
                    .absolute()
                    .top_8()
                    .right_8()
                    .type_sm()
                    .text_color(p.txt_muted)
                    .child(
                        text(format!(
                            "{}/{}",
                            format_step(self.current_step),
                            format_step(ONBOARDING_STEP_COUNT)
                        ))
                        .tracking_wider(),
                    ),
            )
            .child(div().mb_12().child(step_indicator(self.current_step, cx)))
            .child(content_area)
            .child(navigation)
    }
}
