//! The styles of the apps or of the sites: one row for each, with the
//! dropdown that changes its style and the button that removes it.

use gpui_kit::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, Task, Window, div, prelude::*, px,
};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::analytics;
use crate::backend::{self, AppInstruction, Backend, CommandResult, SiteStyle};
use crate::settings::SettingsStore;
use crate::ui::motion::Transitions as _;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::app_icon::{AppIcons, app_icon, site_icon};
use crate::ui::widgets::focus::{focus_ring, focus_visible};
use crate::ui::widgets::icon;
use crate::ui::widgets::style_select::{InheritTarget, StyleSelect, StyleSelected};
use crate::ui::{t, upper};

/// The style key a newly added app or site starts with (the neutral style).
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
    ) -> CommandResult<Rule> {
        match self {
            Scope::Apps => backend
                .set_app_instruction(key, name.unwrap_or_default(), style)
                .map(Rule::from),
            Scope::Sites => backend.set_site_style(key, style).map(Rule::from),
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

    fn inherit_target(self) -> InheritTarget {
        match self {
            Scope::Apps => InheritTarget::GlobalDefault,
            Scope::Sites => InheritTarget::AppOrGlobalDefault,
        }
    }
}

struct RowControls {
    select: Entity<StyleSelect>,
    remove_focus: FocusHandle,
    _subscription: Subscription,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuleError {
    title: SharedString,
    detail: SharedString,
}

pub struct StyleRules {
    scope: Scope,
    rules: Vec<Rule>,
    /// The controls of each rule's row, by the rule's id.
    rows: HashMap<String, RowControls>,
    /// Rules with a save or delete currently running in the backend.
    pending: HashSet<String>,
    /// Keys (bundle ids or domains) whose add request is currently running.
    adding: HashSet<String>,
    /// Last write or delete failure for a rule, shown inline so the user can retry.
    errors: HashMap<String, RuleError>,
    /// Counts completed local mutations so an older fetch cannot overwrite them.
    generation: u64,
    icons: Rc<AppIcons>,
    focus_handle: FocusHandle,
    _settings: Subscription,
}

impl StyleRules {
    pub fn new(scope: Scope, cx: &mut Context<Self>) -> Self {
        let settings = cx.observe(&SettingsStore::entity(cx), |_, _, cx| cx.notify());

        Self {
            scope,
            rules: Vec::new(),
            rows: HashMap::new(),
            pending: HashSet::new(),
            adding: HashSet::new(),
            errors: HashMap::new(),
            generation: 0,
            icons: Rc::default(),
            focus_handle: cx.focus_handle(),
            _settings: settings,
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

    /// Shows `rules`, keeping the controls of the rows that stay.
    fn set_rules(&mut self, rules: Vec<Rule>, window: &mut Window, cx: &mut Context<Self>) {
        let mut rows = HashMap::new();
        let inherit = self.scope.inherit_target();

        for rule in &rules {
            let disabled = self.pending.contains(&rule.id);
            let row = match self.rows.remove(&rule.id) {
                Some(row) => {
                    row.select.update(cx, |select, cx| {
                        select.set_value(rule.style.clone(), cx);
                        select.set_disabled(disabled, cx);
                    });
                    row
                }
                None => {
                    let select = cx.new(|cx| {
                        let mut select = StyleSelect::new(rule.style.clone(), inherit, window, cx);
                        select.set_disabled(disabled, cx);
                        select
                    });
                    let subscription = cx.subscribe_in(&select, window, {
                        let id = rule.id.clone();
                        move |this, _, choice, window, cx| match choice {
                            StyleSelected::Style(style) => {
                                this.change_style(&id, *style, window, cx);
                            }
                            StyleSelected::Inherit => this.delete(&id, window, cx),
                        }
                    });

                    RowControls {
                        select,
                        remove_focus: cx.focus_handle().tab_stop(true),
                        _subscription: subscription,
                    }
                }
            };

            rows.insert(rule.id.clone(), row);
        }

        self.errors.retain(|id, _| rows.contains_key(id));
        self.rows = rows;
        self.rules = rules;
        cx.notify();
    }

    /// Fetches the stored styles.
    pub fn fetch(&self, window: &Window, cx: &mut Context<Self>) -> Task<()> {
        let scope = self.scope;
        let generation = self.generation;
        let request = backend::call(cx, move |backend| scope.list(backend));

        cx.spawn_in(window, async move |this, cx| {
            if let Ok(rules) = request.await {
                let refetch = this.update_in(cx, |this, window, cx| {
                    if this.generation != generation {
                        Some(this.fetch(window, cx))
                    } else {
                        this.set_rules(rules, window, cx);
                        None
                    }
                });
                if let Ok(Some(refetch)) = refetch {
                    refetch.await;
                }
            }
        })
    }

    /// Gives an app or a site the neutral style, unless it has a rule already.
    /// Answers whether it was added.
    pub fn add(
        &mut self,
        key: String,
        name: Option<String>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Task<bool> {
        if self.keys().any(|existing| existing == key) || !self.adding.insert(key.clone()) {
            return Task::ready(false);
        }

        let scope = self.scope;
        let request_key = key.clone();
        let request = backend::call(cx, move |backend| {
            scope.set(backend, &request_key, name.as_deref(), INITIAL_STYLE)
        });

        cx.spawn_in(window, async move |this, cx| {
            let result = request.await;
            this.update_in(cx, |this, window, cx| {
                this.adding.remove(&key);
                let Ok(rule) = result else {
                    return false;
                };

                this.generation += 1;
                analytics::capture(cx, scope.added_event(), &[("style", INITIAL_STYLE.into())]);

                let mut rules = this.rules.clone();
                if let Some(existing) = rules
                    .iter_mut()
                    .find(|existing| existing.id == rule.id || existing.key == rule.key)
                {
                    *existing = rule;
                } else {
                    rules.push(rule);
                }
                rules.sort_by_key(|rule| {
                    rule.name
                        .as_deref()
                        .unwrap_or(&rule.key)
                        .to_ascii_lowercase()
                });
                this.set_rules(rules, window, cx);
                true
            })
            .unwrap_or(false)
        })
    }

    fn set_row_disabled(&self, id: &str, disabled: bool, cx: &mut Context<Self>) {
        if let Some(row) = self.rows.get(id) {
            row.select
                .update(cx, |select, cx| select.set_disabled(disabled, cx));
        }
    }

    fn change_style(
        &mut self,
        id: &str,
        style: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending.contains(id) {
            return;
        }
        let Some(rule) = self.rules.iter().find(|rule| rule.id == id) else {
            return;
        };
        if rule.style == style {
            return;
        }

        let scope = self.scope;
        let id = id.to_string();
        let (key, name) = (rule.key.clone(), rule.name.clone());
        self.pending.insert(id.clone());
        self.errors.remove(&id);
        self.set_row_disabled(&id, true, cx);
        cx.notify();

        let request = backend::call(cx, move |backend| {
            scope.set(backend, &key, name.as_deref(), style)
        });

        cx.spawn_in(window, async move |this, cx| {
            let result = request.await;
            this.update(cx, |this, cx| {
                this.pending.remove(&id);
                this.set_row_disabled(&id, false, cx);

                match result {
                    Ok(_) => {
                        this.generation += 1;
                        if let Some(rule) = this.rules.iter_mut().find(|rule| rule.id == id) {
                            rule.style = style.into();
                        }
                        if let Some(row) = this.rows.get(&id) {
                            row.select
                                .update(cx, |select, cx| select.set_value(style, cx));
                        }
                        analytics::capture(cx, scope.changed_event(), &[("style", style.into())]);
                    }
                    Err(error) => {
                        log::error!(target: "style", "change_failed scope={scope:?} error={error}");
                        this.errors.insert(
                            id.clone(),
                            RuleError {
                                title: t(cx, "style.saveFailed"),
                                detail: error.into(),
                            },
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn delete(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending.contains(id) || !self.rules.iter().any(|rule| rule.id == id) {
            return;
        }

        let scope = self.scope;
        let id = id.to_string();
        self.pending.insert(id.clone());
        self.errors.remove(&id);
        self.set_row_disabled(&id, true, cx);
        cx.notify();

        let request_id = id.clone();
        let request = backend::call(cx, move |backend| scope.delete(backend, &request_id));

        cx.spawn_in(window, async move |this, cx| {
            let result = request.await;
            this.update_in(cx, |this, window, cx| {
                this.pending.remove(&id);

                match result {
                    Ok(()) => {
                        let had_focus = this.rows.get(&id).is_some_and(|row| {
                            row.remove_focus.is_focused(window)
                                || row.select.read(cx).is_focused(window)
                        });
                        this.generation += 1;
                        analytics::capture(cx, scope.deleted_event(), &[]);
                        this.rules.retain(|rule| rule.id != id);
                        this.rows.remove(&id);
                        this.errors.remove(&id);
                        if had_focus {
                            window.focus(&this.focus_handle, cx);
                        }
                    }
                    Err(error) => {
                        log::error!(target: "style", "delete_failed scope={scope:?} error={error}");
                        this.set_row_disabled(&id, false, cx);
                        this.errors.insert(
                            id.clone(),
                            RuleError {
                                title: t(cx, "style.removeFailed"),
                                detail: error.into(),
                            },
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn render_row(
        &self,
        rule: &Rule,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let p = palette(cx);
        let row = self.rows.get(&rule.id)?;
        let select = row.select.clone();
        let pending = self.pending.contains(&rule.id);
        let error = self.errors.get(&rule.id).cloned();
        // The dropdown's list is part of the row, though it hangs outside it.
        let hovered = select.read(cx).list_hovered();
        let remove_label = upper(&format!("[{}]", t(cx, "appInstructions.delete")));

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

        let remove_button = div()
            .id("remove")
            .transition_colors()
            .track_focus(&row.remove_focus)
            .focus(|button| button.text_color(p.txt_primary))
            .when(!pending, |button| {
                button
                    .hover(|button| button.text_color(p.txt_primary))
                    .on_click(cx.listener({
                        let id = rule.id.clone();
                        move |this, _, window, cx| this.delete(&id, window, cx)
                    }))
            })
            .relative()
            .flex_shrink_0()
            .type_xs()
            .text_right()
            .text_color(p.txt_secondary)
            .child(
                div()
                    .when(pending, |label| label.opacity(0.5))
                    .child(text(remove_label).tracking_wider()),
            )
            .when(focus_visible(&row.remove_focus, window), |button| {
                button.child(focus_ring(3., cx))
            });

        Some(
            div()
                .id(rule.id.clone())
                .transition_colors()
                .px_4()
                .py_3()
                .flex()
                .flex_col()
                .gap_2()
                .bg(p.surface)
                .border_1()
                .border_color(if hovered { p.border_strong } else { p.border })
                .hover(|row| row.border_color(p.border_strong))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_3()
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
                                        .text_color(p.txt_hint)
                                        .child(text(subtitle).truncate())
                                })),
                        )
                        .child(select)
                        .child(remove_button),
                )
                .children(error.map(|error| {
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .type_xs()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .text_color(p.txt_primary)
                                .child(icon("alert-circle").size_4().text_color(p.ac))
                                .child(text(error.title)),
                        )
                        .when(!error.detail.is_empty(), |block| {
                            block.child(
                                div()
                                    .pl_6()
                                    .text_color(p.txt_hint)
                                    .child(text(error.detail).break_all()),
                            )
                        })
                })),
        )
    }
}

impl Render for StyleRules {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .gap_1()
            .children(
                self.rules
                    .iter()
                    .filter_map(|rule| self.render_row(rule, window, cx)),
            )
    }
}
