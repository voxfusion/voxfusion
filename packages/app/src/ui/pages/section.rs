//! What the Dictionary and Style pages share: the frame with the title and
//! the three tabs, the line above a list, and the card for an empty list.

use gpui_kit::{
    App, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, prelude::*, px,
};

use crate::ui::main_window::{SectionTab, navigate};
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::{t, t_upper, upper};

const TABS: [SectionTab; 3] = [SectionTab::Default, SectionTab::PerApp, SectionTab::Sites];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Dictionary,
    Style,
}

impl Section {
    fn tag(self) -> &'static str {
        match self {
            Section::Dictionary => "[DICTIONARY]",
            Section::Style => "[STYLE]",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Section::Dictionary => "CUSTOM_TERMS",
            Section::Style => "TRANSCRIPTION_STYLE",
        }
    }

    fn description(self, cx: &App) -> Option<SharedString> {
        match self {
            Section::Dictionary => Some(t_upper(cx, "dictionary.description")),
            Section::Style => None,
        }
    }

    fn path(self, tab: SectionTab) -> &'static str {
        match (self, tab) {
            (Section::Dictionary, SectionTab::Default) => "/dictionary",
            (Section::Dictionary, SectionTab::PerApp) => "/dictionary/per-app",
            (Section::Dictionary, SectionTab::Sites) => "/dictionary/sites",
            (Section::Style, SectionTab::Default) => "/style",
            (Section::Style, SectionTab::PerApp) => "/style/per-app",
            (Section::Style, SectionTab::Sites) => "/style/sites",
        }
    }

    fn tab_label(self, tab: SectionTab, cx: &App) -> SharedString {
        let key = match (self, tab) {
            (Section::Dictionary, SectionTab::Default) => "dictionary.tabDefault",
            (Section::Dictionary, SectionTab::PerApp) => "dictionary.tabPerApp",
            (Section::Dictionary, SectionTab::Sites) => "dictionary.tabSites",
            (Section::Style, SectionTab::Default) => "style.tabDefault",
            (Section::Style, SectionTab::PerApp) => "style.tabPerApp",
            (Section::Style, SectionTab::Sites) => "style.tabSites",
        };

        upper(&format!("[{}]", t(cx, key)))
    }
}

/// The page: its title, the tabs, and the content of the tab that is open.
pub fn section_frame(
    section: Section,
    active: SectionTab,
    content: impl IntoElement,
    window: &Window,
    cx: &App,
) -> impl IntoElement {
    let p = palette(cx);

    let tabs = TABS.map(|tab| {
        let path = section.path(tab);

        div()
            .id(path)
            .py_2()
            .type_xs()
            .map(|item| {
                if tab == active {
                    item.text_color(p.ac)
                } else {
                    item.text_color(p.txt_secondary)
                        .hover(|item| item.text_color(p.txt_primary))
                }
            })
            .on_click(move |_, _, cx| navigate(cx, path))
            .child(text(section.tab_label(tab, cx)).tracking_wider())
    });

    div()
        .min_h(window.viewport_size().height)
        .px_6()
        .py_8()
        .flex()
        .flex_col()
        .items_center()
        .font_mono()
        .child(
            div()
                .w_full()
                .max_w(px(672.))
                .child(
                    div()
                        .mb_6()
                        .child(
                            div().type_sm().child(
                                text(section.tag())
                                    .color(p.ac)
                                    .span(" > ", p.txt_muted)
                                    .span(section.name(), p.txt_secondary),
                            ),
                        )
                        .children(section.description(cx).map(|description| {
                            div()
                                .mt_2()
                                .type_xs()
                                .text_color(p.txt_muted)
                                .child(text(description).tracking_wide())
                        })),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_4()
                        .mb_6()
                        .border_b_1()
                        .border_color(p.border)
                        .children(tabs),
                )
                .child(content),
        )
}

/// The line above a list: what the list is for and how many entries it has.
pub fn list_header(
    description: Option<SharedString>,
    count: Option<SharedString>,
    cx: &App,
) -> impl IntoElement {
    let p = palette(cx);

    div()
        .mb_4()
        .flex()
        .items_center()
        .map(|header| {
            if description.is_some() {
                header.justify_between()
            } else {
                header.justify_end()
            }
        })
        .type_xs()
        .text_color(p.txt_muted)
        .children(description.map(text))
        .children(count.map(|count| text(upper(&count))))
}

/// The card shown in place of an empty list. `code` is the machine-style
/// line, such as `NO_TERMS_FOUND`.
pub fn empty_state(
    code: &str,
    title: SharedString,
    description: SharedString,
    cx: &App,
) -> impl IntoElement {
    let p = palette(cx);

    div()
        .p_12()
        .bg(p.surface)
        .border_1()
        .border_color(p.border)
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .child(
            div().mb_4().type_sm().text_color(p.txt_muted).child(
                text("[INFO]")
                    .color(p.ac)
                    .plain(format!(" {code}"))
                    .center(),
            ),
        )
        .child(
            div()
                .type_xs()
                .text_color(p.txt_secondary)
                .child(text(upper(&title)).tracking_wide().center()),
        )
        .child(
            div()
                .mt_2()
                .max_w(px(384.))
                .type_xs()
                .text_color(p.txt_muted)
                .child(text(description).center()),
        )
}
