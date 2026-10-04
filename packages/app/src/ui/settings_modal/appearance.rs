//! The Appearance section: the light, dark or system theme.

use gpui_kit::{
    App, Div, InteractiveElement as _, ParentElement as _, StatefulInteractiveElement as _,
    Styled as _, div, prelude::*, px, rgb,
};

use crate::analytics;
use crate::settings::{SettingsStore, ThemeMode};
use crate::ui::motion::{Faded, Transitions as _};
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;

const THEMES: [(ThemeMode, &str); 3] = [
    (ThemeMode::Light, "LIGHT"),
    (ThemeMode::Dark, "DARK"),
    (ThemeMode::System, "SYSTEM"),
];

/// The name analytics knows a theme by.
fn theme_name(theme: ThemeMode) -> &'static str {
    match theme {
        ThemeMode::Light => "light",
        ThemeMode::Dark => "dark",
        ThemeMode::System => "system",
    }
}

/// A little window in the light or the dark theme. These colors picture the
/// themes; they do not follow the one in effect.
fn preview(dark: bool, bar_width: f32) -> Div {
    let (background, border, bar) = if dark {
        (rgb(0x1a1a1a), rgb(0x333333), rgb(0x444444))
    } else {
        (rgb(0xe0e0e0), rgb(0xcccccc), rgb(0x999999))
    };

    div()
        .bg(background)
        .border_color(border)
        .flex()
        .items_center()
        .justify_center()
        .child(div().w(px(bar_width)).h_1p5().bg(bar))
}

fn theme_option(theme: ThemeMode, label: &'static str, cx: &App) -> Faded {
    let p = palette(cx);
    let selected = SettingsStore::get(cx).theme == theme;

    let picture = match theme {
        ThemeMode::Light => preview(false, 32.).border_1(),
        ThemeMode::Dark => preview(true, 32.).border_1(),
        ThemeMode::System => div()
            .flex()
            .overflow_hidden()
            .child(
                preview(false, 16.)
                    .w_1_2()
                    .border_l_1()
                    .border_t_1()
                    .border_b_1(),
            )
            .child(
                preview(true, 16.)
                    .w_1_2()
                    .border_r_1()
                    .border_t_1()
                    .border_b_1(),
            ),
    };

    div()
        .id(label)
        .transition_all()
        .relative()
        .p_4()
        .border_1()
        .map(|option| {
            if selected {
                option.border_color(p.ac).bg(p.ac_bg)
            } else {
                option
                    .border_color(p.border_strong)
                    .bg(p.surface)
                    .hover(|option| option.border_color(p.txt_faint))
            }
        })
        .on_click(move |_, _, cx| {
            analytics::capture(
                cx,
                "settings_theme_changed",
                &[("theme", theme_name(theme).into())],
            );
            SettingsStore::update(cx, |settings| settings.theme = theme);
        })
        .child(picture.w_full().h_12().mb_3())
        // The name is inline text in a button whose own font is the 16px
        // interface font: its line is 24px tall, and the name sits on the
        // baseline of that font.
        .child(
            div()
                .h_6()
                .pt(px(7.))
                .type_xs()
                .text_color(if selected { p.ac } else { p.txt_secondary })
                .child(text(label).tracking_wider().center()),
        )
        .when(selected, |option| {
            option.child(
                div()
                    .absolute()
                    .top_2()
                    .right_2()
                    .type_xs()
                    .text_color(p.ac)
                    .child(text("[*]")),
            )
        })
}

pub(super) fn render(cx: &App) -> Div {
    let p = palette(cx);

    div()
        .child(
            div()
                .mb_4()
                .type_xs()
                .text_color(p.txt_muted)
                .child(text("THEME_MODE").tracking_wider()),
        )
        .child(
            div()
                .grid()
                .grid_cols(3)
                .gap_4()
                .children(THEMES.map(|(theme, label)| theme_option(theme, label, cx))),
        )
}
