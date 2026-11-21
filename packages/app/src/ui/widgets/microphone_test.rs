//! Explicit microphone input test with a live level meter and clear feedback
//! when sound is detected, when no sound is heard, or when capture fails.

use gpui_kit::{
    App, Bounds, Context, Div, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, Task, Window, canvas, div, fill, point, prelude::*, px,
};
use std::time::Duration;

use crate::backend::{
    self, AppEvent, AudioDevice, MICROPHONE_PERMISSION_DENIED_ERROR,
    MICROPHONE_TEST_RECORDING_IN_PROGRESS_ERROR, PermissionState,
};
use crate::events;
use crate::settings::SettingsStore;
use crate::ui::motion::Transitions as _;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::focus::{focus_ring, focus_visible};
use crate::ui::widgets::{animations_frozen, icon, spinning};
use crate::ui::{t, t_upper};

const MICROPHONE_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone";

/// How long to wait before warning that no audible input has been detected.
const NO_SOUND_TIMEOUT: Duration = Duration::from_secs(3);

/// Minimum input level on the 0..=100 scale counted as audible sound rather
/// than silence or an idle hardware noise floor.
const SOUND_THRESHOLD: f32 = 1.0;

/// Smooth release factor applied per incoming ~33 ms level update.
const LEVEL_DECAY: f32 = 0.72;

const METER_SEGMENTS: usize = 20;
const METER_PEAK_START: usize = 16;
const METER_SEGMENT_GAP: f32 = 3.0;

/// Visual backdrop the control sits on, so its own fill contrasts cleanly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestSurface {
    /// Inside the elevated onboarding step card (`p.surface`).
    Onboarding,
    /// Inside the settings modal content pane (`p.base`).
    Settings,
}

#[derive(Debug, Clone, PartialEq)]
enum Phase {
    Idle,
    Starting { session_id: u64 },
    Running { session_id: u64 },
    PermissionDenied,
    RecordingBusy,
    Failed(SharedString),
}

fn selected_device(cx: &App) -> Option<String> {
    SettingsStore::get(cx)
        .selected_microphone_id
        .clone()
        .filter(|device| device != "default" && !device.is_empty())
}

/// Maps a 0..=100 audio level onto 0..=1 with a mild perceptual curve so
/// normal speech fills the meter clearly while silence stays flat.
fn meter_fraction(level: f32) -> f32 {
    if !level.is_finite() || level <= 0.25 {
        return 0.0;
    }
    (level.clamp(0.0, 100.0) / 100.0).powf(0.55)
}

