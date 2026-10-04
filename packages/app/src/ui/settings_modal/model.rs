//! The Models section: the speech-to-text models, which one is in use, and
//! downloading the others.

use gpui_kit::{
    AnyElement, Context, Div, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Window, div,
    prelude::*, px,
};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::analytics;
use crate::backend::{
    self, AppEvent, DOWNLOAD_CANCELLED_ERROR, DOWNLOAD_IN_PROGRESS_ERROR, ModelDownloadProgress,
    ModelInfo, command_error,
};
use crate::events;
use crate::ui::download_format::{format_eta, format_mb};
use crate::ui::motion::Transitions as _;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::progress_bar::{Progress, progress_bar};
use crate::ui::widgets::{icon, spinning};
use crate::ui::{t, t_upper, t_with, upper};

/// How far a download has come, and how fast it is going.
struct DownloadStats {
    /// Whole percentage, 0-100.
    progress: u32,
    downloaded_bytes: u64,
    total_bytes: u64,
    bytes_per_second: Option<f64>,
    eta_seconds: Option<f64>,
}

/// The bytes a download had reached at some moment.
#[derive(Clone, Copy)]
struct Sample {
    at: Instant,
    bytes: u64,
}

/// The download whose progress is shown.
struct Download {
    model_id: String,
    bar: Progress,
}

/// The download rate after a new sample: the rate since the last sample,
/// smoothed so the readout does not flicker. Without a usable last sample
/// the rate stays what it was.
fn smoothed_speed(
    previous: Option<f64>,
    last_sample: Option<Sample>,
    now: Instant,
    downloaded_bytes: u64,
) -> Option<f64> {
    let Some(sample) = last_sample else {
        return previous;
    };
    let elapsed_ms = now.saturating_duration_since(sample.at).as_millis();
    if elapsed_ms == 0 || downloaded_bytes < sample.bytes {
        return previous;
    }

    let instant = (downloaded_bytes - sample.bytes) as f64 * 1000. / elapsed_ms as f64;

    Some(match previous {
        Some(previous) => previous * 0.7 + instant * 0.3,
        None => instant,
    })
}

/// The seconds a download has left, once it has a rate and is not done.
fn eta_seconds(
    bytes_per_second: Option<f64>,
    downloaded_bytes: u64,
    total_bytes: u64,
) -> Option<f64> {
    let remaining_bytes = total_bytes.checked_sub(downloaded_bytes)?;
    let bytes_per_second = bytes_per_second.filter(|rate| *rate > 0.)?;

    (remaining_bytes > 0).then(|| remaining_bytes as f64 / bytes_per_second)
}

pub(super) struct ModelSettings {
    models: Vec<ModelInfo>,
    download_stats: HashMap<String, DownloadStats>,
    downloading: Option<Download>,
    /// The model being made the active one.
    busy_id: Option<String>,
    error: Option<SharedString>,
    last_samples: HashMap<String, Sample>,
    cancel_requested: HashSet<String>,
    _subscription: Subscription,
}

