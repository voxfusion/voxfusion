//! A bar that fills up to a percentage. A new percentage is reached over
//! 300ms rather than at once, like the `transition-all duration-300` of the
//! design.

use gpui_kit::{
    Animation, AnimationExt as _, App, Div, ElementId, IntoElement as _, ParentElement as _,
    Pixels, Styled as _, div, relative,
};
use std::time::{Duration, Instant};

use crate::ui::motion::ease;
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
