//! The Style page: the transcription style used everywhere, and the styles
//! of single apps and sites.

mod default;
mod per_app;
mod per_site;
mod rules;

use gpui_kit::{
    AppContext as _, Context, Div, Entity, IntoElement, ParentElement as _, Render, SharedString,
    Styled as _, Subscription, Task, Window, div,
};

use crate::backend::{self, AppEvent, ModelInfo};
use crate::events;
use crate::ui::main_window::SectionTab;
use crate::ui::pages::section::{Section, section_frame};
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::icon;
use crate::ui::{t, t_with};

use default::DefaultStyle;
use per_app::AppStyles;
use per_site::SiteStyles;

/// Whether the active speech-to-text model applies transcription styles.
/// Styles are prompts, which only Whisper takes.
#[derive(Debug, Clone, PartialEq, Eq)]
enum StyleSupport {
    /// The models have not been listed yet, listing them failed, or none is active.
    Unknown,
    /// A Whisper model is active, so style choices take effect.
    Supported,
    /// A non-Whisper model is active and ignores styles, carrying its display name.
    Ignored(SharedString),
}

impl StyleSupport {
    fn is_supported(&self) -> Option<bool> {
        match self {
            StyleSupport::Unknown => None,
            StyleSupport::Supported => Some(true),
            StyleSupport::Ignored(_) => Some(false),
        }
    }
}

fn model_display_name(model: &ModelInfo) -> SharedString {
    let name = model.name.trim();
    if !name.is_empty() {
        name.to_string().into()
    } else if !model.engine.trim().is_empty() {
        model.engine.trim().to_string().into()
    } else {
        model.id.trim().to_string().into()
    }
}

fn style_support_for_models(models: &[ModelInfo]) -> StyleSupport {
    match models.iter().find(|model| model.active) {
        Some(model) if model.engine == "whisper" => StyleSupport::Supported,
        Some(model) => StyleSupport::Ignored(model_display_name(model)),
        None => StyleSupport::Unknown,
    }
}

enum Content {
    Default(Entity<DefaultStyle>),
    PerApp(Entity<AppStyles>),
    Sites(Entity<SiteStyles>),
}

impl Content {
    fn new(
        tab: SectionTab,
        styles_supported: Option<bool>,
        window: &mut Window,
        cx: &mut Context<StylePage>,
    ) -> Self {
        match tab {
            SectionTab::Default => {
                Content::Default(cx.new(|cx| DefaultStyle::new(styles_supported, cx)))
            }
            SectionTab::PerApp => Content::PerApp(cx.new(|cx| AppStyles::new(window, cx))),
            SectionTab::Sites => Content::Sites(cx.new(|cx| SiteStyles::new(window, cx))),
        }
    }
}

pub struct StylePage {
    tab: SectionTab,
    content: Content,
    style_support: StyleSupport,
    /// Replaces any earlier in-flight model check so stale responses cannot win.
    model_check: Option<Task<()>>,
    _subscription: Subscription,
}

impl StylePage {
    pub fn new(tab: SectionTab, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let subscription = cx.subscribe(&events::hub(cx), |this, _, event, cx| {
            if let AppEvent::ActiveModelChanged = event {
                this.check_model(cx);
            }
        });

        let mut page = Self {
            tab,
            content: Content::new(tab, None, window, cx),
            style_support: StyleSupport::Unknown,
            model_check: None,
            _subscription: subscription,
        };
        page.check_model(cx);
        page
    }

    fn check_model(&mut self, cx: &mut Context<Self>) {
        let models = backend::call(cx, |backend| backend.list_models());

        self.model_check = Some(cx.spawn(async move |this, cx| {
            let support = match models.await {
                Ok(models) => style_support_for_models(&models),
                Err(error) => {
                    log::error!(target: "style", "list_models_failed error={error}");
                    StyleSupport::Unknown
                }
            };

            this.update(cx, |this, cx| {
                this.model_check = None;
                this.style_support = support;
                if let Content::Default(default) = &this.content {
                    default.update(cx, |default, cx| {
                        default.set_styles_supported(this.style_support.is_supported(), cx);
                    });
                }
                cx.notify();
            })
            .ok();
        }));
    }

    /// Opens `tab`. Its content starts fresh every time it is opened.
    pub fn set_tab(&mut self, tab: SectionTab, window: &mut Window, cx: &mut Context<Self>) {
        if tab == self.tab {
            return;
        }

        self.tab = tab;
        self.content = Content::new(tab, self.style_support.is_supported(), window, cx);
        if self.style_support == StyleSupport::Unknown && self.model_check.is_none() {
            self.check_model(cx);
        }
        cx.notify();
    }

    /// Shown across all Style tabs when the active model ignores style prompts.
    fn render_model_warning(&self, cx: &Context<Self>) -> Option<Div> {
        let StyleSupport::Ignored(model_name) = &self.style_support else {
            return None;
        };

        let p = palette(cx);
        let headline = (!model_name.is_empty()).then(|| {
            t_with(
                cx,
                "style.modelIgnoresStyles",
                &[("model", model_name.as_ref())],
            )
        });
        let detail_color = if headline.is_some() {
            p.txt_hint
        } else {
            p.txt_primary
        };

        Some(
            div()
                .mb_6()
                .p_4()
                .bg(p.ac_bg)
                .border_1()
                .border_color(p.ac)
                .flex()
                .items_start()
                .gap_3()
                .type_xs()
                .child(icon("alert-circle").size_4().text_color(p.ac))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .min_w_0()
                        .flex_1()
                        .children(
                            headline.map(|headline| {
                                div().text_color(p.txt_primary).child(text(headline))
                            }),
                        )
                        .child(
                            div()
                                .text_color(detail_color)
                                .child(text(t(cx, "settings.modelStylesWhisperOnly"))),
                        ),
                ),
        )
    }
}

impl Render for StylePage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tab_content = match &self.content {
            Content::Default(content) => content.clone().into_any_element(),
            Content::PerApp(content) => content.clone().into_any_element(),
            Content::Sites(content) => content.clone().into_any_element(),
        };

        let content = div()
            .children(self.render_model_warning(cx))
            .child(tab_content);

        section_frame(Section::Style, self.tab, content, window, cx)
    }
}
