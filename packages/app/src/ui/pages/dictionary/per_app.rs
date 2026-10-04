//! Per-app dictionaries: words used while a given app is focused.

use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    Subscription, Window, div, px, size,
};
use std::rc::Rc;

use crate::analytics;
use crate::backend::InstalledApp;
use crate::ui::apps_cache;
use crate::ui::pages::section::{empty_state, list_header};
use crate::ui::widgets::app_icon::AppIcons;
use crate::ui::widgets::app_search::{AppSearch, AppSelected, SKELETON_ROWS, app_row_skeleton};
use crate::ui::{t, t_with};

use super::groups::{DictionaryGroups, Scope};

pub struct AppDictionaries {
    groups: Entity<DictionaryGroups>,
    search: Entity<AppSearch>,
    /// Whether the installed apps and the dictionaries are still on their way.
    loading: bool,
    _subscriptions: [Subscription; 2],
}

impl AppDictionaries {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        analytics::capture(
            cx,
            "$pageview",
            &[("$current_url", "/dictionary/per-app".into())],
        );

        let groups = cx.new(|cx| DictionaryGroups::new(Scope::Apps, window, cx));
        let search = cx.new(|cx| AppSearch::new(window, cx));
        let cached_apps = apps_cache::cached(cx);

        let subscriptions = [
            // Apps that have a dictionary are not offered again.
            cx.observe(&groups, |this, groups, cx| {
                let configured = groups.read(cx).keys().map(str::to_string).collect();
                this.search
                    .update(cx, |search, cx| search.set_excluded(configured, cx));
                cx.notify();
            }),
            cx.subscribe_in(&search, window, |this, _, AppSelected(app), window, cx| {
                this.groups.update(cx, |groups, cx| {
                    groups.add(app.bundle_id.clone(), Some(app.name.clone()), cx);
                });
                this.search
                    .update(cx, |search, cx| search.reset(window, cx));
            }),
        ];

        let mut page = Self {
            groups,
            search,
            loading: cached_apps.is_none(),
            _subscriptions: subscriptions,
        };

        page.search
            .update(cx, |search, cx| search.set_loading(page.loading, cx));
        if let Some(apps) = cached_apps {
            page.set_installed_apps(apps, cx);
        }
        page.load(cx);
        page
    }

    fn set_installed_apps(&mut self, apps: Vec<InstalledApp>, cx: &mut Context<Self>) {
        let icons = Rc::new(AppIcons::decode(&apps));

        self.groups
            .update(cx, |groups, cx| groups.set_icons(icons.clone(), cx));
        self.search
            .update(cx, |search, cx| search.set_apps(apps, icons, cx));
    }

    /// Loads the installed apps, then the dictionaries.
    fn load(&mut self, cx: &mut Context<Self>) {
        let apps = apps_cache::load(cx);

        cx.spawn(async move |this, cx| {
            match apps.await {
                Ok(apps) => {
                    this.update(cx, |this, cx| this.set_installed_apps(apps, cx))
                        .ok();
                }
                Err(error) => {
                    log::error!(target: "dictionary", "installed_apps_failed error={error}");
                }
            }

            let fetch = this.update(cx, |this, cx| {
                this.groups.update(cx, |groups, cx| groups.fetch(cx))
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
}

impl Render for AppDictionaries {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.groups.read(cx).len();
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
                        .map(|index| app_row_skeleton(index, Some(size(px(80.), px(16.))), cx)),
                )
                .into_any_element()
        } else if count == 0 {
            empty_state(
                "NO_APPS_CONFIGURED",
                t(cx, "dictionary.perAppEmptyState"),
                t(cx, "dictionary.perAppEmptyStateDescription"),
                cx,
            )
            .into_any_element()
        } else {
            self.groups.clone().into_any_element()
        };

        div()
            .child(list_header(
                Some(t(cx, "dictionary.perAppDescription")),
                count_label,
                cx,
            ))
            .child(self.search.clone())
            .child(list)
    }
}
