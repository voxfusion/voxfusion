//! Pieces shared across the windows.

pub mod add_site_form;
pub mod app_icon;
pub mod app_search;
mod icon;
pub mod progress_bar;
pub mod select;
pub mod style_select;
pub mod text_field;
pub mod toggle;
pub mod word_list;

pub use icon::{Icon, icon, spinning};

use std::sync::atomic::{AtomicBool, Ordering};

static ANIMATIONS_FROZEN: AtomicBool = AtomicBool::new(false);

/// Holds every animation at its first frame, so screenshots are repeatable.
#[cfg(feature = "fixture")]
pub fn freeze_animations() {
    ANIMATIONS_FROZEN.store(true, Ordering::Relaxed);
}

pub fn animations_frozen() -> bool {
    ANIMATIONS_FROZEN.load(Ordering::Relaxed)
}
