//! Step 2: asking for the Accessibility permission, which typing into other
//! apps needs.

use gpui_kit::{
    Context, EventEmitter, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, Window, div, prelude::*,
};
use std::time::Duration;

use super::parts::{
    PermissionChanged, accent_button, button_label, card, checking_row, description, icon_box,
    side_note, status_row, step_column, title,
};
use crate::backend::{self, AppEvent};
use crate::events;
use crate::ui::t;
use crate::ui::theme::palette;
use crate::ui::widgets::icon;

const PERMISSION_POLL_INTERVAL: Duration = Duration::from_millis(1000);

/// How long to wait before each further probe once the system has announced
/// a change: its permission database takes a moment to settle.
const RETRY_DELAYS: [Duration; 3] = [
    Duration::from_millis(200),
    Duration::from_millis(500),
    Duration::from_millis(1000),
];

pub struct AccessibilityPermissionStep {
    /// `None` until the permission has been looked up.
    granted: Option<bool>,
    requesting: bool,
    polling: Option<Task<()>>,
    change_notifications: Option<Subscription>,
}

impl EventEmitter<PermissionChanged> for AccessibilityPermissionStep {}

impl AccessibilityPermissionStep {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut step = Self {
            granted: None,
            requesting: false,
            polling: None,
            change_notifications: None,
        };

        let first_check = step.check_permission(cx);
        cx.spawn(async move |this, cx| {
            first_check.await;

            this.update(cx, |this, cx| {
                if this.granted == Some(true) {
                    return;
                }

                // Polling is the fallback for the notification macOS posts
                // when the user toggles the switch in System Settings.
                this.start_polling(cx);
                this.change_notifications =
                    Some(cx.subscribe(&events::hub(cx), |this, _, event, cx| {
                        if matches!(event, AppEvent::AccessibilityChanged) {
                            this.check_with_retries(cx);
                        }
                    }));
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
            self.polling = None;
        }

        self.granted = Some(granted);
        cx.emit(PermissionChanged(granted));
        cx.notify();
    }

    fn check_permission(&mut self, cx: &mut Context<Self>) -> Task<()> {
        let granted = backend::call(cx, |backend| backend.check_accessibility());

        cx.spawn(async move |this, cx| {
            let granted = granted.await;
            this.update(cx, |this, cx| this.set_granted(granted, cx))
                .ok();
        })
    }

    /// Probes right away and then after each of [`RETRY_DELAYS`], until the
    /// permission shows as granted.
    fn check_with_retries(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let delays = std::iter::once(Duration::ZERO).chain(RETRY_DELAYS);

            for delay in delays {
                if !delay.is_zero() {
                    cx.background_executor().timer(delay).await;
                }

                let Ok(check) = this.update(cx, |this, cx| this.check_permission(cx)) else {
                    return;
                };
                check.await;

                match this.read_with(cx, |this, _| this.granted) {
                    Ok(Some(true)) | Err(_) => return,
                    Ok(_) => {}
                }
            }

            // The probe can lag behind the Settings notification. The
            // notification itself is reliable: it fires only when the user
            // toggles the switch. Trust it.
            this.update(cx, |this, cx| this.set_granted(true, cx)).ok();
        })
        .detach();
    }

    fn open_settings(&mut self, cx: &mut Context<Self>) {
        self.requesting = true;
        cx.notify();

        let request = backend::call(cx, |backend| backend.request_accessibility());
        cx.spawn(async move |this, cx| {
            request.await;

            this.update(cx, |this, cx| {
                this.start_polling(cx);
                this.requesting = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn render_status(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let p = palette(cx);

        match self.granted {
            None => checking_row(t(cx, "onboarding.checkingPermission"), cx),
            Some(true) => status_row(
                icon("check").size_5(),
                t(cx, "onboarding.accessibilityGranted"),
                p.success,
            ),
            Some(false) => status_row(
                icon("alert-circle").size_5(),
                t(cx, "onboarding.accessibilityNotGranted"),
                p.ac,
            ),
        }
        .mb_6()
    }

    fn render_action(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let enabled = !self.requesting && self.granted.is_some();

        let button = accent_button("open-settings", enabled, palette(cx).surface, cx)
            .gap_2()
            .px_6()
            .when(enabled, |button| {
                button.on_click(cx.listener(|this, _, _, cx| this.open_settings(cx)))
            })
            .child(button_label(&t(cx, "onboarding.openSystemPreferences")))
            .child(icon("external-link").size_4());

        div().flex().justify_center().child(button)
    }
}

impl Render for AccessibilityPermissionStep {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);

        let content = div()
            .child(icon_box(icon("shield").text_color(p.ac), cx))
            .child(title(&t(cx, "onboarding.accessibilityTitle"), cx))
            .child(description(t(cx, "onboarding.accessibilityDescription"), cx).mb_4())
            .when(self.granted == Some(false), |content| {
                content.child(side_note(t(cx, "onboarding.accessibilityInstructions"), cx).mb_6())
            })
            .child(self.render_status(cx))
            .when(self.granted != Some(true), |content| {
                content.child(self.render_action(cx))
            });

        step_column("[STEP_02] > ACCESSIBILITY_PERMISSION", p.ac).child(card(content, cx))
    }
}
