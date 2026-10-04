//! The list of installed apps, which is slow to gather, loaded once and
//! shared by the pages that offer an app picker.

use gpui_kit::{App, Global, Task};

use crate::backend::{self, CommandResult, InstalledApp};

#[derive(Default)]
struct AppsCache {
    apps: Option<Vec<InstalledApp>>,
}

impl Global for AppsCache {}

/// The apps, if they have been loaded.
pub fn cached(cx: &App) -> Option<Vec<InstalledApp>> {
    cx.try_global::<AppsCache>()
        .and_then(|cache| cache.apps.clone())
}

/// Loads the apps, from the cache when it is filled.
pub fn load(cx: &mut App) -> Task<CommandResult<Vec<InstalledApp>>> {
    if let Some(apps) = cached(cx) {
        return Task::ready(Ok(apps));
    }

    let request = backend::call(cx, |backend| backend.list_installed_apps());

    cx.spawn(async move |cx| {
        let apps = request.await?;
        cx.update(|cx| cx.default_global::<AppsCache>().apps = Some(apps.clone()));

        Ok(apps)
    })
}

/// Starts loading the apps in the background.
pub fn warm(cx: &mut App) {
    let load = load(cx);

    cx.spawn(async move |_| match load.await {
        Ok(apps) => log::debug!(target: "app", "installed_apps_loaded count={}", apps.len()),
        Err(error) => log::error!(target: "app", "installed_apps_failed error={error}"),
    })
    .detach();
}
