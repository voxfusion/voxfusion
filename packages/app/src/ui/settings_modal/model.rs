//! The Models section: the speech-to-text models, which one is in use,
//! downloading the others, and removing downloaded files to reclaim space.

use gpui_kit::{
    AnyElement, App, Context, Div, Hsla, InteractiveElement as _, IntoElement, KeyDownEvent,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, Window, div, prelude::*, px,
};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::analytics;
use crate::backend::{
    self, AppEvent, DOWNLOAD_CANCELLED_ERROR, DOWNLOAD_IN_PROGRESS_ERROR, ModelDownloadProgress,
    ModelInfo, command_error,
};
use crate::events;
use crate::ui::download_format::{SizeUnit, format_eta, format_mb, format_storage};
use crate::ui::motion::Transitions as _;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::focus::{focus_ring, focus_visible, keyed_focus_handle};
use crate::ui::widgets::model_summary::model_summary;
use crate::ui::widgets::progress_bar::{Progress, progress_bar};
use crate::ui::widgets::{icon, spinning};
use crate::ui::{t, t_upper, t_with};

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

/// A download whose progress is shown on its model card.
struct Download {
    bar: Progress,
    /// Every byte is in and the file is being checked against its checksum.
    verifying: bool,
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

#[derive(Clone)]
struct ModelError {
    title_key: &'static str,
    detail: SharedString,
}

struct RemovedNotice {
    model_name: SharedString,
    freed_bytes: u64,
}

fn format_bytes(bytes: u64, cx: &App) -> SharedString {
    let (value, unit) = format_storage(bytes);
    let key = match unit {
        SizeUnit::Bytes => "settings.sizeBytes",
        SizeUnit::Kilobytes => "settings.sizeKb",
        SizeUnit::Megabytes => "settings.sizeMb",
        SizeUnit::Gigabytes => "settings.sizeGb",
    };
    t_with(cx, key, &[("value", &value)])
}

fn model_storage_label(model: &ModelInfo, cx: &App) -> SharedString {
    match model.disk_size_bytes {
        Some(bytes) => format_bytes(bytes, cx),
        None => model.size_label.clone().into(),
    }
}

fn model_has_files(model: &ModelInfo) -> bool {
    model.downloaded || model.has_files || model.disk_size_bytes.unwrap_or(0) > 0
}

pub(super) struct ModelSettings {
    models: Vec<ModelInfo>,
    /// Incremented on each `list_models` request so older responses finishing
    /// out of order cannot overwrite fresher state.
    refresh_generation: u64,
    download_stats: HashMap<String, DownloadStats>,
    downloading: HashMap<String, Download>,
    /// The model being made the active one.
    busy_id: Option<String>,
    /// The model waiting for removal confirmation.
    confirming_remove: Option<String>,
    /// The model whose files are currently being deleted.
    removing_id: Option<String>,
    /// Feedback shown after removing a model so the user sees space freed.
    removed_notice: Option<RemovedNotice>,
    /// Moves focus into the removal confirmation's Cancel button once it mounts.
    pending_focus_cancel: Option<String>,
    /// Moves focus back to a card's Remove button after cancelling confirmation.
    pending_focus_remove: Option<String>,
    error: Option<ModelError>,
    last_samples: HashMap<String, Sample>,
    cancel_requested: HashSet<String>,
    _subscription: Subscription,
}

impl ModelSettings {
    pub(super) fn new(cx: &mut Context<Self>) -> Self {
        let subscription = cx.subscribe(&events::hub(cx), |this, _, event, cx| match event {
            AppEvent::ModelDownloadProgress(progress) => this.handle_progress(progress, cx),
            AppEvent::ModelVerifying { model_id } => this.handle_verifying(model_id, cx),
            AppEvent::ModelDownloadStopped { model_id } => this.handle_download_stopped(model_id, cx),
            AppEvent::ModelRemoved { model_id, .. } => this.handle_removed(model_id, cx),
            AppEvent::ActiveModelChanged => this.refresh(cx),
            _ => {}
        });

        let mut section = Self {
            models: Vec::new(),
            refresh_generation: 0,
            download_stats: HashMap::new(),
            downloading: HashMap::new(),
            busy_id: None,
            confirming_remove: None,
            removing_id: None,
            removed_notice: None,
            pending_focus_cancel: None,
            pending_focus_remove: None,
            error: None,
            last_samples: HashMap::new(),
            cancel_requested: HashSet::new(),
            _subscription: subscription,
        };
        section.refresh(cx);
        section
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.refresh_generation = self.refresh_generation.wrapping_add(1);
        let generation = self.refresh_generation;
        let models = backend::call(cx, |backend| backend.list_models());

        cx.spawn(async move |this, cx| {
            let models = models.await;

            this.update(cx, |this, cx| {
                if generation != this.refresh_generation {
                    return;
                }
                match models {
                    Ok(models) => {
                        for model in &models {
                            if model.downloading && !model.downloaded {
                                this.show_download(&model.id);
                            }
                        }
                        if let Some(id) = this.confirming_remove.as_deref()
                            && !models.iter().any(|model| {
                                model.id == id
                                    && !model.active
                                    && !model.downloading
                                    && model_has_files(model)
                            })
                        {
                            this.confirming_remove = None;
                        }
                        this.models = models;
                    }
                    Err(error) => {
                        this.error = Some(ModelError {
                            title_key: "settings.modelDownloadFailed",
                            detail: command_error("list_models", &error).into(),
                        });
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn is_downloading(&self, model_id: &str) -> bool {
        self.downloading.contains_key(model_id)
            || self
                .models
                .iter()
                .any(|model| model.id == model_id && model.downloading)
    }

    /// Shows the progress of `model_id`'s download, starting the bar at the
    /// progress known so far without resetting an already-visible bar.
    fn show_download(&mut self, model_id: &str) {
        let progress = self
            .download_stats
            .get(model_id)
            .map_or(0, |stats| stats.progress);

        self.downloading
            .entry(model_id.to_string())
            .or_insert_with(|| Download {
                bar: Progress::new(progress as f32),
                verifying: false,
            });
    }

    fn hide_download(&mut self, model_id: &str) {
        self.downloading.remove(model_id);
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
            if let Some(model) = self.models.iter_mut().find(|model| model.id == *model_id) {
                model.downloading = false;
            }
            self.refresh(cx);
        } else {
            if self.confirming_remove.as_deref() == Some(model_id.as_str()) {
                self.confirming_remove = None;
            }
            // A download may already be running, started before this section
            // was opened: show it alongside any other active downloads.
            self.show_download(model_id);
        }

        if let Some(download) = self.downloading.get_mut(model_id) {
            download.bar.set(progress.progress as f32);
            download.verifying = false;
        }
        cx.notify();
    }

    fn handle_verifying(&mut self, model_id: &str, cx: &mut Context<Self>) {
        if self.confirming_remove.as_deref() == Some(model_id) {
            self.confirming_remove = None;
        }
        // A download may be verifying already, started before this section
        // was opened: show it alongside any other active downloads.
        self.show_download(model_id);

        if let Some(download) = self.downloading.get_mut(model_id) {
            // The transfer is over: the bar is full while the file is
            // checked. The progress stays where the transfer left it, so a
            // cancelled check still notes that the download resumes.
            download.bar.set(100.);
            download.verifying = true;
        }
        cx.notify();
    }

    fn handle_download_stopped(&mut self, model_id: &str, cx: &mut Context<Self>) {
        self.hide_download(model_id);
        if let Some(model) = self.models.iter_mut().find(|model| model.id == model_id) {
            model.downloading = false;
        }
        self.refresh(cx);
        cx.notify();
    }

    fn handle_removed(&mut self, model_id: &str, cx: &mut Context<Self>) {
        self.download_stats.remove(model_id);
        self.last_samples.remove(model_id);
        if self.confirming_remove.as_deref() == Some(model_id) {
            self.confirming_remove = None;
        }
        self.refresh(cx);
        cx.notify();
    }

    fn download(&mut self, model_id: String, cx: &mut Context<Self>) {
        if self.removing_id.as_deref() == Some(model_id.as_str()) {
            return;
        }
        self.error = None;
        self.removed_notice = None;
        if self.confirming_remove.as_deref() == Some(model_id.as_str()) {
            self.confirming_remove = None;
        }
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
                        this.error = Some(ModelError {
                            title_key: "settings.modelDownloadFailed",
                            detail: message.into(),
                        });
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
                this.error = Some(ModelError {
                    title_key: "settings.modelDownloadFailed",
                    detail: command_error("cancel_model_download", &error).into(),
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn use_model(&mut self, model_id: String, cx: &mut Context<Self>) {
        if self.removing_id.as_deref() == Some(model_id.as_str()) {
            return;
        }
        self.error = None;
        self.removed_notice = None;
        if self.confirming_remove.as_deref() == Some(model_id.as_str()) {
            self.confirming_remove = None;
        }
        self.busy_id = Some(model_id.clone());
        cx.notify();

        let activate = backend::call(cx, {
            let model_id = model_id.clone();
            move |backend| backend.set_active_model(&model_id)
        });

        cx.spawn(async move |this, cx| {
            let result = activate.await;

            // The Style page and this section both listen for this, even when
            // this view has been closed in the meantime.
            if result.is_ok() {
                cx.update(|cx| events::emit(cx, AppEvent::ActiveModelChanged));
            }

            this.update(cx, |this, cx| {
                this.busy_id = None;
                cx.notify();

                if let Err(error) = result {
                    this.error = Some(ModelError {
                        title_key: "settings.modelDownloadFailed",
                        detail: command_error("set_active_model", &error).into(),
                    });
                    return;
                }

                analytics::capture(cx, "settings_model_changed", &[("model", model_id.into())]);
            })
            .ok();
        })
        .detach();
    }

    fn request_remove(&mut self, model_id: String, cx: &mut Context<Self>) {
        if self.removing_id.is_some()
            || self.busy_id.as_deref() == Some(model_id.as_str())
            || self.is_downloading(&model_id)
        {
            return;
        }
        self.error = None;
        self.removed_notice = None;
        self.confirming_remove = Some(model_id.clone());
        self.pending_focus_cancel = Some(model_id);
        cx.notify();
    }

    fn cancel_remove(&mut self, model_id: String, cx: &mut Context<Self>) {
        if self.removing_id.is_some() {
            return;
        }
        self.confirming_remove = None;
        self.pending_focus_remove = Some(model_id);
        cx.notify();
    }

    fn confirm_remove(&mut self, model_id: String, cx: &mut Context<Self>) {
        if self.removing_id.is_some()
            || self.busy_id.as_deref() == Some(model_id.as_str())
            || self.is_downloading(&model_id)
        {
            return;
        }

        let model_name: SharedString = self
            .models
            .iter()
            .find(|model| model.id == model_id)
            .map(|model| model.name.clone())
            .unwrap_or_else(|| model_id.clone())
            .into();

        self.error = None;
        self.removed_notice = None;
        self.removing_id = Some(model_id.clone());
        cx.notify();

        let request = backend::call(cx, {
            let model_id = model_id.clone();
            move |backend| backend.remove_model(&model_id)
        });

        cx.spawn(async move |this, cx| {
            let result = request.await;

            // Other views (such as onboarding) and this section listen for
            // ModelRemoved, even if this modal closed while deleting.
            if let Ok(freed_bytes) = result {
                cx.update(|cx| {
                    events::emit(
                        cx,
                        AppEvent::ModelRemoved {
                            model_id: model_id.clone(),
                            freed_bytes,
                        },
                    );
                });
            }

            this.update(cx, |this, cx| {
                this.removing_id = None;
                if this.confirming_remove.as_deref() == Some(model_id.as_str()) {
                    this.confirming_remove = None;
                }

                match result {
                    Ok(freed_bytes) => {
                        this.download_stats.remove(&model_id);
                        this.last_samples.remove(&model_id);
                        this.removed_notice = Some(RemovedNotice {
                            model_name,
                            freed_bytes,
                        });
                        analytics::capture(
                            cx,
                            "settings_model_removed",
                            &[("model", model_id.into())],
                        );
                    }
                    Err(error) => {
                        log::error!(
                            target: "settings",
                            "remove_model_failed model_id={model_id} error={error}"
                        );
                        this.error = Some(ModelError {
                            title_key: "settings.modelRemoveFailed",
                            detail: command_error("remove_model", &error).into(),
                        });
                        this.refresh(cx);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Whether part of `model` was downloaded by an attempt that then stopped.
    fn has_partial_download(&self, model: &ModelInfo) -> bool {
        !self.is_downloading(&model.id)
            && !model.downloaded
            && (model_has_files(model)
                || self
                    .download_stats
                    .get(&model.id)
                    .is_some_and(|stats| stats.progress > 0 && stats.progress < 100))
    }

    /// What can be done with `model`, at the right of its card.
    fn render_status(
        &self,
        model: &ModelInfo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
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
        let removing = self.removing_id.as_deref() == Some(model.id.as_str());

        if !model.downloaded {
            let download_id = SharedString::from(format!("download-{}", model.id));
            let focus_handle = keyed_focus_handle(download_id.clone(), window, cx);
            return Some(
                status
                    .id(download_id)
                    .transition_colors()
                    .when(!removing, |button| button.track_focus(&focus_handle))
                    .relative()
                    .px_3()
                    .py_1p5()
                    .bg(p.ac)
                    .text_color(p.ac_on)
                    .map(|button| {
                        if removing {
                            button.opacity(0.5)
                        } else {
                            button
                                .hover(|button| button.bg(p.ac_hover))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.download(model_id.clone(), cx);
                                }))
                        }
                    })
                    .child(icon("download").size_3p5())
                    .child(text(t_upper(cx, "settings.modelDownload")).tracking_wider())
                    .when(!removing && focus_visible(&focus_handle, window), |button| {
                        button.child(focus_ring(4., cx))
                    })
                    .into_any_element(),
            );
        }

        if model.experimental {
            return Some(
                status
                    .text_color(p.txt_secondary)
                    .child(icon("check").size_3p5().text_color(p.success))
                    .child(text(t_upper(cx, "settings.modelDownloaded")).tracking_wider())
                    .into_any_element(),
            );
        }

        let busy = self.busy_id.as_deref() == Some(model.id.as_str()) || removing;
        let use_id = SharedString::from(format!("use-{}", model.id));
        let focus_handle = keyed_focus_handle(use_id.clone(), window, cx);

        Some(
            div()
                .id(use_id)
                .transition_colors()
                .when(!busy, |button| button.track_focus(&focus_handle))
                .relative()
                .px_3()
                .py_1p5()
                .border_1()
                .border_color(p.ac)
                .type_xs()
                .text_color(p.ac)
                .map(|button| {
                    if busy {
                        button.opacity(0.5)
                    } else {
                        button
                            .hover(|button| button.bg(p.ac).text_color(p.ac_on))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.use_model(model_id.clone(), cx);
                            }))
                    }
                })
                .child(text(t_upper(cx, "settings.modelUse")).tracking_wider())
                .when(!busy && focus_visible(&focus_handle, window), |button| {
                    button.child(focus_ring(4., cx))
                })
                .into_any_element(),
        )
    }

    fn render_download(
        &self,
        model_id: &str,
        download: &Download,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = palette(cx);
        let stats = self.download_stats.get(model_id);
        let progress = stats.map_or(0, |stats| stats.progress);
        let cancel_id = SharedString::from(format!("cancel-download-{model_id}"));
        let spinner_id = SharedString::from(format!("download-spinner-{model_id}"));
        let model_id = model_id.to_string();
        let cancel_focus = keyed_focus_handle(cancel_id.clone(), window, cx);

        let figures = stats.map(|stats| {
            // A file being checked has every byte, and the speed and the time
            // left of its transfer are stale.
            let downloaded_bytes = if download.verifying {
                stats.total_bytes
            } else {
                stats.downloaded_bytes
            };
            let mut figures = t_with(
                cx,
                "settings.modelDownloadedOfTotal",
                &[
                    ("downloaded", &format_mb(downloaded_bytes as f64)),
                    ("total", &format_mb(stats.total_bytes as f64)),
                ],
            )
            .to_string();

            if !download.verifying {
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
            }

            div()
                .type_px(11.)
                .text_color(p.txt_hint)
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
                            .child(spinning(spinner_id, icon("loader").size_3p5()))
                            .child(text(if download.verifying {
                                t(cx, "settings.modelVerifying")
                            } else {
                                format!("{progress}% — {}", t(cx, "settings.modelDownloading"))
                                    .into()
                            })),
                    )
                    .child(
                        div()
                            .id(cancel_id)
                            .transition_colors()
                            .track_focus(&cancel_focus)
                            .relative()
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
                            .child(text(t_upper(cx, "settings.cancel")).tracking_wider())
                            .when(focus_visible(&cancel_focus, window), |button| {
                                button.child(focus_ring(4., cx))
                            }),
                    ),
            )
            .children(figures)
    }

    /// Storage readout, Remove control, or inline removal confirmation under a
    /// model card whenever the model has files on disk.
    fn render_storage(
        &self,
        model: &ModelInfo,
        focus_cancel: bool,
        focus_remove: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        if self.is_downloading(&model.id) || !model_has_files(model) {
            return None;
        }

        let p = palette(cx);
        let size = model_storage_label(model, cx);

        if model.active {
            let has_other_downloaded = self.models.iter().any(|other| {
                other.id != model.id
                    && other.downloaded
                    && other.available
                    && !other.experimental
            });
            let has_other_selectable = self
                .models
                .iter()
                .any(|other| other.id != model.id && other.available && !other.experimental);
            let explanation_key = if has_other_downloaded {
                "settings.modelRemoveActiveNote"
            } else if has_other_selectable {
                "settings.modelRemoveDownloadOtherNote"
            } else {
                "settings.modelRemoveUnavailableNote"
            };
            let explanation = t_with(cx, explanation_key, &[("size", &size)]);

            return Some(
                div()
                    .mt_3()
                    .pt_3()
                    .border_t_1()
                    .border_color(p.border)
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .type_px(11.)
                            .leading_relaxed(11.)
                            .text_color(p.txt_hint)
                            .child(text(explanation)),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .px_2p5()
                            .py_1()
                            .border_1()
                            .border_color(p.border_strong)
                            .type_xs()
                            .text_color(p.txt_hint)
                            .opacity(0.45)
                            .child(icon("trash-2").size_3p5())
                            .child(text(t_upper(cx, "settings.modelRemove")).tracking_wider()),
                    ),
            );
        }

        if self.confirming_remove.as_deref() == Some(model.id.as_str()) {
            let removing = self.removing_id.as_deref() == Some(model.id.as_str());
            let cancel_id = SharedString::from(format!("cancel-remove-{}", model.id));
            let confirm_id = SharedString::from(format!("confirm-remove-{}", model.id));
            let cancel_focus = keyed_focus_handle(cancel_id.clone(), window, cx);
            let confirm_focus = keyed_focus_handle(confirm_id.clone(), window, cx);

            if focus_cancel {
                window.focus(&cancel_focus, cx);
            }

            let cancel_model_id = model.id.clone();
            let escape_model_id = model.id.clone();
            let confirm_model_id = model.id.clone();

            return Some(
                div()
                    .mt_3()
                    .p_3()
                    .border_1()
                    .border_color(p.ac)
                    .bg(p.ac_bg)
                    .type_xs()
                    .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                        if event.keystroke.key == "escape" && this.removing_id.is_none() {
                            this.cancel_remove(escape_model_id.clone(), cx);
                            cx.stop_propagation();
                        }
                    }))
                    .child(
                        div()
                            .mb_3()
                            .leading_relaxed(12.)
                            .text_color(p.txt_primary)
                            .child(text(t_with(
                                cx,
                                "settings.modelRemoveConfirm",
                                &[("name", &model.name), ("size", &size)],
                            ))),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                div()
                                    .id(cancel_id)
                                    .transition_colors()
                                    .when(!removing, |button| button.track_focus(&cancel_focus))
                                    .relative()
                                    .px_3()
                                    .py_1p5()
                                    .border_1()
                                    .border_color(p.border_strong)
                                    .text_color(p.txt_secondary)
                                    .when(!removing, |button| {
                                        button
                                            .hover(|button| {
                                                button
                                                    .border_color(p.txt_muted)
                                                    .text_color(p.txt_primary)
                                            })
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.cancel_remove(cancel_model_id.clone(), cx);
                                            }))
                                    })
                                    .when(removing, |button| button.opacity(0.5))
                                    .child(text(t_upper(cx, "settings.cancel")).tracking_wider())
                                    .when(
                                        !removing && focus_visible(&cancel_focus, window),
                                        |button| button.child(focus_ring(4., cx)),
                                    ),
                            )
                            .child(
                                div()
                                    .id(confirm_id)
                                    .transition_colors()
                                    .when(!removing, |button| button.track_focus(&confirm_focus))
                                    .relative()
                                    .flex()
                                    .items_center()
                                    .gap_1p5()
                                    .px_3()
                                    .py_1p5()
                                    .border_1()
                                    .border_color(p.ac)
                                    .bg(p.ac)
                                    .text_color(p.ac_on)
                                    .map(|button| {
                                        if removing {
                                            button.opacity(0.5).child(spinning(
                                                "removing-model-spinner",
                                                icon("loader").size_3p5(),
                                            ))
                                        } else {
                                            button
                                                .hover(|button| {
                                                    button.bg(p.ac_hover).border_color(p.ac_hover)
                                                })
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.confirm_remove(
                                                        confirm_model_id.clone(),
                                                        cx,
                                                    );
                                                }))
                                        }
                                    })
                                    .child(
                                        text(t_upper(cx, "settings.modelRemove")).tracking_wider(),
                                    )
                                    .when(
                                        !removing && focus_visible(&confirm_focus, window),
                                        |button| button.child(focus_ring(4., cx)),
                                    ),
                            ),
                    ),
            );
        }

