//! The dot-matrix spinner shown while a recording is transcribed: a five by
//! five matrix of dots, around whose edge three pulses travel.

use gpui_kit::{
    Animation, AnimationExt as _, Bounds, Corners, Hsla, IntoElement, ParentElement as _,
    Styled as _, canvas, div, fill, point, px, size,
};
use std::time::Duration;

use crate::ui::widgets::animations_frozen;

const MATRIX_SIZE: usize = 5;
const CENTER: usize = MATRIX_SIZE / 2;
/// The dots the pulses travel along, in order.
const RING_PATH: [usize; 12] = [1, 2, 3, 9, 14, 19, 23, 22, 21, 15, 10, 5];
const CYCLE: Duration = Duration::from_millis(1200);

/// The opacity a dot pulses through, at equal intervals; three pulses a cycle.
const PULSE: [f32; 4] = [1., 0.7, 0.29, 0.18];
/// How many steps the opacity takes from one value of the pulse to the next.
const STEPS: f32 = 12.;

/// The opacity of a dot at rest: the matrix shows as a faint disc with a
/// stronger center.
fn resting_opacity(index: usize) -> f32 {
    let (row, column) = (index / MATRIX_SIZE, index % MATRIX_SIZE);
    let distance = (row.abs_diff(CENTER) as f32).hypot(column.abs_diff(CENTER) as f32);

    if distance > CENTER as f32 {
        0.
    } else if row == CENTER && column == CENTER {
        0.18
    } else {
        0.08
    }
}

/// The opacity of a pulsing dot `progress` of the way through its cycle.
fn pulse_opacity(progress: f32) -> f32 {
    let position = progress.rem_euclid(1.) * RING_PATH.len() as f32;
    let interval = position.floor() as usize;
    let from = PULSE[interval % PULSE.len()];
    // The cycle ends on the pulse's last value instead of starting the next.
    let to = if interval + 1 == RING_PATH.len() {
        from
    } else {
        PULSE[(interval + 1) % PULSE.len()]
    };
    let step = (position.fract() * STEPS).floor() / STEPS;

    from + (to - from) * step
}

/// The opacity of dot `index`. `cycle` is how far the spinner is through its
/// cycle; without it the dots are at rest.
fn dot_opacity(index: usize, cycle: Option<f32>) -> f32 {
    let ring_position = RING_PATH.iter().position(|dot| *dot == index);

    match (cycle, ring_position) {
        // Each dot of the ring runs the cycle later than the one before it.
        (Some(cycle), Some(position)) => {
            pulse_opacity(cycle - position as f32 / RING_PATH.len() as f32)
        }
        _ => resting_opacity(index),
    }
}

fn dots(dot_size: f32, pitch: f32, color: Hsla, cycle: Option<f32>) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let scale_factor = window.scale_factor();
            // Dots start on whole device pixels, halves rounded upwards, as
            // a browser places boxes.
            let snap = |offset: f32| (offset * scale_factor + 0.5).floor() / scale_factor;

            for index in 0..MATRIX_SIZE * MATRIX_SIZE {
                let opacity = dot_opacity(index, cycle);
                if opacity == 0. {
                    continue;
                }

                let (row, column) = (index / MATRIX_SIZE, index % MATRIX_SIZE);
                let origin = bounds.origin
                    + point(
                        px(snap(column as f32 * pitch)),
                        px(snap(row as f32 * pitch)),
                    );

                window.paint_quad(
                    fill(
                        Bounds::new(origin, size(px(dot_size), px(dot_size))),
                        color.opacity(opacity),
                    )
                    .corner_radii(Corners::all(px(dot_size / 2.))),
                );
            }
        },
    )
    .size_full()
}

/// A spinner `size` wide and high, of dots `dot_size` across.
pub fn dot_matrix_spinner(size: f32, dot_size: f32, color: Hsla) -> impl IntoElement {
    let gap = (size - dot_size * MATRIX_SIZE as f32) / (MATRIX_SIZE - 1) as f32;
    let pitch = dot_size + gap;
    let frame = div().size(px(size)).flex_none();

    if animations_frozen() {
        return frame
            .child(dots(dot_size, pitch, color, None))
            .into_any_element();
    }

    frame
        .with_animation(
            "dot-matrix-spinner",
            Animation::new(CYCLE).repeat(),
            move |frame, cycle| frame.child(dots(dot_size, pitch, color, Some(cycle))),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-4,
            "{actual} is not {expected}"
        );
    }

    #[test]
    fn dots_at_rest_form_a_disc_with_a_stronger_center() {
        let resting: Vec<f32> = (0..25).map(resting_opacity).collect();

        #[rustfmt::skip]
        let expected = [
            0.,   0.,   0.08, 0.,   0.,
            0.,   0.08, 0.08, 0.08, 0.,
            0.08, 0.08, 0.18, 0.08, 0.08,
            0.,   0.08, 0.08, 0.08, 0.,
            0.,   0.,   0.08, 0.,   0.,
        ];
        assert_eq!(resting, expected);
    }

    #[test]
    fn a_pulse_fades_in_steps_and_returns_three_times_a_cycle() {
        assert_close(pulse_opacity(0.), 1.);
        assert_close(pulse_opacity(1. / 12.), 0.7);
        assert_close(pulse_opacity(2. / 12.), 0.29);
        assert_close(pulse_opacity(3. / 12.), 0.18);
        assert_close(pulse_opacity(4. / 12.), 1.);
        assert_close(pulse_opacity(8. / 12.), 1.);

        // Between two values the opacity moves in twelfths of their distance.
        assert_close(pulse_opacity(0.3 / 12.), 1. - 0.3 * 3. / 12.);
        assert_close(pulse_opacity(0.32 / 12.), 1. - 0.3 * 3. / 12.);
        assert_close(pulse_opacity(0.35 / 12.), 1. - 0.3 * 4. / 12.);
        assert_close(pulse_opacity(3.3 / 12.), 0.18 + 0.82 * 3. / 12.);
    }

    #[test]
    fn the_cycle_ends_faded_out() {
        assert_close(pulse_opacity(11. / 12.), 0.18);
        assert_close(pulse_opacity(11.9 / 12.), 0.18);
    }

    #[test]
    fn each_ring_dot_trails_the_one_before_it() {
        let cycle = 0.5;

        assert_close(dot_opacity(RING_PATH[0], Some(cycle)), pulse_opacity(0.5));
        assert_close(
            dot_opacity(RING_PATH[3], Some(cycle)),
            pulse_opacity(0.5 - 3. / 12.),
        );
        // Before its own start a dot is in the previous cycle.
        assert_close(
            dot_opacity(RING_PATH[9], Some(cycle)),
            pulse_opacity(0.5 + 3. / 12.),
        );
    }

    #[test]
    fn dots_off_the_ring_never_pulse() {
        assert_eq!(dot_opacity(12, Some(0.3)), 0.18);
        assert_eq!(dot_opacity(7, Some(0.3)), 0.08);
        assert_eq!(dot_opacity(0, Some(0.3)), 0.);
    }

    #[test]
    fn a_frozen_spinner_shows_every_dot_at_rest() {
        for index in 0..25 {
            assert_eq!(dot_opacity(index, None), resting_opacity(index));
        }
    }
}
