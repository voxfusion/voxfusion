//! Per-site styles: the style used while a given site is open in the
//! focused browser tab.

use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Subscription,
    Window, div, prelude::*,
};

use crate::analytics;
use crate::settings::SettingsStore;
use crate::ui::pages::section::{empty_state, list_header};
use crate::ui::widgets::add_site_form::{AddSiteForm, SiteAdded};
use crate::ui::widgets::style_select::style_label;
use crate::ui::{t, t_with};

use super::rules::{Scope, StyleRules};

pub struct SiteStyles {
    rules: Entity<StyleRules>,
    form: Entity<AddSiteForm>,
    loading: bool,
    _subscriptions: [Subscription; 3],
}

impl SiteStyles {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        analytics::capture(cx, "$pageview", &[("$current_url", "/style/sites".into())]);

        let rules = cx.new(|cx| StyleRules::new(Scope::Sites, cx));
        let form = cx.new(|cx| AddSiteForm::new(window, cx));

        let subscriptions = [
            cx.observe(&rules, |_, _, cx| cx.notify()),
            cx.subscribe_in(&form, window, |this, _, SiteAdded(domain), window, cx| {
                this.rules
                    .update(cx, |rules, cx| rules.add(domain.clone(), None, window, cx))
                    .detach();
            }),
            cx.observe(&SettingsStore::entity(cx), |_, _, cx| cx.notify()),
        ];

        let fetch = rules.update(cx, |rules, cx| rules.fetch(window, cx));
        cx.spawn(async move |this, cx| {
            fetch.await;

            this.update(cx, |this, cx| {
                this.loading = false;
                cx.notify();
            })
            .ok();
        })
        .detach();

        Self {
            rules,
            form,
            loading: true,
            _subscriptions: subscriptions,
        }
    }
}

impl Render for SiteStyles {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.rules.read(cx).len();
        let global_default = style_label(&SettingsStore::get(cx).default_style, cx);
        let count_label = (count > 0)
            .then(|| t_with(cx, "dictionary.siteCount", &[("count", &count.to_string())]));

        div()
            .child(list_header(
                Some(t_with(
                    cx,
                    "style.perSiteDescription",
                    &[("style", global_default.as_ref())],
                )),
                count_label,
                cx,
            ))
            .child(self.form.clone())
            .when(!self.loading, |page| {
                if count == 0 {
                    page.child(empty_state(
                        "NO_SITES_CONFIGURED",
                        t(cx, "style.perSiteEmptyState"),
                        t(cx, "style.perSiteEmptyStateDescription"),
                        cx,
                    ))
                } else {
                    page.child(self.rules.clone())
                }
            })
    }
}
