//! Step 6: downloading the default transcription model.

use gpui_kit::{
    AnimationExt as _, Context, Div, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, Window, div, prelude::*, relative,
};
use std::time::{Duration, Instant};

use super::parts::{
    accent_button, button_label, card, description, icon_box, status_row, step_column, title,
};
use super::transition::Tween;
use crate::backend::{
    self, AppEvent, CommandResult, DEFAULT_MODEL_ID, DOWNLOAD_CANCELLED_ERROR,
    DOWNLOAD_IN_PROGRESS_ERROR, ModelDownloadProgress, command_error,
};
use crate::events;
use crate::ui::download_format::{format_eta, format_mb};
use crate::ui::motion::Transitions as _;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::{icon, spinning};
use crate::ui::{t, t_upper, t_with};

const DOWNLOAD_SIZE: &str = "~1.5 GB";
/// How long the progress bar takes to reach a new length.
const BAR_TRANSITION: Duration = Duration::from_millis(300);

/// Raised once the model is on disk.
pub struct DownloadComplete;

/// The download's speed and the time it has left, from its progress reports.
#[derive(Debug, Default)]
struct DownloadMeter {
    last_sample: Option<(Instant, u64)>,
    bytes_per_second: Option<f64>,
    eta_seconds: Option<f64>,
}

impl DownloadMeter {
    fn record(&mut self, now: Instant, downloaded_bytes: u64, total_bytes: u64) {
        if let Some((at, bytes)) = self.last_sample {
            let elapsed_ms = now.saturating_duration_since(at).as_millis();

            if elapsed_ms > 0 && downloaded_bytes >= bytes {
                let instant = (downloaded_bytes - bytes) as f64 * 1000. / elapsed_ms as f64;

                // Smooth the instantaneous rate so the readout doesn't flicker.
                self.bytes_per_second = Some(match self.bytes_per_second {
                    None => instant,
                    Some(previous) => previous * 0.7 + instant * 0.3,
                });
            }
        }
        self.last_sample = Some((now, downloaded_bytes));

        let remaining_bytes = total_bytes as f64 - downloaded_bytes as f64;
        self.eta_seconds = self
            .bytes_per_second
            .filter(|speed| *speed > 0. && remaining_bytes > 0.)
            .map(|speed| remaining_bytes / speed);
    }
}

pub struct ModelDownloadStep {
    /// Whole percentage, 0-100.
    progress: u32,
    /// Bytes downloaded and bytes in total, once a progress report has come.
    transferred: Option<(u64, u64)>,
    meter: DownloadMeter,
    downloading: bool,
    downloaded: bool,
    error: Option<SharedString>,
    cancel_requested: bool,
    /// The length of the progress bar, in percent.
    bar: Tween,
    _events: Subscription,
}

impl EventEmitter<DownloadComplete> for ModelDownloadStep {}

impl ModelDownloadStep {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let events = cx.subscribe(&events::hub(cx), |this, _, event, cx| {
            if let AppEvent::ModelDownloadProgress(report) = event {
                this.handle_progress(report, cx);
            }
        });

        let model_status = backend::call(cx, |backend| backend.check_model_status());
        cx.spawn(async move |this, cx| {
            if model_status.await != Ok(true) {
                return;
            }

            this.update(cx, |this, cx| {
                this.set_progress(100);
                this.mark_downloaded(cx);
            })
            .ok();
        })
        .detach();

