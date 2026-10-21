//! The default style: the one used wherever no app or site has its own.

use gpui_kit::{
    Context, Div, InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, Window, div, prelude::*,
};
use std::time::Duration;

use crate::analytics;
use crate::backend::{self, AppEvent};
use crate::events;
use crate::settings::{STYLE_LIST, SettingsStore};
use crate::ui::motion::Transitions as _;
use crate::ui::pages::section::list_header;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::icon;
use crate::ui::widgets::style_select::style_label;
use crate::ui::{t, t_upper, upper};

/// How long the confirmation shows after the style changes.
const SAVED_FOR: Duration = Duration::from_secs(2);

fn style_description(style: &str, cx: &Context<DefaultStyle>) -> SharedString {
    match style {
        "professional" => t(cx, "style.descriptions.professional"),
        "casual" => t(cx, "style.descriptions.casual"),
        "agents" => t(cx, "style.descriptions.agents"),
        _ => t(cx, "style.descriptions.default"),
    }
}

pub struct DefaultStyle {
    /// Whether the model in use is Parakeet, which ignores styles. Unknown
    /// until the models have been listed.
    parakeet: Option<bool>,
    /// Hides the confirmation that the style changed. Set while it shows.
    saved: Option<Task<()>>,
    _subscriptions: [Subscription; 2],
}

impl DefaultStyle {
    pub fn new(cx: &mut Context<Self>) -> Self {
        analytics::capture(cx, "$pageview", &[("$current_url", "/style".into())]);

        let subscriptions = [
            cx.observe(&SettingsStore::entity(cx), |_, _, cx| cx.notify()),
            cx.subscribe(&events::hub(cx), |this, _, event, cx| {
                if let AppEvent::ActiveModelChanged = event {
                    this.check_model(cx);
                }
            }),
        ];

        let mut page = Self {
            parakeet: None,
            saved: None,
            _subscriptions: subscriptions,
        };
        page.check_model(cx);
        page
    }

    fn check_model(&mut self, cx: &mut Context<Self>) {
        let models = backend::call(cx, |backend| backend.list_models());

        cx.spawn(async move |this, cx| {
            let parakeet = models.await.is_ok_and(|models| {
                models
                    .iter()
                    .any(|model| model.active && model.engine == "parakeet")
            });

            this.update(cx, |this, cx| {
                this.parakeet = Some(parakeet);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn select(&mut self, style: &'static str, cx: &mut Context<Self>) {
        if SettingsStore::get(cx).default_style == style {
            return;
        }

        let saved = SettingsStore::update(cx, |settings| settings.default_style = style.into());
        analytics::capture(cx, "default_style_changed", &[("style", style.into())]);

        // A style that could not be written to disk is in use, but not saved.
        if !saved {
            self.saved = None;
            cx.notify();
            return;
        }

        // Replacing the task starts the time over.
        self.saved = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVED_FOR).await;
            this.update(cx, |this, cx| {
                this.saved = None;
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// The line under the description: that the style is in use, or that it
    /// was just saved. Its height does not change with what it says.
    fn render_status(&self, cx: &Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let status = div().mt_3().h_5().flex().items_center().gap_1p5();
        let confirmed = |status: Div, key| {
            status
                .text_color(p.success)
                .child(icon("check").size_3p5())
                .child(text(t_upper(cx, key)).tracking_wider())
        };

        if self.saved.is_some() {
            confirmed(status, "style.saved")
        } else {
            match self.parakeet {
                Some(false) => confirmed(status, "style.inUse"),
                Some(true) => status
                    .text_color(p.txt_muted)
                    .child(text(t(cx, "style.parakeetIgnoresStyles"))),
                None => status,
            }
        }
    }
}

impl Render for DefaultStyle {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let current = SettingsStore::get(cx).default_style.clone();

        let tabs = STYLE_LIST.map(|style| {
            div()
                .id(style)
                .transition_colors()
                .flex_1()
                .px_4()
                .py_2p5()
                .border_b_2()
                .type_xs()
                .map(|tab| {
                    if style == current {
                        tab.text_color(p.ac).border_color(p.ac).bg(p.hover)
                    } else {
                        tab.text_color(p.txt_secondary)
                            .border_color(gpui_kit::transparent_black())
                            .hover(|tab| tab.text_color(p.txt_primary).bg(p.hover))
                    }
                })
                .on_click(cx.listener(move |this, _, _, cx| this.select(style, cx)))
                .child(
                    text(upper(&style_label(style, cx)))
                        .tracking_wider()
                        .center(),
                )
        });

        div()
            .child(list_header(
                Some(t(cx, "style.defaultStyleDescription")),
                None,
                cx,
            ))
            .child(
                div()
                    .bg(p.surface)
                    .border_1()
                    .border_color(p.border)
                    .child(
                        div()
                            .flex()
                            .border_b_1()
                            .border_color(p.border)
                            .children(tabs),
                    )
                    .child(
                        div()
                            .p_4()
                            .type_xs()
                            .child(
                                div()
                                    .leading_relaxed(12.)
                                    .text_color(p.txt_secondary)
                                    .child(text(style_description(&current, cx))),
                            )
                            .child(self.render_status(cx)),
                    ),
            )
    }
}
