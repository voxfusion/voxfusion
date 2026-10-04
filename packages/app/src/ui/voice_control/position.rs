//! Where the overlay sits: centered along the bottom of the display the
//! cursor is on.
//!
//! Everything is in logical coordinates. macOS interprets device pixel
//! positions with the window's current scale, which still belongs to the
//! previous display while the window moves between a Retina and an external
//! one.

use crate::platform::overlay_window::ScreenRect;

pub const WINDOW_HEIGHT: f64 = 28.;
const BOTTOM_PADDING: f64 = 20.;

/// The display the overlay belongs on: the one under the cursor, or the
/// primary one (the first) when the cursor's display has just gone.
pub fn display_for_cursor(
    cursor: Option<(f64, f64)>,
    displays: &[ScreenRect],
) -> Option<ScreenRect> {
    let under_cursor = cursor.and_then(|(x, y)| {
        displays.iter().find(|display| {
            x >= display.x
                && x < display.x + display.width
                && y >= display.y
                && y < display.y + display.height
        })
    });

    under_cursor.or(displays.first()).copied()
}

/// The frame of an overlay `width` wide on `display`.
pub fn overlay_frame(display: ScreenRect, width: f64) -> ScreenRect {
    ScreenRect {
        x: display.x + (display.width - width) / 2.,
        y: display.y + display.height - WINDOW_HEIGHT - BOTTOM_PADDING,
        width,
        height: WINDOW_HEIGHT,
    }
}

