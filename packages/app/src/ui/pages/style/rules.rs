//! The styles of the apps or of the sites: one row for each, with the
//! dropdown that changes its style and the button that removes it.

use gpui_kit::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _, Subscription, Task,
    Window, div, px,
};
use std::collections::HashMap;
use std::rc::Rc;

use crate::analytics;
use crate::backend::{self, AppInstruction, Backend, CommandResult, SiteStyle};
use crate::ui::motion::Transitions as _;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::app_icon::{AppIcons, app_icon, site_icon};
use crate::ui::widgets::style_select::{StyleSelect, StyleSelected};

/// The style a newly added app or site starts with.
const INITIAL_STYLE: &str = "default";

/// Whose styles these are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Apps,
    Sites,
}

/// The style of one app or one site.
#[derive(Debug, Clone, PartialEq)]
struct Rule {
    id: String,
    /// The app's bundle id or the site's domain.
    key: String,
    /// An app's name. Sites go by their domain.
    name: Option<String>,
    style: String,
}

impl From<AppInstruction> for Rule {
    fn from(instruction: AppInstruction) -> Self {
        Self {
            id: instruction.id,
            key: instruction.bundle_id,
            name: Some(instruction.app_name),
            style: instruction.style,
        }
    }
}

impl From<SiteStyle> for Rule {
    fn from(site: SiteStyle) -> Self {
        Self {
            id: site.id,
            key: site.domain,
            name: None,
            style: site.style,
        }
    }
}

impl Scope {
    fn list(self, backend: &dyn Backend) -> CommandResult<Vec<Rule>> {
        fn rules<T: Into<Rule>>(stored: Vec<T>) -> Vec<Rule> {
            stored.into_iter().map(Into::into).collect()
        }

        match self {
            Scope::Apps => backend.list_app_instructions().map(rules),
            Scope::Sites => backend.list_site_styles().map(rules),
        }
    }

    fn set(
        self,
        backend: &dyn Backend,
        key: &str,
        name: Option<&str>,
        style: &str,
    ) -> CommandResult<()> {
        match self {
            Scope::Apps => backend
                .set_app_instruction(key, name.unwrap_or_default(), style)
                .map(|_| ()),
            Scope::Sites => backend.set_site_style(key, style).map(|_| ()),
        }
    }

    fn delete(self, backend: &dyn Backend, id: &str) -> CommandResult<()> {
        match self {
            Scope::Apps => backend.delete_app_instruction(id),
            Scope::Sites => backend.delete_site_style(id),
        }
    }

    fn added_event(self) -> &'static str {
        match self {
            Scope::Apps => "app_instruction_added",
            Scope::Sites => "site_style_added",
        }
    }

    fn changed_event(self) -> &'static str {
        match self {
            Scope::Apps => "app_instruction_style_changed",
            Scope::Sites => "site_style_changed",
        }
    }

    fn deleted_event(self) -> &'static str {
        match self {
            Scope::Apps => "app_instruction_deleted",
            Scope::Sites => "site_style_deleted",
        }
    }
}

struct RowSelect {
    select: Entity<StyleSelect>,
    _subscription: Subscription,
}

pub struct StyleRules {
    scope: Scope,
    rules: Vec<Rule>,
    /// The dropdown of each rule's row, by the rule's id.
    selects: HashMap<String, RowSelect>,
    icons: Rc<AppIcons>,
    focus_handle: FocusHandle,
}

