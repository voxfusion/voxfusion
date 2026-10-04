//! The first-run wizard: permissions, the microphone, hotkeys, privacy, the
//! model download and a try-out, one step at a time.

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
    AnimationExt as _, AppContext as _, Context, Div, Entity, IntoElement, ParentElement as _,
    Render, StatefulInteractiveElement as _, Styled as _, Subscription, Window, div, prelude::*,
    px,
};
use std::time::Duration;

use self::accessibility_permission::AccessibilityPermissionStep;
use self::completion::completion_step;
use self::hotkey::HotkeyStep;
use self::learning::{LearningStep, TranscriptionTried};
use self::microphone::MicrophoneStep;
use self::microphone_permission::MicrophonePermissionStep;
use self::model_download::{DownloadComplete, ModelDownloadStep};
use self::parts::{PermissionChanged, accent_button, button_label};
use self::privacy::privacy_step;
use self::step_indicator::{format_step, step_indicator};
use self::transition::Tween;
use crate::analytics;
use crate::settings::{self, MODEL_DOWNLOAD_STEP, ONBOARDING_STEP_COUNT, SettingsStore};
use crate::ui::t;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;

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
    model_downloaded: bool,
    learning_completed: bool,
    step_events: Option<Subscription>,
}

impl OnboardingWizard {
    pub fn new(initial_step: u32, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut wizard = Self {
            current_step: initial_step,
            step: StepView::Completion,
            animating: false,
            opacity: Tween::resting(1., STEP_FADE),
            offset: Tween::resting(0., STEP_FADE),
            max_width: Tween::resting(layout_width(initial_step), WIDTH_TRANSITION),
            mic_permission_granted: false,
            accessibility_permission_granted: false,
            model_downloaded: false,
            learning_completed: false,
            step_events: None,
        };
        wizard.mount_step(cx);
        wizard
    }

    /// Replaces the step shown with a fresh one for the current step number.
    fn mount_step(&mut self, cx: &mut Context<Self>) {
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
                let step = cx.new(ModelDownloadStep::new);
                self.step_events =
                    Some(cx.subscribe(&step, |this, _, _: &DownloadComplete, cx| {
                        this.model_downloaded = true;
                        cx.notify();
                    }));
                StepView::ModelDownload(step)
            }
            LEARNING_STEP => {
                let step = cx.new(LearningStep::new);
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
            MODEL_DOWNLOAD_STEP => self.model_downloaded,
            LEARNING_STEP => self.learning_completed,
            _ => true,
        }
    }

    fn go_to_next(&mut self, cx: &mut Context<Self>) {
        if self.animating || self.current_step >= ONBOARDING_STEP_COUNT {
            return;
        }

        analytics::capture(
            cx,
            "onboarding_step_completed",
            &[("step", self.current_step.into())],
        );
        self.change_step(Direction::Forward, cx);
    }

    fn go_to_previous(&mut self, cx: &mut Context<Self>) {
        if self.animating || self.current_step <= 1 {
            return;
        }

        self.change_step(Direction::Backward, cx);
    }

    /// Fades the current step out towards `direction`, then brings the
    /// neighbouring step in from the same side.
    fn change_step(&mut self, direction: Direction, cx: &mut Context<Self>) {
        self.animating = true;
        self.opacity.retarget(0.);
        self.offset.retarget(match direction {
            Direction::Forward => SLIDE_DISTANCE,
            Direction::Backward => -SLIDE_DISTANCE,
        });
        cx.notify();

        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(STEP_FADE).await;

            this.update(cx, |this, cx| {
                this.current_step = match direction {
                    Direction::Forward => this.current_step + 1,
                    Direction::Backward => this.current_step - 1,
                };
                this.mount_step(cx);

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
        analytics::capture(cx, "onboarding_completed", &[]);
        settings::mark_onboarding_complete(cx);
    }

    fn render_navigation(&self, cx: &mut Context<Self>) -> Div {
        let p = palette(cx);

        let back = (self.current_step > 1).then(|| {
            div()
                .id("back")
                .px_6()
                .py_3()
                .type_sm()
                .border_1()
                .border_color(p.border_strong)
                .text_color(p.txt_secondary)
                .hover(|button| button.text_color(p.txt_primary).border_color(p.ac))
                .on_click(cx.listener(|this, _, _, cx| this.go_to_previous(cx)))
                .child(button_label(&t(cx, "onboarding.back")))
        });

        // Neither button is positioned, so the grid runs over both.
        let forward = if self.current_step < ONBOARDING_STEP_COUNT {
            let enabled = self.can_proceed();

            accent_button("next", enabled, p.base, cx)
                .px_8()
                .when(enabled, |button| {
                    button.on_click(cx.listener(|this, _, _, cx| this.go_to_next(cx)))
                })
                .child(button_label(&t(cx, "onboarding.next")))
        } else {
            accent_button("get-started", true, p.base, cx)
                .px_8()
                .on_click(cx.listener(|this, _, _, cx| this.complete(cx)))
                .child(button_label(&t(cx, "onboarding.getStarted")))
        };

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
            .child(forward)
    }
}

impl Render for OnboardingWizard {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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

        let content_area = div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .w_full()
            .child(content)
            .with_animation(
                max_width.animation_id("content-width"),
                max_width.animation(),
                move |area, progress| area.max_w(px(max_width.frame(progress))),
            );

        let navigation = self.render_navigation(cx).with_animation(
            max_width.animation_id("navigation-width"),
            max_width.animation(),
            move |navigation, progress| navigation.max_w(px(max_width.frame(progress))),
        );

        div()
            .relative()
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