        let disabled =
            self.busy_id.as_deref() == Some(model.id.as_str()) || self.removing_id.is_some();
        let remove_id = SharedString::from(format!("remove-model-{}", model.id));
        let remove_focus = keyed_focus_handle(remove_id.clone(), window, cx);
        if focus_remove && !disabled {
            window.focus(&remove_focus, cx);
        }
        let model_id = model.id.clone();

        Some(
            div()
                .mt_3()
                .pt_3()
                .border_t_1()
                .border_color(p.border)
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .type_px(11.)
                        .leading_relaxed(11.)
                        .text_color(p.txt_hint)
                        .child(text(t_with(
                            cx,
                            "settings.modelRemoveFrees",
                            &[("size", &size)],
                        ))),
                )
                .child(
                    div()
                        .id(remove_id)
                        .transition_colors()
                        .when(!disabled, |button| button.track_focus(&remove_focus))
                        .relative()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .px_2p5()
                        .py_1()
                        .border_1()
                        .border_color(p.border_strong)
                        .type_xs()
                        .text_color(p.txt_secondary)
                        .map(|button| {
                            if disabled {
                                button.opacity(0.5)
                            } else {
                                button
                                    .hover(|button| button.border_color(p.ac).text_color(p.ac))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.request_remove(model_id.clone(), cx);
                                    }))
                            }
                        })
                        .child(icon("trash-2").size_3p5())
                        .child(text(t_upper(cx, "settings.modelRemove")).tracking_wider())
                        .when(!disabled && focus_visible(&remove_focus, window), |button| {
                            button.child(focus_ring(4., cx))
                        }),
                ),
        )
    }

    fn render_model(
        &self,
        model: &ModelInfo,
        focus_cancel: bool,
        focus_remove: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = palette(cx);

        let status = self.render_status(model, window, cx);
        let download = self
            .downloading
            .get(&model.id)
            .map(|download| self.render_download(&model.id, download, window, cx));
        let storage = self.render_storage(model, focus_cancel, focus_remove, window, cx);

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
                    .child(model_summary(model, cx))
                    .child(div().flex_shrink_0().children(status)),
            )
            .children(download)
            .when(self.has_partial_download(model), |card| {
                card.child(note(t(cx, "settings.modelResumeNote"), p.txt_hint))
            })
            .when(model.experimental, |card| {
                card.child(note(t(cx, "settings.modelExperimentalNote"), p.txt_hint))
            })
            .children(storage)
            .into_any_element()
    }
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let focus_cancel_id = self.pending_focus_cancel.take();
        let focus_remove_id = self.pending_focus_remove.take();

        let models: Vec<_> = self
            .models
            .iter()
            .map(|model| {
                let focus_cancel = focus_cancel_id.as_deref() == Some(model.id.as_str());
                let focus_remove = focus_remove_id.as_deref() == Some(model.id.as_str());
                self.render_model(model, focus_cancel, focus_remove, window, cx)
            })
            .collect();

        let removed = self.error.is_none().then(|| {
            self.removed_notice.as_ref().map(|notice| {
                let message = if notice.freed_bytes > 0 {
                    let size = format_bytes(notice.freed_bytes, cx);
                    t_with(
                        cx,
                        "settings.modelRemovedFreed",
                        &[("name", notice.model_name.as_ref()), ("size", &size)],
                    )
                } else {
                    t_with(
                        cx,
                        "settings.modelRemoved",
                        &[("name", notice.model_name.as_ref())],
                    )
                };

                div()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .type_xs()
                    .text_color(p.success)
                    .child(icon("check").size_3p5())
                    .child(text(message))
            })
        });

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
                        .child(text(t(cx, error.title_key))),
                )
                .when(!error.detail.is_empty(), |block| {
                    block.child(
                        div()
                            .type_px(11.)
                            .text_color(p.txt_hint)
                            .child(text(error.detail).break_all()),
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
                    .text_color(p.txt_hint)
                    .child(text(t(cx, "settings.modelsDescription"))),
            )
            // With no models the list is an empty block, whose margin
            // collapses into the next one's: it takes no gap of its own.
            .when(!models.is_empty(), |section| {
                section.child(div().flex().flex_col().gap_3().children(models))
            })
            .children(removed.flatten())
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