/// The frame to move the overlay to, or `None` when `current` is already it.
///
/// The window's actual frame is compared rather than a remembered one: a
/// replacement display can have the same origin but other dimensions, and
/// macOS moves windows itself when their display disconnects.
pub fn frame_correction(
    display: ScreenRect,
    width: f64,
    current: ScreenRect,
    scale_factor: f64,
) -> Option<ScreenRect> {
    let target = overlay_frame(display, width);
    // Frames read back from a window are rounded to device pixels.
    let tolerance = 0.5 / scale_factor;
    let close = |a: f64, b: f64| (a - b).abs() <= tolerance;

    let in_place = close(current.x, target.x)
        && close(current.y, target.y)
        && close(current.width, target.width)
        && close(current.height, target.height);

    (!in_place).then_some(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A display given in device pixels, as the system reports it.
    fn display(width: f64, height: f64, scale_factor: f64, x: f64, y: f64) -> ScreenRect {
        ScreenRect {
            x: x / scale_factor,
            y: y / scale_factor,
            width: width / scale_factor,
            height: height / scale_factor,
        }
    }

    /// A window that, like a real one, reports its position rounded to
    /// device pixels.
    struct Window {
        frame: ScreenRect,
        scale_factor: f64,
        moves: Vec<(f64, f64)>,
        fail_next_move: bool,
    }

    impl Window {
        fn new() -> Self {
            Self {
                frame: ScreenRect {
                    x: 0.,
                    y: 0.,
                    width: 100.,
                    height: WINDOW_HEIGHT,
                },
                scale_factor: 1.,
                moves: Vec::new(),
                fail_next_move: false,
            }
        }

        fn reported_frame(&self) -> ScreenRect {
            let snap = |value: f64| (value * self.scale_factor).round() / self.scale_factor;

            ScreenRect {
                x: snap(self.frame.x),
                y: snap(self.frame.y),
                ..self.frame
            }
        }

        /// One placement check, as the overlay runs it every second.
        fn check(
            &mut self,
            cursor_display: Option<ScreenRect>,
            primary: Option<ScreenRect>,
            width: f64,
        ) {
            let displays: Vec<ScreenRect> = match (cursor_display, primary) {
                (Some(display), Some(primary)) if display != primary => vec![primary, display],
                (Some(display), _) => vec![display],
                (None, Some(primary)) => vec![primary],
                (None, None) => Vec::new(),
            };
            let cursor = cursor_display.map(|display| (display.x + 200., display.y + 200.));

            let Some(display) = display_for_cursor(cursor, &displays) else {
                return;
            };
            let Some(target) =
                frame_correction(display, width, self.reported_frame(), self.scale_factor)
            else {
                return;
            };

            if self.fail_next_move {
                self.fail_next_move = false;
                return;
            }

            self.moves.push((target.x, target.y));
            self.frame = target;
        }

        fn position(&self) -> (f64, f64) {
            (self.frame.x, self.frame.y)
        }
    }

    #[test]
    fn recenters_after_unplugging_a_larger_display_with_the_same_origin() {
        let mut window = Window::new();
        let external = display(2560., 1440., 1., 0., 0.);
        window.check(Some(external), Some(external), 100.);
        assert_eq!(window.position(), (1230., 1392.));

        let built_in = display(3024., 1964., 2., 0., 0.);
        window.scale_factor = 2.;
        window.check(Some(built_in), Some(built_in), 100.);
        assert_eq!(window.position(), (706., 934.));
    }

    #[test]
    fn follows_resolution_and_scale_changes_without_an_origin_change() {
        let mut window = Window::new();
        let first = display(2560., 1440., 1., 0., 0.);
        window.check(Some(first), Some(first), 100.);

        let smaller = display(1920., 1080., 1., 0., 0.);
        window.check(Some(smaller), Some(smaller), 100.);
        assert_eq!(window.position(), (910., 1032.));

        let scaled = display(1920., 1080., 2., 0., 0.);
        window.scale_factor = 2.;
        window.check(Some(scaled), Some(scaled), 100.);
        assert_eq!(window.position(), (430., 492.));
    }

    #[test]
    fn moves_to_an_offset_retina_display_while_the_window_has_its_old_scale() {
        let mut window = Window::new();
        let primary = display(2560., 1440., 1., 0., 0.);
        window.check(Some(primary), Some(primary), 100.);

        let retina = display(3024., 1964., 2., -3024., -400.);
        window.check(Some(retina), Some(primary), 100.);
        assert_eq!(window.position(), (-806., 734.));

        window.scale_factor = 2.;
        window.check(Some(retina), Some(primary), 100.);
        assert_eq!(window.moves.len(), 2);
    }

    #[test]
    fn repairs_a_move_by_the_system_on_an_unchanged_display() {
        let mut window = Window::new();
        let primary = display(2560., 1440., 1., 0., 0.);
        window.check(Some(primary), Some(primary), 100.);

        window.frame.x = 1400.;
        window.frame.y = 1300.;
        window.check(Some(primary), Some(primary), 100.);
        assert_eq!(window.position(), (1230., 1392.));
        assert_eq!(window.moves.len(), 2);
    }

    #[test]
    fn keeps_compact_hands_free_and_error_widths_centered() {
        let mut window = Window::new();
        let primary = display(2560., 1440., 1., 0., 0.);

        for (width, x) in [(100., 1230.), (140., 1210.), (260., 1150.)] {
            window.check(Some(primary), Some(primary), width);
            assert_eq!(window.position(), (x, 1392.));
            assert_eq!(window.frame.width, width);
        }
    }

    #[test]
    fn leaves_a_centered_window_alone_despite_pixel_rounding() {
        let mut window = Window::new();
        let odd = display(1921., 1080., 1., 0., 0.);
        window.check(Some(odd), Some(odd), 100.);
        window.check(Some(odd), Some(odd), 100.);
        assert_eq!(window.moves.len(), 1);
    }

    #[test]
    fn falls_back_to_the_primary_display_when_the_cursor_display_disappears() {
        let mut window = Window::new();
        window.scale_factor = 2.;
        window.check(None, Some(display(3024., 1964., 2., 0., 0.)), 100.);
        assert_eq!(window.position(), (706., 934.));
    }

    #[test]
    fn waits_for_a_later_check_when_no_display_is_available() {
        let mut window = Window::new();
        window.check(None, None, 100.);
        assert!(window.moves.is_empty());

        let built_in = display(3024., 1964., 2., 0., 0.);
        window.scale_factor = 2.;
        window.check(Some(built_in), Some(built_in), 100.);
        assert_eq!(window.position(), (706., 934.));
    }

    #[test]
    fn repeats_a_move_that_did_not_happen_on_the_next_check() {
        let mut window = Window::new();
        let primary = display(2560., 1440., 1., 0., 0.);
        window.fail_next_move = true;
        window.check(Some(primary), Some(primary), 100.);
        assert!(window.moves.is_empty());

        window.check(Some(primary), Some(primary), 100.);
        assert_eq!(window.position(), (1230., 1392.));
    }

    #[test]
    fn picks_the_display_under_the_cursor() {
        let primary = display(2560., 1440., 1., 0., 0.);
        let left = display(1920., 1080., 1., -1920., 0.);
        let displays = [primary, left];

        assert_eq!(
            display_for_cursor(Some((-10., 500.)), &displays),
            Some(left)
        );
        assert_eq!(display_for_cursor(Some((0., 0.)), &displays), Some(primary));
        assert_eq!(
            display_for_cursor(Some((5000., 0.)), &displays),
            Some(primary)
        );
        assert_eq!(display_for_cursor(None, &displays), Some(primary));
    }
}
