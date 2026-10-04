//! Starts the app: its services, the menu bar item, and the windows.

use gpui_kit::{AnyWindowHandle, App, AsyncApp, Entity, Global, Menu, MenuItem, TextRenderingMode};
use std::sync::Arc;

use crate::actions::{self, CheckForUpdates, CloseWindow, OpenSettings, Quit};
use crate::backend::native::{self, NativeBackend};
use crate::backend::{self, AppEvent, EventSender};
use crate::settings::SettingsStore;
use crate::ui::main_window::{MainView, Route, main_window_options, navigate, open_main_window};
use crate::updater::AppUpdater;
use crate::{
    analytics, assets, diagnostics, events, logging, paths, platform, single_instance, ui,
};

pub fn run() {
    // Asking for System Audio Recording relaunches this executable with an
    // argument (see `backend::native::media`); that copy does only that.
    if native::run_permission_request_if_asked() || diagnostics::run_if_asked() {
        return;
    }

    #[cfg(feature = "fixture")]
    if let Some(scenario) = crate::fixture::load_scenario() {
        return run_fixture(scenario);
    }

    logging::init();
    log::info!(
        target: "runtime",
        "setup_started cargo_package_version={}",
        env!("CARGO_PKG_VERSION")
    );

    // macOS gives permissions to an app bundle. A bare executable started
    // from a terminal is held to the terminal's permissions: the prompts name
    // the terminal, and the one for System Audio Recording never appears.
    // `scripts/dev.sh` runs a debug build as a bundle.
    #[cfg(target_os = "macos")]
    if !std::env::current_exe().is_ok_and(|executable| {
        executable
            .to_string_lossy()
            .contains(".app/Contents/MacOS/")
    }) {
        log::warn!(target: "runtime", "started_outside_bundle");
    }

    let (sender, receiver) = events::channel();

    if !single_instance::acquire(sender.clone()) {
        log::info!(target: "runtime", "single_instance_requested");
        return;
    }

    let app = gpui_kit::application().with_assets(assets::Assets);

    // Launching the app again while it runs brings its window back.
    app.on_reopen(|cx| {
        log::info!(target: "runtime", "reopen_requested");
        native::resynchronize_system_keys("reopen");
        show_main_window(None, cx);
    });

    app.run(move |cx: &mut App| {
        platform::activation::become_menu_bar_app();
        init_interface(cx);
        events::init(sender.clone(), receiver, cx);
        SettingsStore::init(Some(paths::settings_file()), None, cx);

        let backend = match NativeBackend::new(sender.clone()) {
            Ok(backend) => backend,
            Err(error) => {
                log::error!(target: "runtime", "backend_failed error={error}");
                cx.quit();
                return;
            }
        };
        backend::init(backend, Arc::new(AppUpdater::new()), cx);

        analytics::init(cx);
        analytics::capture(cx, "app_opened", &[]);

        ui::voice_control::init(cx);
        log::info!(target: "runtime", "voice_control_created");

        handle_actions(cx);
        handle_events(cx);

        menu_bar_item::setup(sender.clone(), cx);

        show_main_window(None, cx);

        cx.on_app_quit(|_| async {
            log::warn!(target: "runtime", "exit");
            native::restore_media_on_exit();
            single_instance::release();
        })
        .detach();

        log::info!(target: "runtime", "setup_completed");
    });
}

/// What every window needs before it opens.
fn init_interface(cx: &mut App) {
    gpui_kit::init(cx);
    // The design uses grayscale antialiasing (`-webkit-font-smoothing: antialiased`).
    cx.set_text_rendering_mode(TextRenderingMode::Grayscale);
    assets::load_fonts(cx);
    ui::theme::init_fonts(cx);
    actions::bind_keys(cx);
}

struct MainWindow {
    window: AnyWindowHandle,
    view: Entity<MainView>,
}

impl Global for MainWindow {}