impl ModelSettings {
    pub(super) fn new(cx: &mut Context<Self>) -> Self {
        let subscription = cx.subscribe(&events::hub(cx), |this, _, event, cx| {
            if let AppEvent::ModelDownloadProgress(progress) = event {
                this.handle_progress(progress, cx);
            }
        });

        let mut section = Self {
            models: Vec::new(),
            download_stats: HashMap::new(),
            downloading: None,
            busy_id: None,
            error: None,
            last_samples: HashMap::new(),
            cancel_requested: HashSet::new(),
            _subscription: subscription,
        };
        section.refresh(cx);
        section
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let models = backend::call(cx, |backend| backend.list_models());

        cx.spawn(async move |this, cx| {
            let models = models.await;

            this.update(cx, |this, cx| {
                match models {
                    Ok(models) => this.models = models,
                    Err(error) => this.error = Some(command_error("list_models", &error).into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn is_downloading(&self, model_id: &str) -> bool {
        self.downloading
            .as_ref()
            .is_some_and(|download| download.model_id == model_id)
    }

    /// Shows the progress of `model_id`'s download, starting the bar at the
    /// progress known so far.
    fn show_download(&mut self, model_id: &str) {
        let progress = self
            .download_stats
            .get(model_id)
            .map_or(0, |stats| stats.progress);

        self.downloading = Some(Download {
            model_id: model_id.to_string(),
            bar: Progress::new(progress as f32),
        });
    }

    fn hide_download(&mut self, model_id: &str) {
        if self.is_downloading(model_id) {
            self.downloading = None;
        }
    }

    fn handle_progress(&mut self, progress: &ModelDownloadProgress, cx: &mut Context<Self>) {
        let model_id = &progress.model_id;
        let now = Instant::now();

        let previous_speed = self
            .download_stats
            .get(model_id)
            .and_then(|stats| stats.bytes_per_second);
        let bytes_per_second = smoothed_speed(
            previous_speed,
            self.last_samples.get(model_id).copied(),
            now,
            progress.downloaded_bytes,
        );
        self.last_samples.insert(
            model_id.clone(),
            Sample {
                at: now,
                bytes: progress.downloaded_bytes,
            },
        );

        self.download_stats.insert(
            model_id.clone(),
            DownloadStats {
                progress: progress.progress,
                downloaded_bytes: progress.downloaded_bytes,
                total_bytes: progress.total_bytes,
                bytes_per_second,
                eta_seconds: eta_seconds(
                    bytes_per_second,
                    progress.downloaded_bytes,
                    progress.total_bytes,
                ),
            },
        );

        if progress.progress >= 100 {
            self.hide_download(model_id);
            self.refresh(cx);
        } else if self.downloading.is_none() {
            // A download may already be running, started before this section
            // was opened: show it.
            self.show_download(model_id);
        }

        if let Some(download) = &mut self.downloading
            && download.model_id == *model_id
        {
            download.bar.set(progress.progress as f32);
        }
        cx.notify();
    }

    fn download(&mut self, model_id: String, cx: &mut Context<Self>) {
        self.error = None;
        self.cancel_requested.remove(&model_id);
        // Progress from an earlier, cancelled attempt stays on screen: the
        // download resumes, so the bar goes on from there rather than from 0.
        self.show_download(&model_id);
        analytics::capture(
            cx,
            "settings_model_download_started",
            &[("model", model_id.clone().into())],
        );
        cx.notify();

        let download = backend::call(cx, {
            let model_id = model_id.clone();
            move |backend| backend.download_model(&model_id)
        });

        cx.spawn(async move |this, cx| {
            let result = download.await;

            this.update(cx, |this, cx| {
                let was_cancelled = this.cancel_requested.remove(&model_id);

                if let Err(error) = result {
                    let message = command_error("download_model", &error);
                    if message.contains(DOWNLOAD_IN_PROGRESS_ERROR) {
                        // The same download is already running; its progress
                        // stays on screen.
                        return;
                    }

                    this.hide_download(&model_id);
                    if !was_cancelled && !message.contains(DOWNLOAD_CANCELLED_ERROR) {
                        this.error = Some(message.into());
                    }
                } else {
                    this.hide_download(&model_id);
                }

                cx.notify();
                this.refresh(cx);
            })
            .ok();
        })
        .detach();
    }

    fn cancel_download(&mut self, model_id: String, cx: &mut Context<Self>) {
        self.cancel_requested.insert(model_id.clone());

        let cancel = backend::call(cx, {
            let model_id = model_id.clone();
            move |backend| backend.cancel_model_download(&model_id)
        });

        cx.spawn(async move |this, cx| {
            let Err(error) = cancel.await else {
                return;
            };

            this.update(cx, |this, cx| {
                this.cancel_requested.remove(&model_id);
                this.error = Some(command_error("cancel_model_download", &error).into());
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn use_model(&mut self, model_id: String, cx: &mut Context<Self>) {
        self.error = None;
        self.busy_id = Some(model_id.clone());
        cx.notify();

        let activate = backend::call(cx, {
            let model_id = model_id.clone();
            move |backend| backend.set_active_model(&model_id)
        });

        cx.spawn(async move |this, cx| {
            let result = activate.await;

            this.update(cx, |this, cx| {
                this.busy_id = None;
                cx.notify();

                if let Err(error) = result {
                    this.error = Some(command_error("set_active_model", &error).into());
                    return;
                }

                analytics::capture(cx, "settings_model_changed", &[("model", model_id.into())]);
                this.refresh(cx);
            })
            .ok();
        })
        .detach();
    }

    /// Whether part of `model` was downloaded by an attempt that then stopped.
    fn has_partial_download(&self, model: &ModelInfo) -> bool {
        !self.is_downloading(&model.id)
            && !model.downloaded
            && self
                .download_stats
                .get(&model.id)
                .is_some_and(|stats| stats.progress > 0 && stats.progress < 100)
    }

    /// What can be done with `model`, at the right of its card.
    fn render_status(&self, model: &ModelInfo, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = palette(cx);
        let status = div().flex().items_center().gap_1p5().type_xs();

        if model.active {
            return Some(
                status
                    .text_color(p.success)
                    .child(icon("check").size_4())
                    .child(text(t_upper(cx, "settings.modelInUse")).tracking_wider())
                    .into_any_element(),
            );
        }
        if self.is_downloading(&model.id) {
            return None;
        }

        let model_id = model.id.clone();

        if !model.downloaded {
            return Some(
                status
                    .id("download")
                    .transition_colors()
                    .px_3()
                    .py_1p5()
                    .bg(p.ac)
                    .hover(|button| button.bg(p.ac_hover))
                    .text_color(p.ac_on)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.download(model_id.clone(), cx);
                    }))
                    .child(icon("download").size_3p5())
                    .child(text(t_upper(cx, "settings.modelDownload")).tracking_wider())
                    .into_any_element(),
            );
        }

        if model.experimental {
            return Some(
                status
                    .text_color(p.txt_muted)
                    .child(icon("check").size_3p5().text_color(p.success))
                    .child(text(t_upper(cx, "settings.modelDownloaded")).tracking_wider())
                    .into_any_element(),
            );
        }

        let busy = self.busy_id.as_deref() == Some(model.id.as_str());

        Some(
            div()
                .id("use")
                .transition_colors()
                .px_3()
                .py_1p5()
                .border_1()
                .border_color(p.ac)
                .type_xs()
                .text_color(p.ac)
                .hover(|button| button.bg(p.ac).text_color(p.ac_on))
                .map(|button| {
                    if busy {
                        button.opacity(0.5)
                    } else {
                        button.on_click(cx.listener(move |this, _, _, cx| {
                            this.use_model(model_id.clone(), cx);
                        }))
                    }
                })
                .child(text(t_upper(cx, "settings.modelUse")).tracking_wider())
                .into_any_element(),
        )
    }

    fn render_download(&self, download: &Download, cx: &mut Context<Self>) -> Div {
        let p = palette(cx);
        let stats = self.download_stats.get(&download.model_id);
        let progress = stats.map_or(0, |stats| stats.progress);
        let model_id = download.model_id.clone();

        let figures = stats.map(|stats| {
            let mut figures = t_with(
                cx,
                "settings.modelDownloadedOfTotal",
                &[
                    ("downloaded", &format_mb(stats.downloaded_bytes as f64)),
                    ("total", &format_mb(stats.total_bytes as f64)),
                ],
            )
            .to_string();

            if let Some(bytes_per_second) = stats.bytes_per_second {
                figures.push_str(" · ");
                figures.push_str(&t_with(
                    cx,
                    "settings.modelDownloadSpeed",
                    &[("speed", &format_mb(bytes_per_second))],
                ));
            }
            if let Some(eta_seconds) = stats.eta_seconds {
                figures.push_str(" · ");
                figures.push_str(&t_with(
                    cx,
                    "settings.modelEta",
                    &[("eta", &format_eta(eta_seconds))],
                ));
            }

            div()
                .type_px(11.)
                .text_color(p.txt_faint)
                .child(text(figures))
        });

        div()
            .mt_3()
            .flex()
            .flex_col()
            .gap_2()
            .child(progress_bar("download-progress", &download.bar, px(8.), cx))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .type_xs()
                            .text_color(p.txt_secondary)
                            .child(spinning("download-spinner", icon("loader").size_3p5()))
                            .child(text(format!(
                                "{progress}% — {}",
                                t(cx, "settings.modelDownloading")
                            ))),
                    )
                    .child(
                        div()
                            .id("cancel")
                            .transition_colors()
                            .px_2()
                            .py_1()
                            .border_1()
                            .border_color(p.border_strong)
                            .type_px(10.)
                            .text_color(p.txt_secondary)
                            .hover(|button| button.border_color(p.ac).text_color(p.ac))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.cancel_download(model_id.clone(), cx);
                            }))
                            .child(text(t_upper(cx, "settings.cancel")).tracking_wider()),
                    ),
            )
            .children(figures)
    }

    fn render_model(&self, model: &ModelInfo, cx: &mut Context<Self>) -> AnyElement {
        let p = palette(cx);

        let status = self.render_status(model, cx);
        let download = self
            .downloading
            .as_ref()
            .filter(|download| download.model_id == model.id)
            .map(|download| self.render_download(download, cx));

        div()
            .id(SharedString::from(model.id.clone()))
            .border_1()
            .p_4()
            .map(|card| {
                if model.active {
                    card.border_color(p.ac).bg(p.ac_bg)
                } else {
                    card.border_color(p.border_strong).bg(p.surface)
                }
            })
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .min_w_0()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .flex_wrap()
                                    .child(div().type_sm().text_color(p.txt_primary).child(
                                        text(upper(&model.name)).tracking_wider().truncate(),
                                    ))
                                    .when(model.recommended, |row| {
                                        row.child(badge(
                                            t_upper(cx, "settings.modelRecommended"),
                                            p.ac,
                                            p.ac,
                                        ))
                                    })
                                    .when(model.experimental, |row| {
                                        row.child(badge(
                                            t_upper(cx, "settings.modelExperimental"),
                                            p.txt_muted,
                                            p.border_strong,
                                        ))
                                    }),
                            )
                            .child(div().mt_1p5().type_xs().text_color(p.txt_faint).child(text(
                                format!("{} · {}", model.size_label, model.languages),
                            ))),
                    )
                    .child(div().flex_shrink_0().children(status)),
            )
            .children(download)
            .when(self.has_partial_download(model), |card| {
                card.child(note(t(cx, "settings.modelResumeNote"), p.txt_faint))
            })
            .when(model.experimental, |card| {
                card.child(note(t(cx, "settings.modelExperimentalNote"), p.txt_faint))
            })
            .into_any_element()
    }
}

/// A small framed label after a model's name.
fn badge(label: SharedString, color: Hsla, border: Hsla) -> Div {
    div()
        .px_1p5()
        .py_0p5()
        .border_1()
        .border_color(border)
        .type_px(10.)
        .text_color(color)
        .child(text(label).tracking_wider())
}

/// A remark under a model's details.
fn note(message: SharedString, color: Hsla) -> Div {
    div()
        .mt_3()
        .type_px(11.)
        .leading_relaxed(11.)
        .text_color(color)
        .child(text(message))
}

impl Render for ModelSettings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let models: Vec<_> = self
            .models
            .iter()
            .map(|model| self.render_model(model, cx))
            .collect();

