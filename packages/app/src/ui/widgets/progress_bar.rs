//! A bar that fills up to a percentage. A new percentage is reached over
//! 300ms rather than at once, like the `transition-all duration-300` of the
//! design.

use gpui_kit::{
    Animation, AnimationExt as _, App, Div, ElementId, IntoElement as _, ParentElement as _,
    Pixels, Styled as _, div, relative,
};
use std::time::{Duration, Instant};

use crate::ui::theme::palette;
use crate::ui::widgets::animations_frozen;

const DURATION: Duration = Duration::from_millis(300);

/// A percentage on its way to the latest value it was given.
pub struct Progress {
    from: f32,
    to: f32,
    changed_at: Instant,
    /// How many times the value changed. Each change is its own animation.
    changes: usize,
}

impl Progress {
    pub fn new(percent: f32) -> Self {
        Self {
            from: percent,
            to: percent,
            changed_at: Instant::now(),
            changes: 0,
        }
    }

    pub fn set(&mut self, percent: f32) {
        self.set_at(percent, Instant::now());
    }

    fn set_at(&mut self, percent: f32, now: Instant) {
        if percent == self.to {
            return;
        }

        // A change while the bar is still moving continues from where it is.
        self.from = self.shown_at(now);
        self.to = percent;
        self.changed_at = now;
        self.changes += 1;
    }

    fn shown_at(&self, now: Instant) -> f32 {
        let elapsed = now.saturating_duration_since(self.changed_at);
        let time = (elapsed.as_secs_f32() / DURATION.as_secs_f32()).min(1.);

        self.from + (self.to - self.from) * ease(time)
    }
}

/// Tailwind's transition timing, `cubic-bezier(0.4, 0, 0.2, 1)`: the eased
/// value for a share of the duration.
fn ease(time: f32) -> f32 {
    const X1: f32 = 0.4;
    const Y1: f32 = 0.;
    const X2: f32 = 0.2;
    const Y2: f32 = 1.;

    let curve = |parameter: f32, first: f32, second: f32| {
        let rest = 1. - parameter;
        3. * rest * rest * parameter * first
            + 3. * rest * parameter * parameter * second
            + parameter * parameter * parameter
    };

    // The curve gives time and value for a parameter; find the parameter for
    // this time. Time grows with the parameter, so bisecting converges.
    let (mut low, mut high) = (0., 1.);
    for _ in 0..24 {
        let middle = (low + high) / 2.;
        if curve(middle, X1, X2) < time {
            low = middle;
        } else {
            high = middle;
        }
    }

    curve((low + high) / 2., Y1, Y2)
}

/// A track `height` tall with its fill. `name` must be unique among the
/// siblings of the bar.
pub fn progress_bar(name: &'static str, progress: &Progress, height: Pixels, cx: &App) -> Div {
    let p = palette(cx);
    let (from, to) = (progress.from, progress.to);
    let fill = div().h_full().bg(p.ac);

    let fill = if animations_frozen() {
        fill.w(relative(to / 100.)).into_any_element()
    } else {
        fill.with_animation(
            ElementId::NamedInteger(name.into(), progress.changes as u64),
            Animation::new(DURATION).with_easing(ease),
            move |fill, eased| fill.w(relative((from + (to - from) * eased) / 100.)),
        )
        .into_any_element()
    };

    div()
        .w_full()
        .h(height)
        .bg(p.border)
        .overflow_hidden()
        .child(fill)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.001
    }

    #[test]
    fn easing_follows_the_css_curve() {
        assert!(close(ease(0.), 0.));
        assert!(close(ease(0.25), 0.2366));
        assert!(close(ease(0.5), 0.7756));
        assert!(close(ease(0.75), 0.9594));
        assert!(close(ease(1.), 1.));
    }

    #[test]
    fn a_new_value_is_reached_after_the_duration() {
        let start = Instant::now();
        let mut progress = Progress::new(0.);
        progress.set_at(40., start);

        assert!(close(progress.shown_at(start), 0.));
        assert!(close(
            progress.shown_at(start + DURATION / 2),
            40. * ease(0.5)
        ));
        assert!(close(progress.shown_at(start + DURATION), 40.));
        assert!(close(progress.shown_at(start + DURATION * 3), 40.));
    }

    #[test]
    fn a_change_on_the_way_continues_from_the_value_shown() {
        let start = Instant::now();
        let mut progress = Progress::new(0.);
        progress.set_at(40., start);

        let halfway = start + DURATION / 2;
        let shown = progress.shown_at(halfway);
        progress.set_at(80., halfway);

        assert!(close(progress.shown_at(halfway), shown));
        assert!(close(progress.shown_at(halfway + DURATION), 80.));
    }

    #[test]
    fn the_same_value_again_does_not_restart_the_move() {
        let start = Instant::now();
        let mut progress = Progress::new(10.);
        progress.set_at(10., start);

        assert_eq!(progress.changes, 0);
    }
}