/// Brings the main window to the front, opening it if it was closed. Closing
/// the window only closes the window: the app keeps running in the menu bar.
pub fn show_main_window(path: Option<&str>, cx: &mut App) {
    let open = cx.try_global::<MainWindow>().map(|main| main.window);
    let raised = open.is_some_and(|window| {
        window
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    });

    if raised {
        if let Some(path) = path {
            navigate(cx, path);
        }
    } else {
        let route = Route::from_path(path.unwrap_or("/"));
        let options = main_window_options(None, None, cx);

        match open_main_window(route, options, cx) {
            Ok((window, view)) => cx.set_global(MainWindow { window, view }),
            Err(error) => log::error!(target: "runtime", "main_window_failed error={error}"),
        }
    }

    // A menu bar app is not activated by opening a window.
    cx.activate(true);
}

fn handle_actions(cx: &mut App) {
    cx.on_action(|_: &Quit, cx| cx.quit());

    cx.on_action(|_: &CloseWindow, cx| {
        let Some(window) = cx.try_global::<MainWindow>().map(|main| main.window) else {
            return;
        };
        log::info!(target: "runtime", "window_close_requested");

        // The shortcut arrives while the window handles the key, and a
        // window cannot be updated again from inside its own update.
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, _| window.remove_window());
        });
    });

    cx.on_action(|_: &CheckForUpdates, cx| check_for_updates(cx));

    // A menu bar app shows no menus; these give the standard shortcuts a
    // target when macOS looks for one.
    cx.set_menus(vec![Menu::new("VoxFusion").items([
        MenuItem::action("Check for Updates", CheckForUpdates),
        MenuItem::separator(),
        MenuItem::action("Settings…", OpenSettings),
        MenuItem::separator(),
        MenuItem::action("Close Window", CloseWindow),
        MenuItem::action("Quit VoxFusion", Quit),
    ])]);
}

fn check_for_updates(cx: &mut App) {
    log::info!(target: "runtime", "check_for_updates_requested");
    show_main_window(None, cx);
    events::emit(cx, AppEvent::CheckForUpdates);
}

/// The events that concern the app as a whole rather than one window.
fn handle_events(cx: &mut App) {
    cx.subscribe(&events::hub(cx), |_, event, cx| match event {
        AppEvent::ShowMainWindow => {
            log::info!(target: "runtime", "single_instance_requested");
            show_main_window(None, cx);
        }
        AppEvent::AccessibilityPermissionNeeded => {
            // Typing failed because the permission was revoked. The main
            // window may be closed: open it, then let it show the settings.
            show_main_window(None, cx);
            if let Some(view) = cx.try_global::<MainWindow>().map(|main| main.view.clone()) {
                let window = cx.global::<MainWindow>().window;
                let _ = window.update(cx, |_, window, cx| {
                    view.update(cx, |view, cx| view.open_settings(window, cx));
                });
            }
        }
        AppEvent::SelectMicrophone(device_name) => {
            let device_name = (!device_name.is_empty()).then(|| device_name.clone());
            SettingsStore::update(cx, |settings| {
                settings.selected_microphone_id = device_name;
            });
        }
        _ => {}
    })
    .detach();
}

/// The menu bar item and what keeps its microphone list current.
mod menu_bar_item {
    use super::*;
    use crate::tray::{Tray, TrayCommand};

    struct MenuBarItem(Tray);

    impl Global for MenuBarItem {}

    pub fn setup(events: EventSender, cx: &mut App) {
        // The menu calls back outside of any app update; a channel brings
        // its commands back in.
        let (commands, chosen) = async_channel::unbounded::<TrayCommand>();

        let tray = match Tray::new(move |command| {
            let _ = commands.try_send(command);
        }) {
            Ok(tray) => tray,
            Err(error) => {
                log::error!(target: "runtime", "tray_failed error={error}");
                return;
            }
        };
        cx.set_global(MenuBarItem(tray));
        log::info!(target: "runtime", "tray_setup");

        cx.spawn(async move |cx: &mut AsyncApp| {
            while let Ok(command) = chosen.recv().await {
                cx.update(|cx| run_command(command, &events, cx));
            }
        })
        .detach();

        cx.observe(&SettingsStore::entity(cx), |_, cx| list_microphones(cx))
            .detach();
        cx.subscribe(&events::hub(cx), |_, event, cx| {
            if matches!(event, AppEvent::AudioDevicesChanged) {
                list_microphones(cx);
            }
        })
        .detach();
        list_microphones(cx);
    }

