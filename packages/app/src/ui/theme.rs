//! Colors and fonts. The palette holds the light and dark values of the
//! design's color tokens; [`palette`] returns the one in effect.

use gpui_kit::component::{Theme as KitTheme, ThemeMode as KitThemeMode};
use gpui_kit::{
    App, FontFallbacks, Global, Hsla, Rgba, SharedString, Window, WindowAppearance, font, px, rgb,
};
use std::sync::OnceLock;

use crate::settings::{SettingsStore, ThemeMode};
use crate::ui::widgets::animations_frozen;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub dark: bool,

    pub base: Hsla,
    pub surface: Hsla,
    pub elevated: Hsla,
    pub hover: Hsla,
    pub input: Hsla,
    pub overlay: Hsla,

    pub border: Hsla,
    pub border_strong: Hsla,

    pub txt_primary: Hsla,
    pub txt_secondary: Hsla,
    pub txt_muted: Hsla,
    pub txt_faint: Hsla,

    pub ac: Hsla,
    pub ac_hover: Hsla,
    pub ac_bg: Hsla,
    pub ac_on: Hsla,

    pub success: Hsla,
    pub grid_line: Hsla,
    /// Tailwind's `red-500`, used for hotkey validation errors.
    pub error: Hsla,
}

fn hex(value: u32) -> Hsla {
    rgb(value).into()
}

/// A color given as CSS `rgba(r, g, b, alpha)`.
fn css_rgba(r: u8, g: u8, b: u8, alpha: f32) -> Hsla {
    Rgba {
        r: r as f32 / 255.,
        g: g as f32 / 255.,
        b: b as f32 / 255.,
        a: alpha,
    }
    .into()
}

impl Palette {
    pub fn light() -> Self {
        Self {
            dark: false,
            base: hex(0xffffff),
            surface: hex(0xf5f5f5),
            elevated: hex(0xebebeb),
            hover: hex(0xe0e0e0),
            input: hex(0xffffff),
            overlay: css_rgba(0, 0, 0, 0.3),
            border: hex(0xd4d4d4),
            border_strong: hex(0xb0b0b0),
            txt_primary: hex(0x1a1a1a),
            txt_secondary: hex(0x555555),
            txt_muted: hex(0x888888),
            txt_faint: hex(0xaaaaaa),
            ac: hex(0xff3e00),
            ac_hover: hex(0xe03800),
            ac_bg: css_rgba(255, 62, 0, 0.08),
            ac_on: hex(0xffffff),
            success: hex(0x16a34a),
            grid_line: css_rgba(0, 0, 0, 0.04),
            error: hex(0xef4444),
        }
    }

    pub fn dark() -> Self {
        Self {
            dark: true,
            base: hex(0x0a0a0a),
            surface: hex(0x111111),
            elevated: hex(0x1a1a1a),
            hover: hex(0x1a1a1a),
            input: hex(0x0a0a0a),
            overlay: css_rgba(0, 0, 0, 0.7),
            border: hex(0x222222),
            border_strong: hex(0x333333),
            txt_primary: hex(0xe0e0e0),
            txt_secondary: hex(0x888888),
            txt_muted: hex(0x666666),
            txt_faint: hex(0x444444),
            ac: hex(0xff3e00),
            ac_hover: hex(0xff5500),
            ac_bg: hex(0x1a0a00),
            ac_on: hex(0x0a0a0a),
            success: hex(0x00ff88),
            grid_line: css_rgba(255, 255, 255, 0.03),
            error: hex(0xef4444),
        }
    }
}

struct ActivePalette(Palette);

impl Global for ActivePalette {}

/// The palette in effect.
pub fn palette(cx: &App) -> Palette {
    cx.try_global::<ActivePalette>()
        .map(|active| active.0)
        .unwrap_or_else(Palette::light)
}

static SYSTEM_APPEARANCE: OnceLock<bool> = OnceLock::new();

/// Fixes what "system" resolves to, whatever the machine is set to. Fixture
/// scenarios name the appearance they were recorded with.
#[cfg(feature = "fixture")]
pub fn override_system_appearance(dark: bool) {
    let _ = SYSTEM_APPEARANCE.set(dark);
}

/// Whether the system currently shows dark windows.
fn system_is_dark(appearance: WindowAppearance) -> bool {
    if let Some(dark) = SYSTEM_APPEARANCE.get() {
        return *dark;
    }

    matches!(
        appearance,
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    )
}

