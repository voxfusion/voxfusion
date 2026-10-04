//! The Audio section: which microphone records, and what happens to other
//! sound while it does.

use gpui_kit::{
    Animation, AnimationExt as _, App, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Task, Window,
    div, ease_in_out, prelude::*,
};
use std::time::Duration;

use super::SettingsModal;
use crate::analytics;
use crate::backend::{self, AudioDevice};
use crate::settings::{self, SettingsStore};
use crate::ui::motion::Transitions as _;
use crate::ui::t;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::select::{SelectOption, select};
use crate::ui::widgets::toggle::toggle_option;
use crate::ui::widgets::{animations_frozen, icon};

/// One turn of the refresh icon.
const REFRESH_ANIMATION: Duration = Duration::from_millis(600);

/// The value of the option that follows the system's default input.
const DEFAULT_MICROPHONE: &str = "default";

#[derive(Default)]
pub(super) struct AudioState {
    devices: Vec<AudioDevice>,
    /// Whether the refresh button is making its turn, during which it is off.
    refreshing: bool,
    refresh_timer: Option<Task<()>>,
}

impl AudioState {
    pub(super) fn stop_refresh_animation(&mut self) {
        self.refreshing = false;
        self.refresh_timer = None;
    }
}

/// The input devices as the interface lists them. A failure to list them
/// shows as no devices.
fn audio_input_devices(cx: &App) -> Task<Vec<AudioDevice>> {
    let devices = backend::call(cx, |backend| backend.list_audio_devices());

    cx.background_spawn(async move { devices.await.unwrap_or_default() })
}

/// The system default first, named after the device it currently is, then
/// every other device.
fn microphone_options(devices: &[AudioDevice], default_label: &str) -> Vec<SelectOption> {
    let default_device = devices.iter().find(|device| device.is_default);
    let default_label = match default_device {
        Some(device) => format!("{default_label} ({})", device.name),
        None => default_label.to_string(),
    };

    std::iter::once(SelectOption::new(DEFAULT_MICROPHONE, default_label))
        .chain(
            devices
                .iter()
                .filter(|device| !device.is_default)
                .map(|device| SelectOption::new(device.name.clone(), device.name.clone())),
        )
        .collect()
}

impl SettingsModal {
    pub(super) fn fetch_audio_devices(&mut self, cx: &mut Context<Self>) {
        let devices = audio_input_devices(cx);

        cx.spawn(async move |this, cx| {
            let devices = devices.await;
            this.update(cx, |this, cx| {
                this.audio.devices = devices;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn refresh_devices(&mut self, cx: &mut Context<Self>) {
        if self.audio.refreshing {
            return;
        }

        self.audio.refreshing = true;
        self.fetch_audio_devices(cx);
        cx.notify();

        // A frozen animation never gets to its end.
        if animations_frozen() {
            return;
        }

        self.audio.refresh_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REFRESH_ANIMATION).await;
            this.update(cx, |this, cx| {
                this.audio.refreshing = false;
                cx.notify();
            })
            .ok();
        }));
    }

    pub(super) fn render_audio(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let p = palette(cx);
        let settings = SettingsStore::get(cx).clone();
        let refreshing = self.audio.refreshing;

        let refresh_icon = icon("refresh-cw").size_4();
        let refresh_icon = if refreshing {
            let frozen = animations_frozen();
            refresh_icon
                .with_animation(
                    "refresh-devices",
                    Animation::new(REFRESH_ANIMATION).with_easing(ease_in_out),
                    move |icon, turn| if frozen { icon } else { icon.rotate(turn) },
                )
                .into_any_element()
        } else {
            refresh_icon.into_any_element()
        };

        let microphone = div()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .mb_3()
                    .child(
                        div()
                            .type_xs()
                            .text_color(p.txt_muted)
                            .child(text("INPUT_DEVICE").tracking_wider()),
                    )
                    .child(
                        div()
                            .id("refresh-devices")
                            .transition_colors()
                            .p_1p5()
                            .text_color(p.txt_muted)
                            .hover(|button| button.text_color(p.ac))
                            .map(|button| {
                                if refreshing {
                                    button.opacity(0.5)
                                } else {
                                    button.on_click(
                                        cx.listener(|this, _, _, cx| this.refresh_devices(cx)),
                                    )
                                }
                            })
                            .child(refresh_icon),
                    ),
            )
            .child(select(
                "microphone",
                settings
                    .selected_microphone_id
                    .as_deref()
                    .unwrap_or(DEFAULT_MICROPHONE),
                microphone_options(&self.audio.devices, &t(cx, "settings.defaultMicrophone")),
                |value: &SharedString, _, cx| {
                    analytics::capture(cx, "settings_microphone_changed", &[]);

                    let microphone =
                        (value.as_ref() != DEFAULT_MICROPHONE).then(|| value.to_string());
                    SettingsStore::update(cx, |settings| {
                        settings.selected_microphone_id = microphone;
                    });
                },
                window,
                cx,
            ))
            .child(
                div()
                    .mt_3()
                    .type_xs()
                    .text_color(p.txt_faint)
                    .child(text(t(cx, "settings.microphoneDescription"))),
            );

        div()
            .flex()
            .flex_col()
            .gap_6()
            .child(microphone)
            .child(toggle_option(
                "mute-media",
                &t(cx, "settings.muteMediaWhileRecording"),
                t(cx, "settings.muteMediaWhileRecordingDescription"),
                settings.mute_media_while_recording,
                |enabled, _, cx| {
                    analytics::capture(
                        cx,
                        "settings_mute_media_while_recording_changed",
                        &[("enabled", enabled.into())],
                    );
                    settings::set_mute_media(cx, enabled);
                },
                cx,
            ))
            .child(toggle_option(
                "muffle-media",
                &t(cx, "settings.muffleMediaWhileRecording"),
                t(cx, "settings.muffleMediaWhileRecordingDescription"),
                settings.muffle_media_while_recording,
                |enabled, _, cx| {
                    analytics::capture(
                        cx,
                        "settings_muffle_media_while_recording_changed",
                        &[("enabled", enabled.into())],
                    );
                    settings::set_muffle_media(cx, enabled);

                    // macOS asks for System Audio Recording, which the muffle
                    // filter needs.
                    if enabled {
                        backend::call(cx, |backend| backend.request_muffle_permission()).detach();
                    }
                },
                cx,
            ))
            .child(toggle_option(
                "recording-sounds",
                &t(cx, "settings.recordingSounds"),
                t(cx, "settings.recordingSoundsDescription"),
                settings.recording_sounds_enabled,
                |enabled, _, cx| {
                    analytics::capture(
                        cx,
                        "settings_recording_sounds_changed",
                        &[("enabled", enabled.into())],
                    );
                    SettingsStore::update(cx, |settings| {
                        settings.recording_sounds_enabled = enabled;
                    });
                },
                cx,
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str, is_default: bool) -> AudioDevice {
        AudioDevice {
            name: name.into(),
            is_default,
        }
    }

    #[test]
    fn the_default_option_names_the_default_device() {
        let options = microphone_options(
            &[device("USB Mic", false), device("Built-in", true)],
            "System Default",
        );

        assert_eq!(
            options,
            [
                SelectOption::new("default", "System Default (Built-in)"),
                SelectOption::new("USB Mic", "USB Mic"),
            ]
        );
    }

    #[test]
    fn without_devices_only_the_default_option_is_left() {
        assert_eq!(
            microphone_options(&[], "System Default"),
            [SelectOption::new("default", "System Default")]
        );
    }
}
