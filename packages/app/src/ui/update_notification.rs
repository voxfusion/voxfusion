//! The offer of an available update, shown in the sidebar, and its download.

use gpui_kit::{
    Context, Div, Empty, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, Window, div, px,
};
use std::time::{Duration, Instant};

use crate::backend::{self, AppEvent, UpdateDownloadEvent, UpdateInfo};
use crate::events;
use crate::ui::download_format::format_mb;
use crate::ui::motion::Transitions as _;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::icon;
use crate::ui::widgets::progress_bar::{Progress, progress_bar};
use crate::ui::{t, t_upper};

const UPDATE_CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

pub struct UpdateNotification {
    update: Option<UpdateInfo>,
    visible: bool,
    checking: bool,
    /// The version the user chose to ignore, until the app is restarted.
    dismissed_version: Option<String>,
    downloading: bool,
    downloaded_bytes: u64,
    content_length: Option<u64>,
    bar: Progress,
    _subscription: Subscription,
    _periodic_check: Task<()>,
}

impl UpdateNotification {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let subscription = cx.subscribe(&events::hub(cx), |this, _, event, cx| {
            if matches!(event, AppEvent::CheckForUpdates) {
                this.check_for_updates(cx);
            }
        });

        let periodic_check = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(UPDATE_CHECK_INTERVAL).await;

