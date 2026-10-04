mod api;
pub mod native;

pub use api::*;

use gpui_kit::{App, AppContext as _, Global, Task};

struct GlobalBackend {
    backend: SharedBackend,
    updater: SharedUpdater,
}

impl Global for GlobalBackend {}

pub fn init(backend: SharedBackend, updater: SharedUpdater, cx: &mut App) {
    cx.set_global(GlobalBackend { backend, updater });
}

pub fn backend(cx: &App) -> SharedBackend {
    cx.global::<GlobalBackend>().backend.clone()
}

pub fn updater(cx: &App) -> SharedUpdater {
    cx.global::<GlobalBackend>().updater.clone()
}

/// Runs a backend call on its own thread, since any of them may block: on
/// the database, on CoreAudio, or for the length of a download.
pub fn call<T: Send + 'static>(
    cx: &App,
    request: impl FnOnce(&dyn Backend) -> T + Send + 'static,
) -> Task<T> {
    run_off_thread(cx, backend(cx), move |backend| request(&**backend))
}

/// [`call`] for the updater.
pub fn call_updater<T: Send + 'static>(
    cx: &App,
    request: impl FnOnce(&dyn Updater) -> T + Send + 'static,
) -> Task<T> {
    run_off_thread(cx, updater(cx), move |updater| request(&**updater))
}

fn run_off_thread<S: Send + 'static, T: Send + 'static>(
    cx: &App,
    service: S,
    request: impl FnOnce(&S) -> T + Send + 'static,
) -> Task<T> {
    let (sender, receiver) = async_channel::bounded(1);

    std::thread::spawn(move || {
        let _ = sender.send_blocking(request(&service));
    });

    cx.background_spawn(async move {
        receiver
            .recv()
            .await
            .expect("a backend call always reports its result")
    })
}

/// The message the interface shows for a failed call.
pub fn command_error(command: &str, error: &str) -> String {
    format!("{command} failed: {error}")
}
