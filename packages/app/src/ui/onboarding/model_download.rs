//! Step 6: choosing the transcription model and downloading it.

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
use crate::analytics;
use crate::backend::{
    self, AppEvent, CommandResult, DOWNLOAD_CANCELLED_ERROR, DOWNLOAD_IN_PROGRESS_ERROR,
    ModelDownloadProgress, ModelInfo, command_error,
};
use crate::events;
use crate::ui::download_format::{format_eta, format_mb};
use crate::ui::motion::Transitions as _;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::model_summary::model_summary;
use crate::ui::widgets::{icon, spinning};
use crate::ui::{t, t_upper, t_with};

/// How long the progress bar takes to reach a new length.
const BAR_TRANSITION: Duration = Duration::from_millis(300);

/// Raised each time the step learns whether the chosen model is on disk and
/// in use, which is what lets the wizard go on.
pub struct ModelReady(pub bool);

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
    /// The models this build can run, once they are listed.
    models: Vec<ModelInfo>,
    /// The chosen model. A finished download makes it the active one.
    selected: Option<String>,
    /// Whether the chosen model is downloaded and in use.
    ready: bool,
    /// The model being made the active one.
    activating: Option<String>,
    /// Whole percentage of the chosen model's download, 0-100.
    progress: u32,
    /// Bytes downloaded and bytes in total, once a progress report has come.
    transferred: Option<(u64, u64)>,
    meter: DownloadMeter,
    downloading: bool,
    error: Option<SharedString>,
    cancel_requested: bool,
    /// The length of the progress bar, in percent.
    bar: Tween,
    _events: Subscription,
}

impl EventEmitter<ModelReady> for ModelDownloadStep {}

impl ModelDownloadStep {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let events = cx.subscribe(&events::hub(cx), |this, _, event, cx| {
            if let AppEvent::ModelDownloadProgress(report) = event {
                this.handle_progress(report, cx);
            }
        });