impl StyleRules {
    pub fn new(scope: Scope, cx: &mut Context<Self>) -> Self {
        Self {
            scope,
            rules: Vec::new(),
            selects: HashMap::new(),
            icons: Rc::default(),
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// The bundle ids or domains that have a style.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.rules.iter().map(|rule| rule.key.as_str())
    }

    /// The icons the apps' rows show.
    pub fn set_icons(&mut self, icons: Rc<AppIcons>, cx: &mut Context<Self>) {
        self.icons = icons;
        cx.notify();
    }

    /// Shows `rules`, keeping the dropdowns of the rows that stay.
    fn set_rules(&mut self, rules: Vec<Rule>, window: &mut Window, cx: &mut Context<Self>) {
        let mut selects = HashMap::new();

        for rule in &rules {
            let row = match self.selects.remove(&rule.id) {
                Some(row) => {
                    row.select
                        .update(cx, |select, cx| select.set_value(rule.style.clone(), cx));
                    row
                }
                None => {
                    let select = cx.new(|cx| StyleSelect::new(rule.style.clone(), cx));
                    let subscription = cx.subscribe_in(&select, window, {
                        let id = rule.id.clone();
                        move |this, _, StyleSelected(style), window, cx| {
                            this.change_style(&id, style, window, cx);
                        }
                    });

                    RowSelect {
                        select,
                        _subscription: subscription,
                    }
                }
            };

            selects.insert(rule.id.clone(), row);
        }

        self.selects = selects;
        self.rules = rules;
        cx.notify();
    }

    /// Fetches the stored styles.
    pub fn fetch(&self, window: &Window, cx: &mut Context<Self>) -> Task<()> {
        let scope = self.scope;
        let request = backend::call(cx, move |backend| scope.list(backend));

        cx.spawn_in(window, async move |this, cx| {
            if let Ok(rules) = request.await {
                this.update_in(cx, |this, window, cx| this.set_rules(rules, window, cx))
                    .ok();
            }
        })
    }

    /// Gives an app or a site the default style, unless it has one already.
    /// Answers whether it was added.
    pub fn add(
        &mut self,
        key: String,
        name: Option<String>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Task<bool> {
        if self.keys().any(|existing| existing == key) {
            return Task::ready(false);
        }

        let scope = self.scope;
        let request = backend::call(cx, move |backend| {
            scope.set(backend, &key, name.as_deref(), INITIAL_STYLE)
        });

        cx.spawn_in(window, async move |this, cx| {
            if request.await.is_err() {
                return false;
            }

            let fetch = this.update_in(cx, |this, window, cx| {
                analytics::capture(cx, scope.added_event(), &[("style", INITIAL_STYLE.into())]);
                this.fetch(window, cx)
            });
            if let Ok(fetch) = fetch {
                fetch.await;
            }

            true
        })
    }

    fn change_style(
        &mut self,
        id: &str,
        style: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(rule) = self.rules.iter_mut().find(|rule| rule.id == id) else {
            return;
        };
        rule.style = style.into();

        let scope = self.scope;
        let (key, name) = (rule.key.clone(), rule.name.clone());
        let request = backend::call(cx, move |backend| {
            scope.set(backend, &key, name.as_deref(), style)
        });

        if let Some(row) = self.selects.get(id) {
            row.select
                .update(cx, |select, cx| select.set_value(style, cx));
        }
        // A changed row is laid out anew, and its dropdown does not keep the
        // keyboard focus.
        window.focus(&self.focus_handle, cx);
        cx.notify();

        cx.spawn_in(window, async move |this, cx| match request.await {
            Ok(()) => {
                this.update(cx, |_, cx| {
                    analytics::capture(cx, scope.changed_event(), &[("style", style.into())]);
                })
                .ok();
            }
            Err(_) => {
                if let Ok(fetch) = this.update_in(cx, |this, window, cx| this.fetch(window, cx)) {
                    fetch.await;
                }
            }
        })
        .detach();
    }

    fn delete(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let scope = self.scope;
        analytics::capture(cx, scope.deleted_event(), &[]);

        self.rules.retain(|rule| rule.id != id);
        self.selects.remove(id);
        cx.notify();

        let id = id.to_string();
        let request = backend::call(cx, move |backend| scope.delete(backend, &id));

        cx.spawn_in(window, async move |this, cx| {
            if request.await.is_err()
                && let Ok(fetch) = this.update_in(cx, |this, window, cx| this.fetch(window, cx))
            {
                fetch.await;
            }
        })
        .detach();
    }

    fn render_row(&self, rule: &Rule, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let p = palette(cx);
        let select = self.selects.get(&rule.id)?.select.clone();
        // The dropdown's list is part of the row, though it hangs outside it.
        let hovered = select.read(cx).list_hovered();

        let (picture, title, subtitle) = match &rule.name {
            Some(name) => (
                app_icon(self.icons.get(&rule.key), name, cx).into_any_element(),
                name.clone(),
                Some(rule.key.clone()),
            ),
            None => (
                site_icon(Some(&rule.key), px(32.), cx).into_any_element(),
                rule.key.clone(),
                None,
            ),
        };

        Some(
            div()
                .id(rule.id.clone())
                .group("style-rule")
                .transition_colors()
                .px_4()
                .py_3()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .bg(p.surface)
                .border_1()
                .border_color(if hovered { p.border_strong } else { p.border })
                .hover(|row| row.border_color(p.border_strong))
                .child(picture)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .flex_1()
                        .child(
                            div()
                                .text_color(p.txt_primary)
                                .child(text(title).truncate()),
                        )
                        .children(subtitle.map(|subtitle| {
                            div()
                                .type_xs()
                                .text_color(p.txt_muted)
                                .child(text(subtitle).truncate())
                        })),
                )
                .child(select)
                .child(
                    div()
                        .id("remove")
                        .transition_all()
                        .type_xs()
                        .text_right()
                        .text_color(p.txt_muted)
                        .opacity(if hovered { 1. } else { 0. })
                        .group_hover("style-rule", |button| button.opacity(1.))
                        .hover(|button| button.text_color(p.ac))
                        .on_click(cx.listener({
                            let id = rule.id.clone();
                            move |this, _, window, cx| this.delete(&id, window, cx)
                        }))
                        .child(text("[DEL]").tracking_wider()),
                ),
        )
    }
}

impl Render for StyleRules {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .gap_1()
            .children(
                self.rules
                    .iter()
                    .filter_map(|rule| self.render_row(rule, cx)),
            )
    }
}
