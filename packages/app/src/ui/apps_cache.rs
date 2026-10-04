//! The installed apps and their icons, which are slow to gather: loaded once
//! while the main window is open, shared by the pages that offer an app
//! picker, and dropped with the window.

use gpui_kit::{App, Global, Task};
use std::rc::Rc;

use crate::backend::{self, CommandResult, InstalledApp};
use crate::ui::widgets::app_icon::AppIcons;

#[derive(Clone)]
pub struct InstalledApps {
    pub apps: Vec<InstalledApp>,
    pub icons: Rc<AppIcons>,
}

#[derive(Default)]
struct AppsCache {
    installed: Option<InstalledApps>,
    /// Counts the clears, so that a load the window did not live to see the
    /// end of does not fill the cache again.
    generation: u64,
}

impl Global for AppsCache {}

/// The apps, if they have been loaded.
pub fn cached(cx: &App) -> Option<InstalledApps> {
    cx.try_global::<AppsCache>()
        .and_then(|cache| cache.installed.clone())
}

/// Loads the apps, from the cache when it is filled.
pub fn load(cx: &mut App) -> Task<CommandResult<InstalledApps>> {
    if let Some(installed) = cached(cx) {
        return Task::ready(Ok(installed));
    }

    let request = backend::call(cx, |backend| backend.list_installed_apps());
    let generation = cx.default_global::<AppsCache>().generation;

    cx.spawn(async move |cx| {
        let apps = request.await?;
        let installed = InstalledApps {
            icons: Rc::new(AppIcons::decode(&apps)),
            apps,
        };
        cx.update(|cx| {
            let cache = cx.default_global::<AppsCache>();
            if cache.generation == generation {
                cache.installed = Some(installed.clone());
            }
        });

        Ok(installed)
    })
}

/// Starts loading the apps in the background.
pub fn warm(cx: &mut App) {
    let load = load(cx);

    cx.spawn(async move |_| match load.await {
        Ok(installed) => {
            log::debug!(target: "app", "installed_apps_loaded count={}", installed.apps.len());
        }
        Err(error) => log::error!(target: "app", "installed_apps_failed error={error}"),
    })
    .detach();
}

/// Forgets the apps, and the icons GPUI decoded for them, which it would
/// otherwise keep for the rest of the run.
pub fn clear(cx: &mut App) {
    let cache = cx.default_global::<AppsCache>();
    cache.generation += 1;
    let Some(installed) = cache.installed.take() else {
        return;
    };

    for icon in installed.icons.images() {
        icon.remove_asset(cx);
    }
}
