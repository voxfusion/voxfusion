//! The measures of the pill when it shows an error.
//!
//! The message and the retry label are as wide as their text, which is a
//! fraction of a pixel that GPUI rounds up box by box. A browser carries the
//! fractions along the row and rounds every edge where it ends up. The pill
//! is measured the browser's way here, so that its boxes sit on the same
//! device pixels.

pub const BORDER: f32 = 1.;
pub const PADDING: f32 = 8.;
pub const ICON_SIZE: f32 = 11.;
pub const GAP: f32 = 6.;
pub const BUTTON_SIZE: f32 = 16.;
pub const RETRY_PADDING: f32 = 6.;

/// A browser's layout unit, in device pixels.
const LAYOUT_UNITS: f32 = 64.;

/// Lengths in logical pixels, each a whole number of device pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ErrorPillLayout {
    /// The pill's distance from the window's left edge.
    pub left: f32,
    pub message_width: f32,
    /// The retry button's width, borders and padding included.
    pub retry_width: Option<f32>,
}

/// Lays out the error pill in a window `window_width` wide, for a message
/// and a retry label whose texts have the given widths.
pub fn error_pill_layout(
    window_width: f32,
    message_width: f32,
    retry_label_width: Option<f32>,
    scale_factor: f32,
) -> ErrorPillLayout {
    // Everything below is in layout units. Text is given the next whole unit.
    let units = |length: f32| length * scale_factor * LAYOUT_UNITS;
    let nearest_pixel = |units: f32| (units / LAYOUT_UNITS + 0.5).floor();

    let message_start = units(BORDER + PADDING + ICON_SIZE + GAP);
    let message = units(message_width).ceil();
    let retry =
        retry_label_width.map(|label| units(label).ceil() + units(2. * (BORDER + RETRY_PADDING)));
    let after_message = retry.map_or(0., |retry| retry + units(GAP));
    let width = message_start
        + message
        + units(GAP)
        + after_message
        + units(BUTTON_SIZE + PADDING + BORDER);

    // Centered; a pill wider than the window starts at its left edge.
    let left = ((units(window_width) - width) / 2.).floor().max(0.);
    let message_left = left + message_start;
    let message_right = message_left + message;
    let retry_left = message_right + units(GAP);

    let pixels = |device_pixels: f32| device_pixels / scale_factor;
    let gap = nearest_pixel(units(GAP));

    ErrorPillLayout {
        left: pixels(nearest_pixel(left)),
        message_width: pixels(nearest_pixel(retry_left) - gap - nearest_pixel(message_left)),
        retry_width: retry
            .map(|retry| pixels(nearest_pixel(retry_left + retry) - nearest_pixel(retry_left))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "Transcription failed" and "RETRY" in the 10px monospace font.
    const MESSAGE: f32 = 120.0195;
    const RETRY: f32 = 32.5049;

    #[test]
    fn edges_are_rounded_where_the_fractions_put_them() {
        // The message runs from 82.47 to 322.52 device pixels and the button
        // from 334.52 to 427.53: boxes 241 and 93 device pixels wide.
        assert_eq!(
            error_pill_layout(260., MESSAGE, Some(RETRY), 2.),
            ErrorPillLayout {
                left: 15.,
                message_width: 120.5,
                retry_width: Some(46.5),
            }
        );
    }

    #[test]
    fn a_pill_without_retry_is_centered_on_its_own_width() {
        // "Recording failed": 16 characters.
        let layout = error_pill_layout(260., 96.0156, None, 2.);

        // 306.03 device pixels wide: the pill starts at 106.98, the message
        // runs from 158.98 to 351.02.
        assert_eq!(layout.left, 53.5);
        assert_eq!(layout.message_width, 96.);
        assert_eq!(layout.retry_width, None);
    }

    #[test]
    fn a_pill_wider_than_the_window_starts_at_its_left_edge() {
        let layout = error_pill_layout(260., 162.03, Some(71.51), 2.);

        assert_eq!(layout.left, 0.);
    }

    #[test]
    fn whole_pixels_are_kept_at_a_scale_of_one() {
        let layout = error_pill_layout(260., MESSAGE, Some(RETRY), 1.);

        // Left 15.23, message from 41.23 to 161.27, button to 213.77.
        assert_eq!(
            layout,
            ErrorPillLayout {
                left: 15.,
                message_width: 120.,
                retry_width: Some(47.),
            }
        );
    }
}
