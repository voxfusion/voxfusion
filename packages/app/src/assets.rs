//! Icons, images and fonts compiled into the app.

use gpui_kit::{App, AssetSource, Result, SharedString};
use std::borrow::Cow;

macro_rules! embedded {
    ($($path:literal),* $(,)?) => {
        &[$(($path, include_bytes!(concat!("../assets/", $path)))),*]
    };
}

const FILES: &[(&str, &[u8])] = embedded![
    "icons/alert-circle.svg",
    "icons/book-open.svg",
    "icons/check-circle.svg",
    "icons/check.svg",
    "icons/chevron-down.svg",
    "icons/copy.svg",
    "icons/download.svg",
    "icons/external-link.svg",
    "icons/globe.svg",
    "icons/home.svg",
    "icons/keyboard.svg",
    "icons/loader.svg",
    "icons/mic.svg",
    "icons/refresh-cw.svg",
    "icons/search.svg",
    "icons/send.svg",
    "icons/settings.svg",
    "icons/shield-check.svg",
    "icons/shield.svg",
    "icons/spinner-ring.svg",
    "icons/wand-2.svg",
    "icons/x.svg",
    "images/app-icon.svg",
];

/// The app's own files, then the icons bundled with GPUI Kit.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = FILES.iter().find(|(name, _)| *name == path) {
            return Ok(Some(Cow::Borrowed(*bytes)));
        }

        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(
            FILES
                .iter()
                .map(|(name, _)| *name)
                .filter(|name| name.starts_with(path))
                .map(SharedString::from),
        );

        Ok(paths)
    }
}

/// Registers the fonts the interface needs beyond the system's.
pub fn load_fonts(cx: &App) {
    let mut fonts: Vec<Cow<'static, [u8]>> = vec![
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Medium.ttf")),
    ];

    // SF Mono, the font `ui-monospace` resolves to, ships with macOS but is
    // not listed among the installed families.
    if cfg!(target_os = "macos") {
        for file in ["SFNSMono.ttf", "SFNSMonoItalic.ttf"] {
            if let Ok(bytes) = std::fs::read(format!("/System/Library/Fonts/{file}")) {
                fonts.push(Cow::Owned(bytes));
            }
        }
    }

    if let Err(error) = cx.text_system().add_fonts(fonts) {
        log::error!(target: "runtime", "font_load_failed error={error}");
    }
}
