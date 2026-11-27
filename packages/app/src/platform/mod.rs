#[cfg(target_os = "macos")]
pub mod accessibility_watcher;

pub mod activation;
pub mod drawables;
#[cfg(target_os = "macos")]
pub mod lid;
pub mod main_thread;
pub mod overlay_window;
pub mod window_icon;

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub mod system_key_watcher;
