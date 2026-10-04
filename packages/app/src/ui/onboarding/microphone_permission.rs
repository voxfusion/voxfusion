//! Step 1: asking for access to the microphone.

use gpui_kit::{
    Context, EventEmitter, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Task, Window, div, prelude::*,
};
use std::time::Duration;

use super::parts::{
    PermissionChanged, accent_button, button_label, card, checking_row, description, icon_box,
    side_note, status_row, step_column, title,
};
use crate::backend::{self, PermissionState};
use crate::ui::t;
use crate::ui::theme::palette;
use crate::ui::widgets::icon;

const MICROPHONE_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone";
const PERMISSION_POLL_INTERVAL: Duration = Duration::from_millis(1000);

pub struct MicrophonePermissionStep {
    /// `None` until the permission has been looked up.
    granted: Option<bool>,
    requesting: bool,
    denied: bool,
    polling: Option<Task<()>>,
}

impl EventEmitter<PermissionChanged> for MicrophonePermissionStep {}

impl MicrophonePermissionStep {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut step = Self {
            granted: None,
            requesting: false,
            denied: false,
            polling: None,
        };

        // Keep polling so the step advances by itself once access is granted
        // in System Settings.
        let first_check = step.check_permission(cx);
        cx.spawn(async move |this, cx| {
            first_check.await;

            this.update(cx, |this, cx| {
                if this.granted != Some(true) {
                    this.start_polling(cx);
                }
            })
            .ok();
        })
        .detach();

        step
    }

    fn start_polling(&mut self, cx: &mut Context<Self>) {
        if self.polling.is_some() {
            return;
        }

        self.polling = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(PERMISSION_POLL_INTERVAL)
                    .await;

                let polled = this.update(cx, |this, cx| this.check_permission(cx).detach());
                if polled.is_err() {
                    break;
                }
            }
        }));
    }

    fn set_granted(&mut self, granted: bool, cx: &mut Context<Self>) {
        if granted {
            self.denied = false;
            self.polling = None;
        }

        self.granted = Some(granted);
        cx.emit(PermissionChanged(granted));
        cx.notify();
    }

    fn check_permission(&mut self, cx: &mut Context<Self>) -> Task<()> {
        let state = backend::call(cx, |backend| backend.microphone_permission());

        cx.spawn(async move |this, cx| {
            let state = state.await;

            this.update(cx, |this, cx| {
                if state == PermissionState::Denied {
                    this.denied = true;
                }
                this.set_granted(state == PermissionState::Granted, cx);
            })
            .ok();
        })
    }

    fn request(&mut self, cx: &mut Context<Self>) {
        self.requesting = true;
        cx.notify();

        let answer = backend::call(cx, |backend| backend.request_microphone_permission());
        cx.spawn(async move |this, cx| {
            let allowed = answer.await;

            this.update(cx, |this, cx| {
                if !allowed {
                    // A refusal is permanent on macOS: asking again can never
                    // succeed, the user has to flip the switch in System
                    // Settings. The next poll finds the permission denied
                    // and brings up the instructions for that.
                    this.start_polling(cx);
                }

                this.requesting = false;
                this.set_granted(allowed, cx);
            })
            .ok();
        })
        .detach();
    }

    fn open_system_settings(&mut self, cx: &mut Context<Self>) {
        cx.open_url(MICROPHONE_SETTINGS_URL);
        self.start_polling(cx);
    }

    fn render_status(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let p = palette(cx);

        let row = match self.granted {
            None => checking_row(t(cx, "onboarding.checkingPermission"), cx),
            Some(true) => status_row(
                icon("check").size_5(),
                t(cx, "onboarding.micPermissionGranted"),
                p.success,
            ),
            Some(false) => status_row(
                icon("alert-circle").size_5(),
                t(cx, "onboarding.micPermissionNotGranted"),
                p.ac,
            ),
        };

        div()
            .mb_6()
            .child(row)
            .when(self.granted == Some(false) && self.denied, |status| {
                status.child(side_note(t(cx, "onboarding.micPermissionDenied"), cx).mt_4())
            })
    }

    fn render_action(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let p = palette(cx);

        let button = if self.denied {
            accent_button("open-settings", true, p.surface, cx)
                .gap_2()
                .px_6()
                .on_click(cx.listener(|this, _, _, cx| this.open_system_settings(cx)))
                .child(button_label(&t(cx, "onboarding.openSystemPreferences")))
                .child(icon("external-link").size_4())
        } else {
            let enabled = !self.requesting && self.granted.is_some();
            let label = if self.requesting {
                t(cx, "onboarding.checkingPermission")
            } else {
                t(cx, "onboarding.grantMicPermission")
            };

            accent_button("grant", enabled, p.surface, cx)
                .px_6()
                .when(enabled, |button| {
                    button.on_click(cx.listener(|this, _, _, cx| this.request(cx)))
                })
                .child(button_label(&label))
        };

        div().flex().justify_center().child(button)
    }
}

impl Render for MicrophonePermissionStep {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);

        let content = div()
            .child(icon_box(icon("mic").text_color(p.ac), cx))
            .child(title(&t(cx, "onboarding.micPermissionTitle"), cx))
            .child(description(t(cx, "onboarding.micPermissionDescription"), cx).mb_8())
            .child(self.render_status(cx))
            .when(self.granted != Some(true), |content| {
                content.child(self.render_action(cx))
            });

        step_column("[STEP_01] > MIC_PERMISSION", p.ac).child(card(content, cx))
    }
}
