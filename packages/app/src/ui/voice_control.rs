//! Dictation, and the small overlay shown while dictating.

mod bars;
mod controller;
mod error_pill;
mod hotkeys;
mod overlay;
mod position;
mod spinner;
mod state;
mod transition;

use gpui_kit::{App, AppContext as _, Entity};

use controller::VoiceController;

/// Starts the dictation controller, which lives until the app exits.
fn start_controller(cx: &mut App) -> Entity<VoiceController> {
    let controller = cx.new(VoiceController::new);

    cx.on_app_quit({
        let controller = controller.clone();
        move |cx| {
            controller.update(cx, |controller, cx| controller.shut_down(cx));
            async {}
        }
    })
    .detach();

    controller
}

/// Makes dictation available: the hotkeys are registered once onboarding
/// allows it, and the overlay waits, hidden, for the first recording. Needs
/// the settings, the event hub and the backend.
pub fn init(cx: &mut App) {
    let controller = start_controller(cx);

    if let Err(error) = overlay::open(controller, None, cx) {
        log::error!(target: "voice", "overlay_window_failed error={error}");
    }
}

/// Opens the overlay as the only window, at `size`, for a fixture scenario.
#[cfg(feature = "fixture")]
pub fn open_fixture_window(size: (f32, f32), cx: &mut App) {
    let controller = start_controller(cx);

    if let Some(scenario) = crate::fixture::load_scenario() {
        controller.update(cx, |controller, _| {
            controller.set_wave_offset(scenario.fixture.wave_offset);
        });
    }

    overlay::open(controller, Some(size), cx).expect("failed to open the overlay");
}
