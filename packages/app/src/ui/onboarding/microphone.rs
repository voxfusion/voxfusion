//! Step 3: choosing the microphone.

use gpui_kit::{
    Bounds, Context, FocusHandle, InteractiveElement as _, IntoElement, MouseDownEvent,
    ParentElement as _, Pixels, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, Window, canvas, deferred, div, prelude::*, px, relative, transparent_black,
};
use std::cell::Cell;
use std::rc::Rc;

use super::parts::{card, description, icon_box, step_column, title};
use crate::backend::{self, AudioDevice};
use crate::settings::SettingsStore;
use crate::ui::motion::Transitions as _;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::turning;
use crate::ui::widgets::{icon, spinning};
use crate::ui::{t, t_upper};

/// The option that follows the system's default input device.
const DEFAULT_OPTION: &str = "default";
/// How tall the list of options may get, border included, before it scrolls.
const LIST_MAX_HEIGHT: f32 = 240.;

struct MicrophoneOption {
    value: String,
    label: SharedString,
}

pub struct MicrophoneStep {
    devices: Vec<AudioDevice>,
    loading: bool,
    list_open: bool,
    trigger_focus: FocusHandle,
    /// Where the select's button is, to tell a press on it from one outside.
    trigger_bounds: Rc<Cell<Bounds<Pixels>>>,
    _settings: Subscription,
}

