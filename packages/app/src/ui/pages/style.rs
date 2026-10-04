//! The Style page: the transcription style used everywhere, and the styles
//! of single apps and sites.

mod default;
mod per_app;
mod per_site;
mod rules;

use gpui_kit::{AppContext as _, Context, Entity, IntoElement, Render, Window};

use crate::ui::main_window::SectionTab;
use crate::ui::pages::section::{Section, section_frame};

use default::DefaultStyle;
use per_app::AppStyles;
use per_site::SiteStyles;

enum Content {
    Default(Entity<DefaultStyle>),
    PerApp(Entity<AppStyles>),
    Sites(Entity<SiteStyles>),
}

impl Content {
    fn new(tab: SectionTab, window: &mut Window, cx: &mut Context<StylePage>) -> Self {
        match tab {
            SectionTab::Default => Content::Default(cx.new(DefaultStyle::new)),
            SectionTab::PerApp => Content::PerApp(cx.new(|cx| AppStyles::new(window, cx))),
            SectionTab::Sites => Content::Sites(cx.new(|cx| SiteStyles::new(window, cx))),
        }
    }
}

pub struct StylePage {
    tab: SectionTab,
    content: Content,
}

impl StylePage {
    pub fn new(tab: SectionTab, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            tab,
            content: Content::new(tab, window, cx),
        }
    }

    /// Opens `tab`. Its content starts fresh every time it is opened.
    pub fn set_tab(&mut self, tab: SectionTab, window: &mut Window, cx: &mut Context<Self>) {
        if tab == self.tab {
            return;
        }

        self.tab = tab;
        self.content = Content::new(tab, window, cx);
        cx.notify();
    }
}

impl Render for StylePage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match &self.content {
            Content::Default(content) => content.clone().into_any_element(),
            Content::PerApp(content) => content.clone().into_any_element(),
            Content::Sites(content) => content.clone().into_any_element(),
        };

        section_frame(Section::Style, self.tab, content, window, cx)
    }
}
