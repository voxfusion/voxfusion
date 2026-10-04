//! The default style: the one used wherever no app or site has its own.

use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, div, prelude::*,
};

use crate::analytics;
use crate::settings::{STYLE_LIST, SettingsStore};
use crate::ui::pages::section::list_header;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::style_select::style_label;
use crate::ui::{t, upper};

fn style_description(style: &str, cx: &Context<DefaultStyle>) -> SharedString {
    match style {
        "professional" => t(cx, "style.descriptions.professional"),
        "casual" => t(cx, "style.descriptions.casual"),
        "agents" => t(cx, "style.descriptions.agents"),
        _ => t(cx, "style.descriptions.default"),
    }
}

pub struct DefaultStyle {
    _subscription: Subscription,
}

impl DefaultStyle {
    pub fn new(cx: &mut Context<Self>) -> Self {
        analytics::capture(cx, "$pageview", &[("$current_url", "/style".into())]);

        Self {
            _subscription: cx.observe(&SettingsStore::entity(cx), |_, _, cx| cx.notify()),
        }
    }

    fn select(&mut self, style: &'static str, cx: &mut Context<Self>) {
        SettingsStore::update(cx, |settings| settings.default_style = style.into());
        analytics::capture(cx, "default_style_changed", &[("style", style.into())]);
    }
}

impl Render for DefaultStyle {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let current = SettingsStore::get(cx).default_style.clone();

        let tabs = STYLE_LIST.map(|style| {
            div()
                .id(style)
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
                            .leading_relaxed(12.)
                            .text_color(p.txt_secondary)
                            .child(text(style_description(&current, cx))),
                    ),
            )
    }
}