    fn run_command(command: TrayCommand, events: &EventSender, cx: &mut App) {
        match command {
            TrayCommand::ShowHome => show_main_window(Some("/"), cx),
            TrayCommand::SelectMicrophone(name) => events.emit(AppEvent::SelectMicrophone(name)),
            TrayCommand::CheckForUpdates => check_for_updates(cx),
            TrayCommand::Quit => cx.quit(),
        }
    }

    /// Refreshes the Microphone submenu. Listing devices is slow (see the
    /// notes on cpal in AGENTS.md), so it happens off the main thread.
    fn list_microphones(cx: &mut App) {
        let devices = backend::call(cx, |backend| backend.list_audio_devices());

        cx.spawn(async move |cx: &mut AsyncApp| {
            let devices = devices.await.unwrap_or_default();

            cx.update(|cx| {
                let selected = SettingsStore::get(cx).selected_microphone_id.clone();
                let devices: Vec<(String, bool)> = devices
                    .into_iter()
                    .map(|device| (device.name, device.is_default))
                    .collect();

                cx.global::<MenuBarItem>()
                    .0
                    .set_microphones(&devices, selected.as_deref());
            });
        })
        .detach();
    }
}

/// Shows one window against the scenario's canned data.
#[cfg(feature = "fixture")]
fn run_fixture(scenario: crate::fixture::Scenario) {
    use crate::fixture::{self, ScenarioWindow};

    let (sender, receiver) = events::channel();

    gpui_kit::application()
        .with_assets(assets::Assets)
        .run(move |cx: &mut App| {
            init_interface(cx);
            events::init(sender.clone(), receiver, cx);

            // Screenshots need everything at rest. `VOXFUSION_FIXTURE_MOTION`
            // leaves the animations running, to watch a transition play.
            if std::env::var_os("VOXFUSION_FIXTURE_MOTION").is_none() {
                ui::widgets::freeze_animations();
                cx.set_reduce_motion(true);
            }
            if let Some(theme) = scenario.theme.as_deref() {
                ui::theme::override_system_appearance(theme == "dark");
            }
            if let Some(now) = scenario.fixture.now {
                fixture::set_clock(now);
            }

            SettingsStore::init(None, Some(scenario.fixture.settings.clone()), cx);
            ui::settings_modal::set_app_version(scenario.fixture.app_version.clone(), cx);
            let (backend, updater, wheels) = fixture::services(scenario.fixture.clone(), sender);
            backend::init(backend, updater, cx);

            cx.spawn(async move |cx: &mut AsyncApp| {
                while let Ok(wheel) = wheels.recv().await {
                    cx.update(|cx| scroll_window(wheel, cx));
                }
            })
            .detach();

            match scenario.window {
                ScenarioWindow::Main => {
                    let mut options = main_window_options(Some(scenario.size), Some((0., 0.)), cx);
                    // Screenshots are of the content alone.
                    options.titlebar = None;

                    open_main_window(Route::from_path(&scenario.route), options, cx)
                        .expect("failed to open the main window");
                }
                ScenarioWindow::VoiceControl => {
                    ui::voice_control::open_fixture_window(scenario.size, cx);
                }
            }
        });
}

/// Scrolls whatever is under the given point of the scenario's window.
#[cfg(feature = "fixture")]
fn scroll_window(wheel: crate::fixture::Wheel, cx: &mut App) {
    use gpui_kit::{PlatformInput, ScrollDelta, ScrollWheelEvent, TouchPhase, point, px};

    let Some(window) = cx.windows().first().copied() else {
        return;
    };

    let _ = window.update(cx, |_, window, cx| {
        window.dispatch_event(
            PlatformInput::ScrollWheel(ScrollWheelEvent {
                position: point(px(wheel.x), px(wheel.y)),
                delta: ScrollDelta::Pixels(point(px(0.), px(-wheel.dy))),
                modifiers: Default::default(),
                touch_phase: TouchPhase::Moved,
            }),
            cx,
        );
    });
}