        let mut step = Self {
            models: Vec::new(),
            selected: None,
            ready: false,
            activating: None,
            progress: 0,
            transferred: None,
            meter: DownloadMeter::default(),
            downloading: false,
            error: None,
            cancel_requested: false,
            bar: Tween::resting(0., BAR_TRANSITION),
            _events: events,
        };
        step.load_models(cx);
        step
    }

    fn load_models(&mut self, cx: &mut Context<Self>) {
        let models = backend::call(cx, |backend| backend.list_models());
        cx.spawn(async move |this, cx| {
            let models = models.await;

            this.update(cx, |this, cx| match models {
                Ok(models) => this.show_models(models, cx),
                Err(error) => {
                    this.error = Some(command_error("list_models", &error).into());
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Offers the models that can transcribe in this build, and picks one: the
    /// active model when it is downloaded, else another downloaded one, else
    /// the active model, which on a fresh profile is the recommended one.
    fn show_models(&mut self, models: Vec<ModelInfo>, cx: &mut Context<Self>) {
        self.models = models
            .into_iter()
            .filter(|model| model.available && !model.experimental)
            .collect();

        let active = self.models.iter().find(|model| model.active);
        let choice = active
            .filter(|model| model.downloaded)
            .or_else(|| self.models.iter().find(|model| model.downloaded))
            .or(active)
            .or_else(|| self.models.iter().find(|model| model.recommended))
            .or_else(|| self.models.first())
            .map(|model| model.id.clone());

        if let Some(model_id) = choice {
            self.choose(model_id, cx);
        }
        cx.notify();
    }

    fn selected_model(&self) -> Option<&ModelInfo> {
        let selected = self.selected.as_deref()?;
        self.models.iter().find(|model| model.id == selected)
    }

    fn set_ready(&mut self, ready: bool, cx: &mut Context<Self>) {
        self.ready = ready;
        cx.emit(ModelReady(ready));
        cx.notify();
    }

    /// Shows `model_id` as the chosen model, with no download progress yet.
    fn select(&mut self, model_id: String) {
        self.selected = Some(model_id);
        self.progress = 0;
        self.transferred = None;
        self.meter = DownloadMeter::default();
        self.error = None;
        self.bar.jump_to(0.);
    }

    /// Picks `model_id`, and makes it the active model if it is downloaded
    /// already. The choice is held while a model downloads or is being made
    /// the active one, so that the last model picked is the one in use.
    fn choose(&mut self, model_id: String, cx: &mut Context<Self>) {
        if self.downloading
            || self.activating.is_some()
            || self.selected.as_ref() == Some(&model_id)
        {
            return;
        }

        let (downloaded, active) = self
            .models
            .iter()
            .find(|model| model.id == model_id)
            .map_or((false, false), |model| (model.downloaded, model.active));

        self.select(model_id.clone());
        self.set_ready(downloaded && active, cx);
        if downloaded && !active {
            self.activate(model_id, cx);
        }
    }

    /// Makes `model_id` the model transcriptions use.
    fn activate(&mut self, model_id: String, cx: &mut Context<Self>) {
        self.activating = Some(model_id.clone());

        let activation = backend::call(cx, {
            let model_id = model_id.clone();
            move |backend| backend.set_active_model(&model_id)
        });

        cx.spawn(async move |this, cx| {
            let result = activation.await;

            this.update(cx, |this, cx| {
                this.activating = None;
                if this.selected.as_ref() != Some(&model_id) {
                    return;
                }

                match result {
                    Ok(()) => {
                        for model in &mut this.models {
                            model.active = model.id == model_id;
                        }
                        this.set_ready(true, cx);
                    }
                    Err(error) => {
                        this.error = Some(command_error("set_active_model", &error).into());
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    /// The chosen model is on disk: it becomes the active one.
    fn mark_downloaded(&mut self, model_id: &str, cx: &mut Context<Self>) {
        if let Some(model) = self.models.iter_mut().find(|model| model.id == model_id) {
            model.downloaded = true;
        }
        self.downloading = false;
        self.error = None;

        // The last progress report and the end of the download both get here.
        if !self.ready && self.activating.is_none() {
            self.activate(model_id.to_string(), cx);
        }
        cx.notify();
    }

    fn bar_is_shown(&self) -> bool {
        self.downloading && !self.ready
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

    fn handle_progress(&mut self, report: &ModelDownloadProgress, cx: &mut Context<Self>) {
        if self.selected.as_ref() != Some(&report.model_id) {
            // A download may already be streaming, started on an earlier
            // visit to this step: show it, unless the step is busy with
            // another model or has one ready.
            let offered = self.models.iter().any(|model| model.id == report.model_id);
            let busy = self.downloading || self.ready || self.activating.is_some();
            if !offered || busy || report.progress >= 100 {
                return;
            }
            self.select(report.model_id.clone());
            self.set_ready(false, cx);
        }

        self.meter
            .record(Instant::now(), report.downloaded_bytes, report.total_bytes);
        self.set_progress(report.progress);
        self.transferred = Some((report.downloaded_bytes, report.total_bytes));

        if report.progress >= 100 {
            self.mark_downloaded(&report.model_id, cx);
        } else {
            self.downloading = true;
        }

        cx.notify();
    }

    fn start_download(&mut self, cx: &mut Context<Self>) {
        // Without a model to download, the list could not be read: the retry
        // reads it again.
        let Some(model_id) = self.selected.clone() else {
            self.error = None;
            self.load_models(cx);
            cx.notify();
            return;
        };

        // The progress is not reset: the backend resumes a partial download,
        // so the bar jumps forward with the first report, not back to 0.
        self.downloading = true;
        self.error = None;
        self.cancel_requested = false;
        self.bar.jump_to(self.progress as f32);
        analytics::capture(
            cx,
            "onboarding_model_download_started",
            &[("model", model_id.clone().into())],
        );
        cx.notify();

        let download = backend::call(cx, {
            let model_id = model_id.clone();
            move |backend| backend.download_model(&model_id)
        });
        cx.spawn(async move |this, cx| {
            let result = download.await;
            this.update(cx, |this, cx| this.finish_download(&model_id, result, cx))
                .ok();
        })
        .detach();
    }

    fn finish_download(
        &mut self,
        model_id: &str,
        result: CommandResult<()>,
        cx: &mut Context<Self>,
    ) {
        if self.selected.as_deref() != Some(model_id) {
            return;
        }

        let message = match result {
            Ok(()) => {
                self.mark_downloaded(model_id, cx);
                return;
            }
            Err(error) => command_error("download_model", &error),
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
        let Some(model_id) = self.selected.clone() else {
            return;
        };
        self.cancel_requested = true;

        let cancellation =
            backend::call(cx, move |backend| backend.cancel_model_download(&model_id));
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

    /// One model to pick, framed like Settings → Models describes it.
    fn render_option(
        &self,
        index: usize,
        model: &ModelInfo,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let p = palette(cx);
        let selected = self.selected.as_deref() == Some(model.id.as_str());
        let model_id = model.id.clone();

        div()
            .id(("model-option", index))
            .transition_colors()
            .flex()
            .items_start()
            .justify_between()
            .gap_3()
            .p_4()
            .border_1()
            .map(|option| {
                if selected {
                    option.border_color(p.ac).bg(p.ac_bg)
                } else {
                    option.border_color(p.border_strong).bg(p.base)
                }
            })
            .hover(move |option| {
                if selected {
                    option
                } else {
                    option.border_color(p.ac)
                }
            })
            .when(!selected, |option| {
                option.on_click(cx.listener(move |this, _, _, cx| {
                    this.choose(model_id.clone(), cx);
                }))
            })
            .child(model_summary(model, cx))
            .child(div().flex_shrink_0().size_4().when(selected, |mark| {
                mark.child(icon("check").size_4().text_color(p.ac))
            }))
    }

    fn render_idle(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let p = palette(cx);

        let download = accent_button("download", true, p.surface, cx)
            .px_6()
            .on_click(cx.listener(|this, _, _, cx| this.start_download(cx)))
            .child(button_label(&t(cx, "onboarding.downloadModel")));

        div()
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
        let failure = self.error.clone().filter(|_| !self.ready);
        let needs_download = self.selected_model().is_some_and(|model| !model.downloaded);

        let state_icon = if self.ready {
            icon("check").text_color(p.success)
        } else {
            icon("download").text_color(p.ac)
        };

        // While a model downloads the choice is held, and only that model
        // is shown.
        let options: Vec<_> = self
            .models
            .iter()
            .enumerate()
            .filter(|(_, model)| !self.downloading || self.selected.as_ref() == Some(&model.id))
            .map(|(index, model)| self.render_option(index, model, cx).into_any_element())
            .collect();

        let state = div()
            .when(self.ready, |state| {
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
                needs_download && !self.downloading && self.error.is_none(),
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
            // With no models the list is an empty block, which would still
            // take its margin.
            .when(!options.is_empty(), |content| {
                content.child(div().mb_6().flex().flex_col().gap_3().children(options))
            })
            .child(state);

        step_column("[STEP_06] > MODEL_SELECT", p.ac).child(card(content, cx))
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
