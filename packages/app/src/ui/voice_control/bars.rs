//! The level bars: a wave that travels along ten bars and grows with the
//! voice.

use gpui_kit::{Bounds, Corners, Hsla, IntoElement, Styled as _, canvas, fill, point, px};
use std::time::Duration;

pub const NUM_BARS: usize = 10;
const BAR_WIDTH: f32 = 2.;
const BAR_GAP: f32 = 2.;
const BAR_BASE_HEIGHT: f32 = 4.;
const BAR_MAX_HEIGHT: f32 = 18.;
const BAR_IDLE_AMPLITUDE: f32 = 2.;
const BAR_VOICE_SCALE: f32 = 2.5;

/// How often the wave moves on by one bar.
pub const WAVE_STEP: Duration = Duration::from_millis(110);
/// How long a bar takes to reach a new height.
pub const HEIGHT_TRANSITION: Duration = Duration::from_millis(200);

const WAVE_PATTERN: [f32; 16] = [
    0.25, 0.45, 0.7, 0.9, 1.0, 0.95, 0.75, 0.5, 0.3, 0.4, 0.65, 0.85, 1.0, 0.9, 0.65, 0.35,
];

pub fn next_wave_offset(wave_offset: usize) -> usize {
    (wave_offset + 1) % WAVE_PATTERN.len()
}

/// The height of each bar. They rest at the base height while a recording is
/// being transcribed.
pub fn bar_heights(wave_offset: usize, audio_level: f32, loading: bool) -> [f32; NUM_BARS] {
    std::array::from_fn(|index| {
        if loading {
            return BAR_BASE_HEIGHT;
        }

        let factor = WAVE_PATTERN[(wave_offset + index) % WAVE_PATTERN.len()];
        let amplitude = BAR_IDLE_AMPLITUDE + audio_level * BAR_VOICE_SCALE;

        (BAR_BASE_HEIGHT + factor * amplitude).min(BAR_MAX_HEIGHT)
    })
}

/// A browser's layout unit, in device pixels.
const LAYOUT_UNITS: f32 = 64.;

/// The device pixel rows a bar `height` tall covers when centered in
/// `available` device pixels.
///
/// The rows are the ones a browser paints: it lays out in 64ths of a device
/// pixel, cutting off the rest, and paints a box between its edges rounded to
/// whole pixels, halves upwards. GPUI's own snapping rounds the height first
/// and halves downwards, which puts every other height a pixel off.
fn bar_rows(height: f32, available: f32, scale_factor: f32) -> (f32, f32) {
    let height = (height * scale_factor * LAYOUT_UNITS).floor();
    let top = ((available * LAYOUT_UNITS - height) / 2.).floor();
    let nearest_pixel = |units: f32| (units / LAYOUT_UNITS + 0.5).floor();

    (nearest_pixel(top), nearest_pixel(top + height))
}

/// The row of bars, as tall as its container.
pub fn bars(heights: [f32; NUM_BARS], color: Hsla) -> impl IntoElement {
    let width = NUM_BARS as f32 * BAR_WIDTH + (NUM_BARS - 1) as f32 * BAR_GAP;

    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let scale_factor = window.scale_factor();
            let available = bounds.size.height.as_f32() * scale_factor;

            for (index, height) in heights.into_iter().enumerate() {
                let (top, bottom) = bar_rows(height, available, scale_factor);
                let left = bounds.left() + px(index as f32 * (BAR_WIDTH + BAR_GAP));

                window.paint_quad(
                    fill(
                        Bounds::from_corners(
                            point(left, bounds.top() + px(top / scale_factor)),
                            point(
                                left + px(BAR_WIDTH),
                                bounds.top() + px(bottom / scale_factor),
                            ),
                        ),
                        color,
                    )
                    .corner_radii(Corners::all(px(BAR_WIDTH / 2.))),
                );
            }
        },
    )
    .w(px(width))
    .flex_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_heights(actual: [f32; NUM_BARS], expected: [f32; NUM_BARS]) {
        for (actual, expected) in actual.iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 1e-4,
                "{actual} is not {expected}"
            );
        }
    }

    #[test]
    fn silence_leaves_a_shallow_wave() {
        assert_heights(
            bar_heights(0, 0., false),
            [4.5, 4.9, 5.4, 5.8, 6.0, 5.9, 5.5, 5.0, 4.6, 4.8],
        );
    }

    #[test]
    fn the_voice_deepens_the_wave() {
        assert_heights(
            bar_heights(0, 2., false),
            [5.75, 7.15, 8.9, 10.3, 11.0, 10.65, 9.25, 7.5, 6.1, 6.8],
        );
    }

    #[test]
    fn bars_stop_at_the_full_height() {
        let heights = bar_heights(0, 9., false);

        assert_eq!(heights[4], 18.);
        assert_eq!(heights[0], 4. + 0.25 * 24.5);
        assert!(heights.iter().all(|height| *height <= 18.));
    }

    #[test]
    fn the_wave_travels_and_wraps_around() {
        assert_eq!(bar_heights(1, 0., false)[0], bar_heights(0, 0., false)[1]);
        assert_heights(
            bar_heights(14, 0., false),
            [5.3, 4.7, 4.5, 4.9, 5.4, 5.8, 6.0, 5.9, 5.5, 5.0],
        );
        assert_eq!(next_wave_offset(14), 15);
        assert_eq!(next_wave_offset(15), 0);
    }

    #[test]
    fn bars_rest_while_transcribing() {
        assert_eq!(bar_heights(5, 3., true), [4.; NUM_BARS]);
    }

    #[test]
    fn bars_cover_the_pixel_rows_a_browser_gives_them() {
        // An 18px row at two device pixels per pixel.
        let rows = |height: f32| bar_rows(height, 36., 2.);

        assert_eq!(rows(4.), (14., 22.));
        assert_eq!(rows(4.5), (14., 23.));
        assert_eq!(rows(4.9), (13., 23.));
        assert_eq!(rows(5.4), (13., 23.));
        assert_eq!(rows(5.5), (13., 24.));
        assert_eq!(rows(5.8), (12., 24.));
        assert_eq!(rows(18.), (0., 36.));
    }

    #[test]
    fn bars_are_centered_at_other_scales() {
        assert_eq!(bar_rows(4., 18., 1.), (7., 11.));
        assert_eq!(bar_rows(5., 18., 1.), (7., 12.));
        assert_eq!(bar_rows(6., 18., 1.), (6., 12.));
    }
}
