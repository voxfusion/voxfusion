//! Per-app styles: the style used while a given app is focused.

use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    Subscription, Window, div, px, size,
};

use crate::analytics;
use crate::ui::apps_cache::{self, InstalledApps};
use crate::ui::pages::section::{empty_state, list_header};
use crate::ui::widgets::app_search::{AppSearch, AppSelected, SKELETON_ROWS, app_row_skeleton};
use crate::ui::{t, t_with};

use super::rules::{Scope, StyleRules};

pub struct AppStyles {
    rules: Entity<StyleRules>,
    search: Entity<AppSearch>,
    /// Whether the installed apps and the styles are still on their way.
    loading: bool,
    _subscriptions: [Subscription; 2],
}

impl AppStyles {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        analytics::capture(
            cx,
            "$pageview",
            &[("$current_url", "/style/per-app".into())],
        );

        let rules = cx.new(|cx| StyleRules::new(Scope::Apps, cx));
        let search = cx.new(|cx| AppSearch::new(window, cx));
        let cached_apps = apps_cache::cached(cx);

        let subscriptions = [
            // Apps that have a style are not offered again.
            cx.observe(&rules, |this, rules, cx| {
                let configured = rules.read(cx).keys().map(str::to_string).collect();
                this.search
                    .update(cx, |search, cx| search.set_excluded(configured, cx));
                cx.notify();
            }),
            cx.subscribe_in(&search, window, Self::add_app),
        ];

        let mut page = Self {
            rules,
            search,
            loading: cached_apps.is_none(),
            _subscriptions: subscriptions,
        };

        page.search
            .update(cx, |search, cx| search.set_loading(page.loading, cx));
        if let Some(installed) = cached_apps {
            page.set_installed_apps(installed, cx);
        }
        page.load(window, cx);
        page
    }

    fn set_installed_apps(&mut self, installed: InstalledApps, cx: &mut Context<Self>) {
        let InstalledApps { apps, icons } = installed;

        self.rules
            .update(cx, |rules, cx| rules.set_icons(icons.clone(), cx));
        self.search
            .update(cx, |search, cx| search.set_apps(apps, icons, cx));
    }

    /// Loads the installed apps, then the styles: neither shows until both
    /// have arrived.
    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let apps = apps_cache::load(cx);

        cx.spawn_in(window, async move |this, cx| {
            if let Ok(installed) = apps.await {
                this.update(cx, |this, cx| this.set_installed_apps(installed, cx))
                    .ok();
            }

            let fetch = this.update_in(cx, |this, window, cx| {
                this.rules.update(cx, |rules, cx| rules.fetch(window, cx))
            });
            if let Ok(fetch) = fetch {
                fetch.await;
            }

            this.update(cx, |this, cx| {
                this.loading = false;
                this.search
                    .update(cx, |search, cx| search.set_loading(false, cx));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn add_app(
        &mut self,
        _: &Entity<AppSearch>,
        AppSelected(app): &AppSelected,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let added = self.rules.update(cx, |rules, cx| {
            rules.add(app.bundle_id.clone(), Some(app.name.clone()), window, cx)
        });

        cx.spawn_in(window, async move |this, cx| {
            if added.await {
                this.update_in(cx, |this, window, cx| {
                    this.search
                        .update(cx, |search, cx| search.reset(window, cx));
                })
                .ok();
            }
        })
        .detach();
    }
}

impl Render for AppStyles {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.rules.read(cx).len();
        let count_label = (count > 0).then(|| {
            t_with(
                cx,
                "appInstructions.appCount",
                &[("count", &count.to_string())],
            )
        });

        let list = if self.loading {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .children(
                    (0..SKELETON_ROWS)
                        .map(|index| app_row_skeleton(index, Some(size(px(160.), px(24.))), cx)),
                )
                .into_any_element()
        } else if count == 0 {
            empty_state(
                "NO_APPS_CONFIGURED",
                t(cx, "appInstructions.emptyState"),
                t(cx, "appInstructions.emptyStateDescription"),
                cx,
            )
            .into_any_element()
        } else {
            self.rules.clone().into_any_element()
        };

        div()
            .child(list_header(
                Some(t(cx, "appInstructions.description")),
                count_label,
                cx,
            ))
            .child(self.search.clone())
            .child(list)
    }
}