        let error = self.error.clone().map(|error| {
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
                        .child(icon("alert-circle").size_4())
                        .child(text(t(cx, "settings.modelDownloadFailed"))),
                )
                .when(!error.is_empty(), |block| {
                    block.child(
                        div()
                            .type_px(11.)
                            .text_color(p.txt_faint)
                            .child(text(error).break_all()),
                    )
                })
        });

        div()
            .flex()
            .flex_col()
            .gap_6()
            .child(
                div()
                    .type_xs()
                    .text_color(p.txt_faint)
                    .child(text(t(cx, "settings.modelsDescription"))),
            )
            // With no models the list is an empty block, whose margin
            // collapses into the next one's: it takes no gap of its own.
            .when(!models.is_empty(), |section| {
                section.child(div().flex().flex_col().gap_3().children(models))
            })
            .children(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn sample(at: Instant, bytes: u64) -> Option<Sample> {
        Some(Sample { at, bytes })
    }

    #[test]
    fn the_first_sample_gives_no_speed() {
        assert_eq!(smoothed_speed(None, None, Instant::now(), 1000), None);
    }

    #[test]
    fn the_second_sample_gives_the_rate_since_the_first() {
        let start = Instant::now();
        let now = start + Duration::from_millis(500);

        assert_eq!(
            smoothed_speed(None, sample(start, 1000), now, 3000),
            Some(4000.)
        );
    }

    #[test]
    fn later_rates_are_smoothed() {
        let start = Instant::now();
        let now = start + Duration::from_secs(1);

        assert_eq!(
            smoothed_speed(Some(1000.), sample(start, 0), now, 2000),
            Some(1000. * 0.7 + 2000. * 0.3)
        );
    }

    #[test]
    fn a_sample_that_cannot_give_a_rate_keeps_the_speed() {
        let start = Instant::now();

        // No time has passed.
        assert_eq!(
            smoothed_speed(Some(5.), sample(start, 0), start, 100),
            Some(5.)
        );
        // The download started over with fewer bytes.
        let later = start + Duration::from_secs(1);
        assert_eq!(
            smoothed_speed(Some(5.), sample(start, 900), later, 100),
            Some(5.)
        );
    }

    #[test]
    fn time_left_needs_a_rate_and_bytes_to_go() {
        assert_eq!(eta_seconds(Some(100.), 200, 1000), Some(8.));
        assert_eq!(eta_seconds(None, 200, 1000), None);
        assert_eq!(eta_seconds(Some(0.), 200, 1000), None);
        assert_eq!(eta_seconds(Some(100.), 1000, 1000), None);
        assert_eq!(eta_seconds(Some(100.), 1200, 1000), None);
    }
}
