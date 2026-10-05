//! The pictures that stand for an app or a site in a list.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use gpui_kit::{
    App, Image, ImageFormat, IntoElement, ParentElement as _, Pixels, Styled as _, div, img,
    prelude::*,
};
use std::collections::HashMap;
use std::sync::Arc;

use crate::backend::InstalledApp;
use crate::ui::text::{TypeScale as _, text};
use crate::ui::theme::palette;
use crate::ui::widgets::icon;

/// The picture inside a base64 `data:` URL: a PNG, or an SVG, which is
/// all some Linux apps have for an icon.
fn decode_image_data_url(url: &str) -> Option<Image> {
    let (format, data) = if let Some(data) = url.strip_prefix("data:image/png;base64,") {
        (ImageFormat::Png, data)
    } else {
        (ImageFormat::Svg, url.strip_prefix("data:image/svg+xml;base64,")?)
    };
    Some(Image::from_bytes(format, STANDARD.decode(data).ok()?))
}

/// The icons of the installed apps, by bundle id.
#[derive(Default)]
pub struct AppIcons {
    by_bundle_id: HashMap<String, Arc<Image>>,
}

impl AppIcons {
    pub fn decode(apps: &[InstalledApp]) -> Self {
        let by_bundle_id = apps
            .iter()
            .filter_map(|app| {
                let image = decode_image_data_url(app.icon_data_url.as_deref()?)?;

                Some((app.bundle_id.clone(), Arc::new(image)))
            })
            .collect();

        Self { by_bundle_id }
    }

    pub fn get(&self, bundle_id: &str) -> Option<Arc<Image>> {
        self.by_bundle_id.get(bundle_id).cloned()
    }

    pub fn images(&self) -> impl Iterator<Item = Arc<Image>> + '_ {
        self.by_bundle_id.values().cloned()
    }
}

/// An app's icon, or the first letter of its name when it has none.
pub fn app_icon(image: Option<Arc<Image>>, name: &str, cx: &App) -> impl IntoElement + use<> {
    let p = palette(cx);

    match image {
        Some(image) => img(image).size_8().flex_shrink_0().into_any_element(),
        None => div()
            .size_8()
            .flex_shrink_0()
            .bg(p.input)
            .border_1()
            .border_color(p.border)
            .flex()
            .items_center()
            .justify_center()
            .type_xs()
            .text_color(p.txt_muted)
            .child(text(initial(name).unwrap_or_default()).center())
            .into_any_element(),
    }
}

/// The first character of `name`, in capitals.
fn initial(name: &str) -> Option<String> {
    name.chars()
        .next()
        .map(|first| first.to_uppercase().collect())
}

/// The square that stands for a site: the first letter of its domain, or a
/// globe while there is no domain. The letter is drawn here because fetching
/// even a favicon would disclose the configured domain.
pub fn site_icon(domain: Option<&str>, size: Pixels, cx: &App) -> impl IntoElement + use<> {
    let p = palette(cx);

    div()
        .size(size)
        .flex_shrink_0()
        .bg(p.input)
        .border_1()
        .border_color(p.border)
        .flex()
        .items_center()
        .justify_center()
        .overflow_hidden()
        .map(
            |square| match domain.and_then(|domain| initial(domain.trim())) {
                Some(letter) => square.child(
                    div()
                        .font_sans()
                        .font_weight(gpui_kit::FontWeight::MEDIUM)
                        .type_xs()
                        .text_color(p.txt_primary)
                        .child(text(letter).center()),
                ),
                None => square.child(icon("globe").size_3p5().text_color(p.txt_muted)),
            },
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded(url: &str) -> Option<(ImageFormat, Vec<u8>)> {
        decode_image_data_url(url).map(|image| (image.format(), image.bytes().to_vec()))
    }

    #[test]
    fn a_png_data_url_decodes_to_its_bytes() {
        assert_eq!(
            decoded("data:image/png;base64,iVBORw0KGgo="),
            Some((
                ImageFormat::Png,
                vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]
            ))
        );
    }

    #[test]
    fn an_svg_data_url_decodes_to_its_markup() {
        assert_eq!(
            decoded("data:image/svg+xml;base64,PHN2Zy8+"),
            Some((ImageFormat::Svg, b"<svg/>".to_vec()))
        );
    }

    #[test]
    fn anything_else_has_no_icon() {
        assert_eq!(decoded("data:image/jpeg;base64,AAAA"), None);
        assert_eq!(decoded("data:image/png;base64,not base64"), None);
        assert_eq!(decoded("https://example.com/icon.png"), None);
    }

    #[test]
    fn the_initial_is_the_first_character_in_capitals() {
        assert_eq!(initial("github.com").as_deref(), Some("G"));
        assert_eq!(initial("Slack").as_deref(), Some("S"));
        assert_eq!(initial("ßeta.de").as_deref(), Some("SS"));
        assert_eq!(initial("пример.рф").as_deref(), Some("П"));
        assert_eq!(initial(""), None);
    }
}