impl MicrophoneStep {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut step = Self {
            devices: Vec::new(),
            loading: false,
            list_open: false,
            trigger_focus: cx.focus_handle(),
            trigger_bounds: Rc::default(),
            _settings: cx.observe(&SettingsStore::entity(cx), |_, _, cx| cx.notify()),
        };
        step.fetch_devices(cx);
        step
    }

    fn fetch_devices(&mut self, cx: &mut Context<Self>) {
        self.loading = true;
        cx.notify();

        let devices = backend::call(cx, |backend| backend.list_audio_devices());
        cx.spawn(async move |this, cx| {
            let devices = devices.await.unwrap_or_default();

            this.update(cx, |this, cx| {
                this.devices = devices;
                this.loading = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The system default first, named after the device it currently is, then
    /// every other device.
    fn options(&self, cx: &Context<Self>) -> Vec<MicrophoneOption> {
        let default_label = t(cx, "settings.defaultMicrophone");
        let default_device = self.devices.iter().find(|device| device.is_default);

        let default_option = MicrophoneOption {
            value: DEFAULT_OPTION.into(),
            label: match default_device {
                Some(device) => format!("{default_label} ({})", device.name).into(),
                None => default_label,
            },
        };

        let other_devices = self
            .devices
            .iter()
            .filter(|device| !device.is_default)
            .map(|device| MicrophoneOption {
                value: device.name.clone(),
                label: device.name.clone().into(),
            });

        std::iter::once(default_option)
            .chain(other_devices)
            .collect()
    }

    fn choose(&mut self, value: &str, window: &mut Window, cx: &mut Context<Self>) {
        let microphone = (value != DEFAULT_OPTION).then(|| value.to_string());
        SettingsStore::update(cx, |settings| {
            settings.selected_microphone_id = microphone;
        });

        self.list_open = false;
        // The pressed option took the focus and is gone with the list.
        window.blur(cx);
        cx.notify();
    }

    fn render_refresh(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let p = palette(cx);
        let loading = self.loading;
        let refresh_icon = icon("refresh-cw").size_4();

        let button = div()
            .id("refresh-devices")
            .transition_colors()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_1p5()
            .type_xs()
            .border_1()
            .border_color(transparent_black())
            .text_color(p.txt_muted)
            .hover(|button| button.text_color(p.ac).border_color(p.border_strong))
            .map(|button| {
                if loading {
                    button.opacity(0.5)
                } else {
                    button.on_click(cx.listener(|this, _, _, cx| this.fetch_devices(cx)))
                }
            })
            .child(if loading {
                spinning("refreshing", refresh_icon).into_any_element()
            } else {
                refresh_icon.into_any_element()
            })
            .child(text(t_upper(cx, "onboarding.refreshDevices")).tracking_wider());

        div().flex().items_center().justify_end().child(button)
    }

    fn render_list(
        &self,
        options: &[MicrophoneOption],
        selected: &str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let p = palette(cx);
        let trigger_bounds = self.trigger_bounds.clone();

        // The rows scroll inside the border: a scrolling element that has a
        // border of its own stops short of its end by the border's width.
        let mut rows = div()
            .id("microphone-options")
            .max_h(px(LIST_MAX_HEIGHT - 2.))
            .overflow_y_scroll();

        for (index, option) in options.iter().enumerate() {
            let is_selected = option.value == selected;
            let value = option.value.clone();

            rows = rows.child(
                div()
                    .id(("microphone-option", index))
                    .transition_colors()
                    .flex()
                    .items_center()
                    .justify_between()
                    .w_full()
                    .px_4()
                    .py_3()
                    .type_sm()
                    .map(|row| {
                        if is_selected {
                            row.bg(p.ac_bg).text_color(p.ac)
                        } else {
                            row.text_color(p.txt_primary)
                        }
                    })
                    .hover(|row| row.bg(p.hover))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.choose(&value, window, cx);
                    }))
                    .child(div().min_w_0().child(text(option.label.clone()).truncate()))
                    .when(is_selected, |row| {
                        row.child(icon("check").size_4().text_color(p.ac))
                    }),
            );
        }

        let list = div()
            .absolute()
            .top(relative(1.))
            .left_0()
            .w_full()
            .mt_1()
            .bg(p.surface)
            .border_1()
            .border_color(p.border_strong)
            .occlude()
            .on_mouse_down_out(cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                // A press on the button is left to the button, which closes
                // the list itself.
                if !trigger_bounds.get().contains(&event.position) {
                    this.list_open = false;
                    cx.notify();
                }
            }))
            .child(rows);

        deferred(list)
    }

    fn render_select(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let p = palette(cx);
        let options = self.options(cx);
        let selected = SettingsStore::get(cx)
            .selected_microphone_id
            .clone()
            .unwrap_or_else(|| DEFAULT_OPTION.into());
        let selected_label = options
            .iter()
            .find(|option| option.value == selected)
            .map(|option| option.label.clone())
            .unwrap_or_else(|| t(cx, "onboarding.selectMicrophone"));
        let trigger_bounds = self.trigger_bounds.clone();

        let trigger = div()
            .id("microphone-select")
            .transition_colors()
            .track_focus(&self.trigger_focus)
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .px_4()
            .py_3()
            .type_sm()
            .bg(p.base)
            .border_1()
            .border_color(p.border_strong)
            .text_color(p.txt_primary)
            .hover(|trigger| trigger.border_color(p.ac))
            .focus(|trigger| trigger.border_color(p.ac))
            .on_click(cx.listener(|this, _, _, cx| {
                this.list_open = !this.list_open;
                cx.notify();
            }))
            .child(div().min_w_0().child(text(selected_label).truncate()))
            .child(turning(
                self.list_open,
                icon("chevron-down").size_5().ml_2().text_color(p.txt_muted),
            ));

        div()
            .relative()
            .child(trigger)
            .child(
                canvas(
                    move |bounds, _, _| trigger_bounds.set(bounds),
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .when(self.list_open, |select| {
                select.child(self.render_list(&options, &selected, cx))
            })
    }
}

impl Render for MicrophoneStep {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);

        let content = div()
            .child(icon_box(icon("mic").text_color(p.ac), cx))
            .child(title(&t(cx, "onboarding.microphoneTitle"), cx))
            .child(description(t(cx, "onboarding.microphoneDescription"), cx).mb_8())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(self.render_refresh(cx))
                    .child(self.render_select(cx)),
            );

        step_column("[STEP_03] > MICROPHONE_SELECT", p.ac).child(card(content, cx))
    }
}