/// Applies the theme setting, resolving "system" with `appearance`.
pub fn apply(appearance: WindowAppearance, cx: &mut App) {
    let dark = match SettingsStore::get(cx).theme {
        ThemeMode::Dark => true,
        ThemeMode::Light => false,
        ThemeMode::System => system_is_dark(appearance),
    };

    let palette = if dark {
        Palette::dark()
    } else {
        Palette::light()
    };

    if cx.try_global::<ActivePalette>().map(|active| active.0) == Some(palette) {
        return;
    }

    cx.set_global(ActivePalette(palette));
    sync_kit_theme(palette, cx);
    cx.refresh_windows();
}

/// Applies the theme setting for `window` and keeps following the system
/// appearance and the setting.
pub fn follow(window: &mut Window, cx: &mut App) {
    apply(window.appearance(), cx);

    window
        .observe_window_appearance(|window, cx| apply(window.appearance(), cx))
        .detach();

    let handle = window.window_handle();
    cx.observe(&SettingsStore::entity(cx), move |_, cx| {
        let _ = handle.update(cx, |_, window, cx| apply(window.appearance(), cx));
    })
    .detach();
}

/// GPUI Kit's text inputs take their colors and fonts from the Kit theme.
fn sync_kit_theme(palette: Palette, cx: &mut App) {
    KitTheme::change(
        if palette.dark {
            KitThemeMode::Dark
        } else {
            KitThemeMode::Light
        },
        None,
        cx,
    );

    let theme = KitTheme::global_mut(cx);
    theme.font_family = sans_font();
    theme.mono_font_family = mono_font();
    theme.font_size = px(16.);
    theme.radius = px(0.);
    theme.radius_lg = px(0.);
    theme.shadow = false;
    theme.background = palette.base;
    theme.foreground = palette.txt_primary;
    theme.muted_foreground = palette.txt_muted;
    theme.border = palette.border;
    theme.input = palette.border_strong;
    theme.ring = palette.ac;
    // Screenshots are taken without the caret, as the reference ones are. A
    // fully transparent caret would count as unset and get the text color.
    theme.caret = if animations_frozen() {
        palette.txt_primary.opacity(0.001)
    } else {
        palette.txt_primary
    };
    theme.selection = if palette.dark {
        hex(0x3f638b)
    } else {
        hex(0xb3d7ff)
    };
}

static MONO_FONT: OnceLock<SharedString> = OnceLock::new();

/// The families to try for the interface font, best first. Tailwind's
/// `font-mono` starts with `ui-monospace`, which is SF Mono on macOS. SF Mono
/// is not among the installed families; it is reached through the system
/// font's monospaced variant, or through its font file (see
/// `assets::load_fonts`). Linux builds, which exist for development, use the
/// font the same CSS stack resolves to there.
fn mono_font_candidates() -> Vec<SharedString> {
    let mut candidates: Vec<SharedString> = Vec::new();

    if let Ok(family) = std::env::var("VOXFUSION_MONO_FONT") {
        candidates.push(family.into());
    }

    if cfg!(target_os = "macos") {
        candidates.extend([
            ".AppleSystemUIFontMonospaced".into(),
            ".SF NS Mono".into(),
            "SF Mono".into(),
            "Menlo".into(),
        ]);
    } else {
        candidates.push("Liberation Mono".into());
    }

    candidates
}

/// Picks the interface font: the first candidate family that exists. Call
/// once at startup, after the app's own fonts are registered.
pub fn init_fonts(cx: &App) {
    let text_system = cx.text_system();
    // A family that does not exist resolves to a fallback font, whose family
    // is another one.
    let exists = |family: &SharedString| {
        let resolved = text_system.resolve_font(&font(family.clone()));
        text_system
            .get_font_for_id(resolved)
            .is_some_and(|resolved| resolved.family == *family)
    };

    let candidates = mono_font_candidates();
    let family = candidates
        .iter()
        .find(|family| exists(family))
        .or(candidates.last())
        .cloned()
        .expect("there is always a candidate");

    log::info!(target: "runtime", "interface_font family={family}");
    let _ = MONO_FONT.set(family);
}

/// The interface font (`font-mono`).
pub fn mono_font() -> SharedString {
    MONO_FONT
        .get()
        .cloned()
        .unwrap_or_else(|| mono_font_candidates().remove(0))
}

/// Fonts for the characters the interface font lacks. macOS chooses these
/// itself; elsewhere they are the ones a browser falls back to.
pub fn mono_font_fallbacks() -> Option<FontFallbacks> {
    if cfg!(target_os = "macos") {
        return None;
    }

    Some(FontFallbacks::from_fonts(vec![
        "DejaVu Sans Mono".into(),
        "Noto Sans CJK JP".into(),
    ]))
}

pub fn sans_font() -> SharedString {
    "Inter".into()
}