                let checked = this.update(cx, |this, cx| this.check_for_updates(cx));
                if checked.is_err() {
                    break;
                }
            }
        });

        let mut notification = Self {
            update: None,
            visible: false,
            checking: false,
            dismissed_version: None,
            downloading: false,
            downloaded_bytes: 0,
            content_length: None,
            bar: Progress::new(0.),
            _subscription: subscription,
            _periodic_check: periodic_check,
        };
        notification.check_for_updates(cx);
        notification
    }

    fn check_for_updates(&mut self, cx: &mut Context<Self>) {
        if self.checking || self.downloading {
            log::debug!(
                target: "updater",
                "check_skipped is_checking={} is_downloading={}",
                self.checking,
                self.downloading
            );
            return;
        }

        self.checking = true;
        let started_at = Instant::now();
        log::debug!(target: "updater", "check_started");

        let check = backend::call_updater(cx, |updater| updater.check());
        cx.spawn(async move |this, cx| {
            let available = check.await;
            let elapsed_ms = started_at.elapsed().as_millis();

            this.update(cx, |this, cx| {
                this.checking = false;

                match available {
                    Ok(Some(update)) => {
                        if this.dismissed_version.as_deref() == Some(update.version.as_str()) {
                            log::debug!(
                                target: "updater",
                                "update_dismissed_for_session version={}",
                                update.version
                            );
                            return;
                        }

                        log::info!(
                            target: "updater",
                            "update_available version={} elapsed_ms={elapsed_ms}",
                            update.version
                        );
                        this.update = Some(update);
                        this.visible = true;
                    }
                    Ok(None) => {
                        log::debug!(target: "updater", "no_update elapsed_ms={elapsed_ms}");
                        this.update = None;
                        this.visible = false;
                    }
                    Err(error) => {
                        log::error!(
                            target: "updater",
                            "check_failed elapsed_ms={elapsed_ms} error={error}"
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        self.dismissed_version = self.update.as_ref().map(|update| update.version.clone());
        log::info!(
            target: "updater",
            "update_ignored version={}",
            self.dismissed_version.as_deref().unwrap_or_default()
        );
        self.visible = false;
        cx.notify();
    }

    fn download_and_restart(&mut self, cx: &mut Context<Self>) {
        let Some(update) = self.update.clone() else {
            return;
        };

        log::info!(
            target: "updater",
            "download_and_restart_requested version={}",
            update.version
        );
        self.downloading = true;
        self.downloaded_bytes = 0;
        self.content_length = None;
        self.bar = Progress::new(0.);
        cx.notify();

        // The updater reports from its own thread until the install is over,
        // which closes the channel.
        let (progress, events) = async_channel::unbounded();
        let install = backend::call_updater(cx, {
            let update = update.clone();
            move |updater| {
                updater.download_and_install(&update, &mut |event| {
                    let _ = progress.send_blocking(event);
                })
            }
        });

        cx.spawn(async move |this, cx| {
            while let Ok(event) = events.recv().await {
                this.update(cx, |this, cx| {
                    this.handle_download_event(&update, event, cx)
                })
                .ok();
            }

            match install.await {
                Ok(()) => {
                    log::warn!(target: "updater", "relaunch_requested version={}", update.version);
                    cx.update(|cx| backend::call_updater(cx, |updater| updater.relaunch()))
                        .await;
                }
                Err(error) => {
                    log::error!(
                        target: "updater",
                        "download_or_install_failed version={} error={error}",
                        update.version
                    );
                    this.update(cx, |this, cx| {
                        this.downloading = false;
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    fn handle_download_event(
        &mut self,
        update: &UpdateInfo,
        event: UpdateDownloadEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            UpdateDownloadEvent::Started { content_length } => {
                log::info!(
                    target: "updater",
                    "download_started version={} content_length={content_length:?}",
                    update.version
                );
                self.downloaded_bytes = 0;
                self.content_length = content_length;
            }
            UpdateDownloadEvent::Progress { chunk_length } => {
                self.downloaded_bytes += chunk_length;
            }
            UpdateDownloadEvent::Finished => {
                log::info!(
                    target: "updater",
                    "download_finished version={} downloaded_bytes={}",
                    update.version,
                    self.downloaded_bytes
                );
                if let Some(total) = self.known_length() {
                    self.downloaded_bytes = total;
                }
            }
        }

        self.bar.set(self.download_percent() as f32);
        cx.notify();
    }

    /// The size of the download, when the server told it.
    fn known_length(&self) -> Option<u64> {
        self.content_length.filter(|length| *length > 0)
    }

    fn download_percent(&self) -> f64 {
        download_percent(self.downloaded_bytes, self.known_length())
    }

    fn render_download(&self, cx: &mut Context<Self>) -> Div {
        let p = palette(cx);

        let figures = self.known_length().map(|total| {
            div()
                .mt_0p5()
                .type_px(10.)
                .text_color(p.txt_muted)
                .child(text(format!(
                    "{} / {} MB ({}%)",
                    format_mb(self.downloaded_bytes as f64),
                    format_mb(total as f64),
                    self.download_percent().round()
                )))
        });

        div()
            .child(progress_bar("update-progress", &self.bar, px(4.), cx))
            .child(
                div()
                    .mt_1p5()
                    .type_px(10.)
                    .text_color(p.txt_muted)
                    .child(text(t_upper(cx, "update.downloading")).tracking_wider()),
            )
            .children(figures)
    }
}

/// How much of the download is there, in percent. Unknown while the size is.
fn download_percent(downloaded_bytes: u64, content_length: Option<u64>) -> f64 {
    match content_length {
        Some(total) => (downloaded_bytes as f64 / total as f64 * 100.).min(100.),
        None => 0.,
    }
}

impl Render for UpdateNotification {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(update) = self.update.as_ref().filter(|_| self.visible) else {
            return Empty.into_any_element();
        };
        let p = palette(cx);

        let ignore = (!self.downloading).then(|| {
            div()
                .id("ignore-update")
                .transition_colors()
                .type_px(10.)
                .text_color(p.txt_muted)
                .hover(|button| button.text_color(p.txt_primary))
                .on_click(cx.listener(|this, _, _, cx| this.dismiss(cx)))
                .child(text(t_upper(cx, "update.ignore")).tracking_wider())
        });

        let notes = update
            .body
            .clone()
            .filter(|body| !body.is_empty())
            .map(|body| {
                div()
                    .id("update-notes")
                    .mb_2()
                    .max_h_24()
                    .overflow_y_scroll()
                    .border_1()
                    .border_color(p.border)
                    .bg(p.surface)
                    .p_2()
                    .child(
                        div()
                            .type_px(10.)
                            .leading_relaxed(10.)
                            .text_color(p.txt_secondary)
                            .child(text(body).pre_wrap()),
                    )
            });

        let action = if self.downloading {
            self.render_download(cx).into_any_element()
        } else {
            div()
                .id("install-update")
                .transition_colors()
                .w_full()
                .flex()
                .items_center()
                .justify_center()
                .gap_1p5()
                .px_2()
                .py_1p5()
                .type_px(10.)
                .border_1()
                .border_color(p.ac)
                .text_color(p.ac)
                .hover(|button| button.bg(p.ac).text_color(p.ac_on))
                .on_click(cx.listener(|this, _, _, cx| this.download_and_restart(cx)))
                .child(text("→").tracking_wider())
                // A button centers its text, which shows once the label wraps.
                .child(
                    text(t_upper(cx, "update.downloadAndRestart"))
                        .tracking_wider()
                        .center(),
                )
                .into_any_element()
        };

        div()
            .border_t_1()
            .border_color(p.border)
            .font_mono()
            .child(
                div()
                    .px_6()
                    .py_3()
                    .child(
                        div()
                            .mb_1p5()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1p5()
                                    .type_px(10.)
                                    .text_color(p.ac)
                                    .child(icon("download").size(px(10.)))
                                    .child(text(t_upper(cx, "update.available")).tracking_wider()),
                            )
                            .children(ignore),
                    )
                    .child(
                        div()
                            .mb_2()
                            .type_xs()
                            .text_color(p.txt_primary)
                            .child(text(format!(
                                "{} {}",
                                t(cx, "update.newVersion"),
                                update.version
                            ))),
                    )
                    .children(notes)
                    .child(action),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::download_percent;

    #[test]
    fn the_share_downloaded_stops_at_the_whole() {
        assert_eq!(download_percent(0, Some(200)), 0.);
        assert_eq!(download_percent(50, Some(200)), 25.);
        assert_eq!(download_percent(300, Some(200)), 100.);
    }

    #[test]
    fn there_is_no_share_of_an_unknown_size() {
        assert_eq!(download_percent(50, None), 0.);
    }
}