pub struct MicrophoneTest {
    surface: TestSurface,
    device: Option<String>,
    /// The OS default microphone name from the last device enumeration, if
    /// known (`None` until the first refresh completes).
    known_default: Option<Option<String>>,
    phase: Phase,
    displayed_level: f32,
    sound_detected: bool,
    no_sound: bool,
    no_sound_timer: Option<Task<()>>,
    toggle_focus: FocusHandle,
    settings_focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl MicrophoneTest {
    pub fn new(surface: TestSurface, cx: &mut Context<Self>) -> Self {
        let subscriptions = vec![
            cx.subscribe(&events::hub(cx), |this, _, event, cx| {
                this.handle_event(event, cx);
            }),
            cx.observe(&SettingsStore::entity(cx), |this, _, cx| {
                this.settings_changed(cx);
            }),
            cx.on_release(|this, cx| {
                if let Some(session_id) = this.active_session() {
                    this.phase = Phase::Idle;
                    this.no_sound_timer = None;
                    backend::call(cx, move |backend| backend.stop_microphone_test(session_id))
                        .detach();
                }
            }),
        ];

        Self {
            surface,
            device: selected_device(cx),
            known_default: None,
            phase: Phase::Idle,
            displayed_level: 0.0,
            sound_detected: false,
            no_sound: false,
            no_sound_timer: None,
            toggle_focus: cx.focus_handle().tab_stop(true),
            settings_focus: cx.focus_handle().tab_stop(true),
            _subscriptions: subscriptions,
        }
    }

    /// Updates the known OS default input device after a device list refresh.
    /// When System Default is selected and the default device changes, an
    /// active test restarts on the new default so the meter stays aligned with
    /// the selector label.
    pub fn sync_devices(&mut self, devices: &[AudioDevice], cx: &mut Context<Self>) {
        let next_default = devices
            .iter()
            .find(|device| device.is_default)
            .map(|device| device.name.clone());
        let previous = self.known_default.replace(next_default.clone());

        if self.device.is_none()
            && let Some(previous_default) = previous
            && previous_default != next_default
        {
            if self.is_active() {
                self.start(cx);
            } else if matches!(
                self.phase,
                Phase::Failed(_) | Phase::RecordingBusy | Phase::PermissionDenied
            ) {
                self.phase = Phase::Idle;
                cx.notify();
            }
        }
    }

    fn active_session(&self) -> Option<u64> {
        match self.phase {
            Phase::Starting { session_id } | Phase::Running { session_id } => Some(session_id),
            _ => None,
        }
    }

    fn is_active(&self) -> bool {
        self.active_session().is_some()
    }

    fn toggle(&mut self, cx: &mut Context<Self>) {
        if self.is_active() {
            self.stop(cx);
        } else {
            self.start(cx);
        }
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        if let Some(previous_session) = self.active_session() {
            backend::call(cx, move |backend| {
                backend.stop_microphone_test(previous_session)
            })
            .detach();
        }

        let session_id = backend::next_microphone_test_session();
        let device = selected_device(cx);
        self.device = device.clone();
        self.phase = Phase::Starting { session_id };
        self.displayed_level = 0.0;
        self.sound_detected = false;
        self.no_sound = false;
        self.no_sound_timer = None;
        cx.notify();

        let permission = backend::call(cx, |backend| backend.microphone_permission());
        cx.spawn(async move |this, cx| {
            let permission = permission.await;
            let should_request = match permission {
                PermissionState::Granted => false,
                PermissionState::Prompt => true,
                PermissionState::Denied => {
                    let _ = this.update(cx, |this, cx| {
                        if this.phase == (Phase::Starting { session_id }) {
                            this.phase = Phase::PermissionDenied;
                            cx.notify();
                        }
                    });
                    return;
                }
            };

            if should_request {
                let request = this
                    .update(cx, |this, cx| {
                        (this.phase == Phase::Starting { session_id }).then(|| {
                            backend::call(cx, |backend| backend.request_microphone_permission())
                        })
                    })
                    .ok()
                    .flatten();
                let Some(request) = request else {
                    return;
                };
                let granted = request.await;
                if !granted {
                    let _ = this.update(cx, |this, cx| {
                        if this.phase == (Phase::Starting { session_id }) {
                            this.phase = Phase::PermissionDenied;
                            cx.notify();
                        }
                    });
                    return;
                }
            }

            let start = this
                .update(cx, |this, cx| {
                    (this.phase == Phase::Starting { session_id }).then(|| {
                        let device = device.clone();
                        backend::call(cx, move |backend| {
                            backend.start_microphone_test(device, session_id)
                        })
                    })
                })
                .ok()
                .flatten();
            let Some(start) = start else {
                return;
            };

            let result = start.await;
            let _ = this.update(cx, |this, cx| {
                if this.phase != (Phase::Starting { session_id }) {
                    return;
                }

                match result {
                    Ok(true) => {
                        this.phase = Phase::Running { session_id };
                        if !this.sound_detected {
                            this.schedule_no_sound_check(session_id, cx);
                        }
                    }
                    Ok(false) => {
                        this.displayed_level = 0.0;
                        this.sound_detected = false;
                        this.no_sound = false;
                        this.no_sound_timer = None;
                        this.phase = Phase::Idle;
                    }
                    Err(error) => {
                        this.displayed_level = 0.0;
                        this.sound_detected = false;
                        this.no_sound = false;
                        this.no_sound_timer = None;
                        this.phase = classify_error(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn stop(&mut self, cx: &mut Context<Self>) {
        let previous = self.active_session();
        self.phase = Phase::Idle;
        self.displayed_level = 0.0;
        self.sound_detected = false;
        self.no_sound = false;
        self.no_sound_timer = None;

        if let Some(session_id) = previous {
            backend::call(cx, move |backend| backend.stop_microphone_test(session_id)).detach();
        }
        cx.notify();
    }

    fn schedule_no_sound_check(&mut self, session_id: u64, cx: &mut Context<Self>) {
        self.no_sound_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(NO_SOUND_TIMEOUT).await;
            let _ = this.update(cx, |this, cx| {
                if this.phase == (Phase::Running { session_id }) && !this.sound_detected {
                    this.no_sound = true;
                    cx.notify();
                }
            });
        }));
    }

    fn settings_changed(&mut self, cx: &mut Context<Self>) {
        let next_device = selected_device(cx);
        if next_device != self.device {
            self.device = next_device;
            if self.is_active() {
                self.start(cx);
                return;
            }
            if matches!(
                self.phase,
                Phase::Failed(_) | Phase::RecordingBusy | Phase::PermissionDenied
            ) {
                self.phase = Phase::Idle;
            }
        }
        cx.notify();
    }

    fn handle_event(&mut self, event: &AppEvent, cx: &mut Context<Self>) {
        match event {
            AppEvent::MicrophoneTestLevel { level, session_id } => {
                let Some(active) = self.active_session() else {
                    return;
                };
                if session_id.is_some_and(|id| id != active) {
                    return;
                }

                let target = meter_fraction(*level);
                if animations_frozen() || target >= self.displayed_level {
                    self.displayed_level = target;
                } else {
                    let decayed = (self.displayed_level * LEVEL_DECAY).max(target);
                    self.displayed_level = if decayed < 0.01 { 0.0 } else { decayed };
                }

                if level.is_finite() && *level >= SOUND_THRESHOLD && !self.sound_detected {
                    self.sound_detected = true;
                    self.no_sound = false;
                    self.no_sound_timer = None;
                }
                cx.notify();
            }
            AppEvent::MicrophoneTestError {
                message,
                session_id,
            } => {
                let Some(active) = self.active_session() else {
                    return;
                };
                if session_id.is_some_and(|id| id != active) {
                    return;
                }

                self.displayed_level = 0.0;
                self.sound_detected = false;
                self.no_sound = false;
                self.no_sound_timer = None;
                self.phase = classify_error(message.clone());
                cx.notify();
            }
            AppEvent::MicrophoneTestStopped { up_to_session } => {
                let Some(active) = self.active_session() else {
                    return;
                };
                if up_to_session.is_some_and(|cutoff| active > cutoff) {
                    return;
                }
                self.stop(cx);
            }
            _ => {}
        }
    }

    fn render_meter(&self, cx: &App) -> impl IntoElement + use<> {
        let p = palette(cx);
        let level = if self.is_active() {
            self.displayed_level
        } else {
            0.0
        };
        let lit = if level <= 0.0 {
            0
        } else {
            ((level.clamp(0.0, 1.0) * METER_SEGMENTS as f32).ceil() as usize).min(METER_SEGMENTS)
        };
        let track_color = if p.dark { p.border_strong } else { p.border };

        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let total_width = bounds.size.width.as_f32();
                if total_width <= 0.0 {
                    return;
                }
                let scale = window.scale_factor();
                let gap = METER_SEGMENT_GAP;
                let seg_width = ((total_width - gap * (METER_SEGMENTS - 1) as f32)
                    / METER_SEGMENTS as f32)
                    .max(1.0);
                let snap = |value: f32| (value * scale).round() / scale;
                let top = bounds.top();
                let bottom = bounds.bottom();

                for index in 0..METER_SEGMENTS {
                    let start = index as f32 * (seg_width + gap);
                    let end = (start + seg_width).min(total_width);
                    let left = bounds.left() + px(snap(start));
                    let right = bounds.left() + px(snap(end));
                    if right <= left {
                        continue;
                    }

                    let color = if index < lit {
                        if index >= METER_PEAK_START {
                            p.ac
                        } else {
                            p.success
                        }
                    } else {
                        track_color
                    };

                    window.paint_quad(fill(
                        Bounds::from_corners(point(left, top), point(right, bottom)),
                        color,
                    ));
                }
            },
        )
        .w_full()
        .h(px(10.))
    }

    fn render_feedback(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<Div> {
        let p = palette(cx);

        match &self.phase {
            Phase::Idle => None,
            Phase::Starting { .. } => Some(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .type_xs()
                    .text_color(p.txt_hint)
                    .child(spinning(
                        "mic-test-starting",
                        icon("spinner-ring").size_3p5().flex_shrink_0(),
                    ))
                    .child(
                        div()
                            .min_w_0()
                            .child(text(t(cx, "microphoneTest.starting"))),
                    ),
            ),
            Phase::Running { .. } if self.sound_detected => Some(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .type_xs()
                    .text_color(p.success)
                    .child(icon("check").size_3p5().flex_shrink_0())
                    .child(
                        div()
                            .min_w_0()
                            .child(text(t(cx, "microphoneTest.soundDetected"))),
                    ),
            ),
            Phase::Running { .. } if self.no_sound => Some(
                div()
                    .flex()
                    .items_start()
                    .gap_2()
                    .type_xs()
                    .text_color(p.ac)
                    .child(icon("alert-circle").size_3p5().mt_0p5().flex_shrink_0())
                    .child(div().min_w_0().child(text(t(cx, "microphoneTest.noSound")))),
            ),
            Phase::Running { .. } => Some(
                div()
                    .flex()
                    .items_start()
                    .gap_2()
                    .type_xs()
                    .text_color(p.txt_hint)
                    .child(icon("mic").size_3p5().mt_0p5().flex_shrink_0())
                    .child(
                        div()
                            .min_w_0()
                            .child(text(t(cx, "microphoneTest.speakNow"))),
                    ),
            ),
            Phase::PermissionDenied => {
                let settings_focused = focus_visible(&self.settings_focus, window);

                Some(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .flex()
                                .items_start()
                                .gap_2()
                                .type_xs()
                                .text_color(p.ac)
                                .child(icon("alert-circle").size_3p5().mt_0p5().flex_shrink_0())
                                .child(
                                    div()
                                        .min_w_0()
                                        .child(text(t(cx, "microphoneTest.permissionDenied"))),
                                ),
                        )
                        .child(
                            div().flex().child(
                                div()
                                    .id("mic-test-open-settings")
                                    .transition_colors()
                                    .track_focus(&self.settings_focus)
                                    .relative()
                                    .flex()
                                    .items_center()
                                    .gap_1p5()
                                    .px_2p5()
                                    .py_1()
                                    .border_1()
                                    .border_color(p.border_strong)
                                    .type_px(11.)
                                    .text_color(p.txt_secondary)
                                    .hover(|button| button.border_color(p.ac).text_color(p.ac))
                                    .on_click(|_, _, cx| cx.open_url(MICROPHONE_SETTINGS_URL))
                                    .child(
                                        text(t_upper(cx, "onboarding.openSystemPreferences"))
                                            .tracking_wider(),
                                    )
                                    .child(icon("external-link").size_3p5())
                                    .when(settings_focused, |button| {
                                        button.child(focus_ring(4., cx))
                                    }),
                            ),
                        ),
                )
            }
            Phase::RecordingBusy => Some(
                div()
                    .flex()
                    .items_start()
                    .gap_2()
                    .type_xs()
                    .text_color(p.ac)
                    .child(icon("alert-circle").size_3p5().mt_0p5().flex_shrink_0())
                    .child(
                        div()
                            .min_w_0()
                            .child(text(t(cx, "microphoneTest.busyRecording"))),
                    ),
            ),
            Phase::Failed(detail) => Some(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .type_xs()
                            .text_color(p.ac)
                            .child(icon("alert-circle").size_3p5().flex_shrink_0())
                            .child(div().min_w_0().child(text(t(cx, "microphoneTest.failed")))),
                    )
                    .when(!detail.is_empty(), |block| {
                        block.child(
                            div()
                                .type_px(11.)
                                .text_color(p.txt_hint)
                                .child(text(detail.clone())),
                        )
                    }),
            ),
        }
    }
}

fn classify_error(error: String) -> Phase {
    if error.contains(MICROPHONE_PERMISSION_DENIED_ERROR) {
        Phase::PermissionDenied
    } else if error.contains(MICROPHONE_TEST_RECORDING_IN_PROGRESS_ERROR) {
        Phase::RecordingBusy
    } else {
        Phase::Failed(error.into())
    }
}

impl Render for MicrophoneTest {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let active = self.is_active();
        let starting = matches!(self.phase, Phase::Starting { .. });
        let button_focused = focus_visible(&self.toggle_focus, window);

        let (panel_bg, panel_border, idle_button_bg) = match self.surface {
            TestSurface::Onboarding => (p.base, p.border_strong, p.surface),
            TestSurface::Settings => (p.surface, p.border, p.base),
        };

        let button_icon = if starting {
            spinning("mic-test-button-spinner", icon("loader").size_3p5()).into_any_element()
        } else if active {
            icon("x").size_3p5().into_any_element()
        } else {
            icon("mic").size_3p5().into_any_element()
        };
        let button_label = if active {
            t_upper(cx, "microphoneTest.stop")
        } else {
            t_upper(cx, "microphoneTest.start")
        };

        let toggle_button = div()
            .id("microphone-test-toggle")
            .transition_colors()
            .track_focus(&self.toggle_focus)
            .relative()
            .flex()
            .items_center()
            .gap_1p5()
            .px_3()
            .py_1p5()
            .border_1()
            .type_xs()
            .map(|button| {
                if active {
                    button
                        .border_color(p.ac)
                        .bg(p.ac_bg)
                        .text_color(p.ac)
                        .hover(|button| button.bg(p.ac).text_color(p.ac_on))
                } else {
                    button
                        .border_color(p.border_strong)
                        .bg(idle_button_bg)
                        .text_color(p.txt_primary)
                        .hover(|button| button.border_color(p.ac).text_color(p.ac))
                }
            })
            .on_click(cx.listener(|this, _, _, cx| this.toggle(cx)))
            .child(button_icon)
            .child(text(button_label).tracking_wider())
            .when(button_focused, |button| button.child(focus_ring(4., cx)));

        let feedback = self.render_feedback(window, cx);

        div()
            .w_full()
            .p_3()
            .bg(panel_bg)
            .border_1()
            .border_color(panel_border)
            .flex()
            .flex_col()
            .gap_2p5()
            .font_mono()
            .child(div().flex().items_center().child(toggle_button))
            .child(self.render_meter(cx))
            .children(feedback)
    }
}
