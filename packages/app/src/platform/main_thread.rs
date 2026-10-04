//! Runs work on the thread that runs the app's event loop.

/// Runs `work` on the main thread and waits for its result. Parts of AppKit
/// and Carbon, such as event monitors and hotkeys, only work from there.
///
/// The main thread must be running its event loop rather than waiting for
/// the caller, or this never returns. Called on the main thread, it runs
/// `work` directly.
#[cfg(target_os = "macos")]
pub fn run<T: Send>(work: impl FnOnce() -> T + Send) -> T {
    dispatch2::run_on_main(|_| work())
}

/// Other platforms have no such restriction.
#[cfg(not(target_os = "macos"))]
pub fn run<T: Send>(work: impl FnOnce() -> T + Send) -> T {
    work()
}