        Self {
            progress: 0,
            transferred: None,
            meter: DownloadMeter::default(),
            downloading: false,
            downloaded: false,
            error: None,
            cancel_requested: false,
            bar: Tween::resting(0., BAR_TRANSITION),
            _events: events,
        }
    }

    fn bar_is_shown(&self) -> bool {
        self.downloading && !self.downloaded
    }

    fn set_progress(&mut self, progress: u32) {
        // A bar that is on screen slides to its new length; one that appears
        // starts out at it.
        if self.bar_is_shown() {
            self.bar.retarget(progress as f32);
        } else {
            self.bar.jump_to(progress as f32);
        }

        self.progress = progress;
    }

    fn mark_downloaded(&mut self, cx: &mut Context<Self>) {
        self.downloaded = true;
        cx.emit(DownloadComplete);
        cx.notify();
    }

    fn handle_progress(&mut self, report: &ModelDownloadProgress, cx: &mut Context<Self>) {
        if report.model_id != DEFAULT_MODEL_ID {
            return;
        }

        self.meter
            .record(Instant::now(), report.downloaded_bytes, report.total_bytes);
        self.set_progress(report.progress);
        self.transferred = Some((report.downloaded_bytes, report.total_bytes));

        if report.progress >= 100 {
            self.mark_downloaded(cx);
        } else {
            // A download may already be streaming (started from Settings
            // before this step was shown): reflect it.
            self.downloading = true;
        }

        cx.notify();
    }

    fn start_download(&mut self, cx: &mut Context<Self>) {
        // The progress is not reset: the backend resumes a partial download,
        // so the bar jumps forward with the first report, not back to 0.
        self.downloading = true;
        self.error = None;
        self.cancel_requested = false;
        self.bar.jump_to(self.progress as f32);
        cx.notify();

        let download = backend::call(cx, |backend| backend.download_model(DEFAULT_MODEL_ID));
        cx.spawn(async move |this, cx| {
            let result = download.await;
            this.update(cx, |this, cx| this.finish_download(result, cx))
                .ok();
        })
        .detach();
    }

    fn finish_download(&mut self, result: CommandResult<()>, cx: &mut Context<Self>) {
        let message = match result {
            Ok(()) => {
                self.mark_downloaded(cx);
                return;
            }
            Err(error) => command_error("download_whisper_model", &error),
        };

        // The same download is already streaming; keep showing its progress.
        if message.contains(DOWNLOAD_IN_PROGRESS_ERROR) {
            return;
        }

        self.downloading = false;
        if !self.cancel_requested && !message.contains(DOWNLOAD_CANCELLED_ERROR) {
            self.error = Some(message.into());
        }
        self.cancel_requested = false;
        cx.notify();
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        self.cancel_requested = true;

        let cancellation = backend::call(cx, |backend| {
            backend.cancel_model_download(DEFAULT_MODEL_ID)
        });
        cx.spawn(async move |this, cx| {
            let Err(error) = cancellation.await else {
                return;
            };

            this.update(cx, |this, cx| {
                this.cancel_requested = false;
                this.error = Some(command_error("cancel_model_download", &error).into());
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The transferred size, then the speed and the time left once they are
    /// known.
    fn transfer_summary(&self, downloaded: u64, total: u64, cx: &Context<Self>) -> String {
        let mut summary = t_with(
            cx,
            "settings.modelDownloadedOfTotal",
            &[
                ("downloaded", &format_mb(downloaded as f64)),
                ("total", &format_mb(total as f64)),
            ],
        )
        .to_string();

        if let Some(speed) = self.meter.bytes_per_second {
            summary.push_str(" · ");
            summary.push_str(&t_with(
                cx,
                "settings.modelDownloadSpeed",
                &[("speed", &format_mb(speed))],
            ));
        }

        if let Some(eta) = self.meter.eta_seconds {
            summary.push_str(" · ");
            summary.push_str(&t_with(
                cx,
                "settings.modelEta",
                &[("eta", &format_eta(eta))],
            ));
        }

        summary
    }

    fn render_downloading(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let p = palette(cx);
        let bar = self.bar;

        let fill = div().h_full().bg(p.ac).with_animation(
            bar.animation_id("download-bar"),
            bar.animation(),
            move |fill, progress| fill.w(relative(bar.frame(progress) / 100.)),
        );

        let cancel = div()
            .id("cancel-download")
            .transition_colors()
            .px_4()
            .py_2()
            .type_xs()
            .border_1()
            .border_color(p.border_strong)
            .text_color(p.txt_secondary)
            .hover(|button| button.border_color(p.ac).text_color(p.ac))
            .on_click(cx.listener(|this, _, _, cx| this.cancel(cx)))
            .child(text(t_upper(cx, "settings.cancel")).tracking_wider());

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .w_full()
                    .h_2()
                    .bg(p.border)
                    .overflow_hidden()
                    .child(fill),
            )
            .child(status_row(
                spinning("downloading", icon("loader").size_4()),
                format!(
                    "{}% — {}",
                    self.progress,
                    t(cx, "onboarding.modelDownloading")
                )
                .into(),
                p.txt_secondary,
            ))
            .when_some(self.transferred, |block, (downloaded, total)| {
                block.child(
                    div()
                        .type_xs()
                        .text_color(p.txt_muted)
                        .child(text(self.transfer_summary(downloaded, total, cx)).center()),
                )
            })
            .child(div().flex().justify_center().child(cancel))
    }

    fn render_resume_note(&self, cx: &Context<Self>) -> Div {
        div()
            .type_xs()
            .text_color(palette(cx).txt_muted)
            .child(text(t(cx, "settings.modelResumeNote")).center())
    }

    fn render_idle(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let p = palette(cx);

        let download = accent_button("download", true, p.surface, cx)
            .px_6()
            .on_click(cx.listener(|this, _, _, cx| this.start_download(cx)))
            .child(button_label(&t(cx, "onboarding.downloadModel")));

        div()
            .child(
                div().mb_4().type_xs().text_color(p.txt_muted).child(
                    text(format!(
                        "{}: {DOWNLOAD_SIZE}",
                        t(cx, "onboarding.modelSize")
                    ))
                    .center(),
                ),
            )
            .when(self.progress > 0, |block| {
                block.child(self.render_resume_note(cx).mb_4())
            })
            .child(div().flex().justify_center().child(download))
    }

    fn render_error(
        &self,
        error: SharedString,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let p = palette(cx);

        let retry = div()
            .id("retry-download")
            .transition_colors()
            .px_4()
            .py_2()
            .type_xs()
            .border_1()
            .border_color(p.ac)
            .text_color(p.ac)
            .hover(|button| button.bg(p.ac).text_color(p.ac_on))
            .on_click(cx.listener(|this, _, _, cx| this.start_download(cx)))
            .child(text(t_upper(cx, "onboarding.retryDownload")).tracking_wider());

        div()
            .child(status_row(
                icon("alert-circle").size_4(),
                t(cx, "settings.modelDownloadFailed"),
                p.ac,
            ))
            .when(!error.is_empty(), |block| {
                block.child(
                    div()
                        .mt_2()
                        .type_xs()
                        .text_color(p.txt_faint)
                        .child(text(error).break_all().center()),
                )
            })
            .when(self.progress > 0, |block| {
                block.child(self.render_resume_note(cx).mt_2())
            })
            .child(div().mt_3().flex().justify_center().child(retry))
    }
}

impl Render for ModelDownloadStep {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let downloading = self.bar_is_shown();
        let failure = self.error.clone().filter(|_| !self.downloaded);

        let state_icon = if self.downloaded {
            icon("check").text_color(p.success)
        } else {
            icon("download").text_color(p.ac)
        };

        let state = div()
            .when(self.downloaded, |state| {
                state.child(status_row(
                    icon("check").size_5(),
                    t(cx, "onboarding.modelDownloadComplete"),
                    p.success,
                ))
            })
            .when(downloading, |state| {
                state.child(self.render_downloading(cx))
            })
            .when(
                !self.downloading && !self.downloaded && self.error.is_none(),
                |state| state.child(self.render_idle(cx)),
            )
            .when_some(failure, |state, error| {
                // Under the progress of a download that could not be
                // cancelled, the failure keeps its distance.
                state.child(
                    div()
                        .when(downloading, |failure| failure.mt_3())
                        .child(self.render_error(error, cx)),
                )
            });

        let content = div()
            .child(icon_box(state_icon, cx))
            .child(title(&t(cx, "onboarding.modelDownloadTitle"), cx))
            .child(description(t(cx, "onboarding.modelDownloadDescription"), cx).mb_8())
            .child(state)
            .child(
                div()
                    .mt_6()
                    .pt_4()
                    .border_t_1()
                    .border_color(p.border)
                    .type_xs()
                    .text_color(p.txt_muted)
                    .child(text(t(cx, "onboarding.modelDownloadNote")).center()),
            );

        step_column("[STEP_06] > DOWNLOAD_MODEL", p.ac).child(card(content, cx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: u64 = 1024 * 1024;

    #[test]
    fn the_first_report_gives_no_speed() {
        let mut meter = DownloadMeter::default();
        meter.record(Instant::now(), 10 * MB, 100 * MB);

        assert_eq!(meter.bytes_per_second, None);
        assert_eq!(meter.eta_seconds, None);
    }

    #[test]
    fn speed_and_time_left_come_from_two_reports() {
        let start = Instant::now();
        let mut meter = DownloadMeter::default();
        meter.record(start, 10 * MB, 100 * MB);
        meter.record(start + Duration::from_secs(2), 30 * MB, 100 * MB);

        assert_eq!(meter.bytes_per_second, Some(10. * MB as f64));
        assert_eq!(meter.eta_seconds, Some(7.));
    }

    #[test]
    fn the_speed_is_smoothed_over_reports() {
        let start = Instant::now();
        let mut meter = DownloadMeter::default();
        meter.record(start, 0, 1000 * MB);
        meter.record(start + Duration::from_secs(1), 10 * MB, 1000 * MB);
        meter.record(start + Duration::from_secs(2), 30 * MB, 1000 * MB);

        // 70% of the previous 10 MB/s and 30% of the latest 20 MB/s.
        let speed = meter.bytes_per_second.unwrap();
        assert!((speed - 13. * MB as f64).abs() < 1.);
    }

    #[test]
    fn a_stalled_download_has_no_time_left_to_show() {
        let start = Instant::now();
        let mut meter = DownloadMeter::default();
        meter.record(start, 10 * MB, 100 * MB);
        meter.record(start + Duration::from_secs(1), 10 * MB, 100 * MB);

        assert_eq!(meter.bytes_per_second, Some(0.));
        assert_eq!(meter.eta_seconds, None);
    }

    #[test]
    fn reports_that_go_backwards_or_come_at_once_keep_the_speed() {
        let start = Instant::now();
        let mut meter = DownloadMeter::default();
        meter.record(start, 10 * MB, 100 * MB);
        meter.record(start + Duration::from_secs(1), 20 * MB, 100 * MB);

        // A restarted download reports fewer bytes than before.
        meter.record(start + Duration::from_secs(2), 5 * MB, 100 * MB);
        assert_eq!(meter.bytes_per_second, Some(10. * MB as f64));

        // Two reports within the same millisecond.
        meter.record(start + Duration::from_secs(2), 6 * MB, 100 * MB);
        assert_eq!(meter.bytes_per_second, Some(10. * MB as f64));
    }

    #[test]
    fn a_finished_download_has_no_time_left() {
        let start = Instant::now();
        let mut meter = DownloadMeter::default();
        meter.record(start, 90 * MB, 100 * MB);
        meter.record(start + Duration::from_secs(1), 100 * MB, 100 * MB);

        assert_eq!(meter.eta_seconds, None);
    }
}
