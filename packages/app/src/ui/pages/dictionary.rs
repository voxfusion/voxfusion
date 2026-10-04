//! The Dictionary page: custom words for everywhere, per app and per site.

mod default;
mod groups;
mod per_app;
mod sites;

use gpui_kit::{AppContext as _, Context, Entity, IntoElement, Render, Window};

use crate::ui::main_window::SectionTab;
use crate::ui::pages::section::{Section, section_frame};

use default::DefaultDictionary;
use per_app::AppDictionaries;
use sites::SiteDictionaries;

enum Content {
    Default(Entity<DefaultDictionary>),
    PerApp(Entity<AppDictionaries>),
    Sites(Entity<SiteDictionaries>),
}

impl Content {
    fn new(tab: SectionTab, window: &mut Window, cx: &mut Context<DictionaryPage>) -> Self {
        match tab {
            SectionTab::Default => {
                Content::Default(cx.new(|cx| DefaultDictionary::new(window, cx)))
            }
            SectionTab::PerApp => Content::PerApp(cx.new(|cx| AppDictionaries::new(window, cx))),
            SectionTab::Sites => Content::Sites(cx.new(|cx| SiteDictionaries::new(window, cx))),
        }
    }
}

pub struct DictionaryPage {
    tab: SectionTab,
    content: Content,
}

impl DictionaryPage {
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

impl Render for DictionaryPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match &self.content {
            Content::Default(content) => content.clone().into_any_element(),
            Content::PerApp(content) => content.clone().into_any_element(),
            Content::Sites(content) => content.clone().into_any_element(),
        };

        section_frame(Section::Dictionary, self.tab, content, window, cx)
    }
}
