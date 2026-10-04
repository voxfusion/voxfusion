//! The left column of the settings: the list of sections and the app version.

use gpui_kit::{
    App, ClipboardItem, Context, Div, Global, InteractiveElement as _, ParentElement as _,
    SharedString, Stateful, StatefulInteractiveElement as _, Styled as _, div, prelude::*, px,
};
use std::time::Duration;

use super::{Section, SettingsModal};
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::icon;
use crate::ui::{t_upper, upper};

/// How long the check mark stays after the version was copied.
const COPIED_MARK: Duration = Duration::from_millis(1500);

struct AppVersion(SharedString);

impl Global for AppVersion {}

/// Makes the settings show `version` instead of the version of this build,
/// so screenshots do not change with every release.
#[cfg(feature = "fixture")]
pub fn set_app_version(version: impl Into<SharedString>, cx: &mut App) {
    cx.set_global(AppVersion(version.into()));
}

fn app_version(cx: &App) -> SharedString {
    cx.try_global::<AppVersion>()
        .map_or(env!("CARGO_PKG_VERSION").into(), |version| {
            version.0.clone()
        })
}

impl Section {
    fn number(self) -> &'static str {
        match self {
            Section::Audio => "01",
            Section::Model => "02",
            Section::Hotkey => "03",
            Section::Appearance => "04",
            Section::Language => "05",
            Section::Privacy => "06",
        }
    }

    fn label_key(self) -> &'static str {
        match self {
            Section::Audio => "settings.audio",
            Section::Model => "settings.models",
            Section::Hotkey => "settings.hotkeys",
            Section::Appearance => "settings.appearance",
            Section::Language => "settings.language",
            Section::Privacy => "sidebar.privacy",
        }
    }
}

impl SettingsModal {
    fn copy_version(&mut self, cx: &mut Context<Self>) {
        if self.version_copied {
            return;
        }

        cx.write_to_clipboard(ClipboardItem::new_string(app_version(cx).to_string()));
        self.version_copied = true;
        cx.notify();

        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COPIED_MARK).await;
            this.update(cx, |this, cx| {
                this.version_copied = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn render_section_item(&self, section: Section, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = palette(cx);
        let active = self.section == section;

        div()
            .id(section.number())
            .flex()
            .items_center()
            .gap_3()
            .w_full()
            .px_4()
            .py_3()
            .type_xs()
            .border_l_2()
            .map(|item| {
                if active {
                    item.text_color(p.ac).border_color(p.ac).bg(p.surface)
                } else {
                    item.text_color(p.txt_muted)
                        .border_color(gpui_kit::transparent_black())
                        .hover(|item| item.text_color(p.txt_secondary).bg(p.surface))
                }
            })
            .on_click(cx.listener(move |this, _, _, cx| this.show_section(section, cx)))
            .child(
                div()
                    .text_color(if active { p.ac } else { p.txt_faint })
                    .child(text(section.number()).tracking_wider()),
            )
            .child(text(t_upper(cx, section.label_key())).tracking_wider())
    }

    pub(super) fn render_sidebar(&self, cx: &mut Context<Self>) -> Div {
        let p = palette(cx);
        let version = app_version(cx);
        let title = format!("[VOXFUSION] > {}", crate::ui::t(cx, "settings.title"));

        let sections = Section::ALL.map(|section| self.render_section_item(section, cx));

        // The button is as wide as its text, which a row makes it.
        let version_row = (!version.is_empty()).then(|| {
            div()
                .flex()
                .px_4()
                .py_3()
                .border_t_1()
                .border_color(p.border)
                .child(
                    div()
                        .id("copy-version")
                        .group("copy-version")
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .type_px(10.)
                        .text_color(p.txt_faint)
                        .hover(|button| button.text_color(p.txt_muted))
                        .on_click(cx.listener(|this, _, _, cx| this.copy_version(cx)))
                        .child(text(format!("v{version}")))
                        .child(if self.version_copied {
                            icon("check").size_3().text_color(p.ac).into_any_element()
                        } else {
                            div()
                                .opacity(0.)
                                .group_hover("copy-version", |icon| icon.opacity(1.))
                                .child(icon("copy").size_3())
                                .into_any_element()
                        }),
                )
        });

        div()
            .w(px(224.))
            .bg(p.base)
            .border_r_1()
            .border_color(p.border)
            .flex()
            .flex_col()
            .child(
                div()
                    .px_4()
                    .py_4()
                    .border_b_1()
                    .border_color(p.border)
                    .child(
                        div()
                            .type_sm()
                            .text_color(p.ac)
                            .child(text(upper(&title)).tracking_wider()),
                    ),
            )
            .child(div().flex_1().py_2().children(sections))
            .children(version_row)
    }
}
