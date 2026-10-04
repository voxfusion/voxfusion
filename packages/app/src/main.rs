mod actions;
mod analytics;
mod app;
mod assets;
mod backend;
mod diagnostics;
mod events;
#[cfg(feature = "fixture")]
mod fixture;
mod i18n;
mod logging;
mod paths;
mod platform;
mod settings;
mod single_instance;
mod sounds;
mod tray;
mod ui;
mod updater;

fn main() {
    app::run();
}
