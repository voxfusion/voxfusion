//! Per-site dictionaries: words used while a given site is open in the
//! focused browser tab.

use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Subscription,
    Window, div, prelude::*,
};

use crate::analytics;
use crate::ui::pages::section::{empty_state, list_header};
use crate::ui::widgets::add_site_form::{AddSiteForm, SiteAdded};
use crate::ui::{t, t_with};

use super::groups::{DictionaryGroups, Scope};

pub struct SiteDictionaries {
    groups: Entity<DictionaryGroups>,
    form: Entity<AddSiteForm>,
    loading: bool,
    _subscriptions: [Subscription; 2],
}

impl SiteDictionaries {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        analytics::capture(
            cx,
            "$pageview",
            &[("$current_url", "/dictionary/sites".into())],
        );

        let groups = cx.new(|cx| DictionaryGroups::new(Scope::Sites, window, cx));
        let form = cx.new(|cx| AddSiteForm::new(window, cx));

        let subscriptions = [
            cx.observe(&groups, |_, _, cx| cx.notify()),
            cx.subscribe(&form, |this, _, SiteAdded(domain), cx| {
                this.groups
                    .update(cx, |groups, cx| groups.add(domain.clone(), None, cx));
            }),
        ];

        let fetch = groups.update(cx, |groups, cx| groups.fetch(cx));
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
            groups,
            form,
            loading: true,
            _subscriptions: subscriptions,
        }
    }
}

impl Render for SiteDictionaries {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.groups.read(cx).len();
        let count_label = (count > 0)
            .then(|| t_with(cx, "dictionary.siteCount", &[("count", &count.to_string())]));

        div()
            .child(list_header(
                Some(t(cx, "dictionary.sitesDescription")),
                count_label,
                cx,
            ))
            .child(self.form.clone())
            .when(!self.loading, |page| {
                if count == 0 {
                    page.child(empty_state(
                        "NO_SITES_CONFIGURED",
                        t(cx, "dictionary.sitesEmptyState"),
                        t(cx, "dictionary.sitesEmptyStateDescription"),
                        cx,
                    ))
                } else {
                    page.child(self.groups.clone())
                }
            })
    }
}
