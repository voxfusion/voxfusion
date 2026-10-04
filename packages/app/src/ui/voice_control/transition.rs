//! Values that move to their target over time, the way CSS transitions move
//! a property: a new target is approached from wherever the value is at that
//! moment.

use std::time::{Duration, Instant};

/// CSS `cubic-bezier(x1, y1, x2, y2)` at `progress` of the duration.
fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, progress: f32) -> f32 {
    // The ends are exact, so a finished transition rests on its target.
    if progress <= 0. {
        return 0.;
    }
    if progress >= 1. {
        return 1.;
    }

    let curve = |a: f32, b: f32, t: f32| {
        let inverse = 1. - t;
        3. * inverse * inverse * t * a + 3. * inverse * t * t * b + t * t * t
    };

    // The curve's x grows with its parameter, so bisection finds the parameter.
    let (mut low, mut high) = (0., 1.);
    for _ in 0..24 {
        let middle = (low + high) / 2.;
        if curve(x1, x2, middle) < progress {
            low = middle;
        } else {
            high = middle;
        }
    }

    curve(y1, y2, (low + high) / 2.)
}

/// Tailwind's `ease-out`.
pub fn ease_out(progress: f32) -> f32 {
    cubic_bezier(0., 0., 0.2, 1., progress)
}

/// `N` values in transition together.
#[derive(Debug, Clone)]
pub struct Transition<const N: usize> {
    from: [f32; N],
    to: [f32; N],
    started: Instant,
    duration: Duration,
    /// Counts the targets set, to tell one run of the transition from the next.
    run: usize,
}

impl<const N: usize> Transition<N> {
    /// Values at rest.
    pub fn at(values: [f32; N], duration: Duration) -> Self {
        Self {
            from: values,
            to: values,
            started: Instant::now(),
            duration,
            run: 0,
        }
    }

    /// Moves to `values` at once.
    pub fn jump_to(&mut self, values: [f32; N]) {
        self.from = values;
        self.to = values;
    }

    /// Starts moving to `target` from the values reached by `now`.
    pub fn retarget(&mut self, target: [f32; N], now: Instant) {
        if target == self.to {
            return;
        }

        self.from = self.values_at(now);
        self.to = target;
        self.started = now;
        self.run += 1;
    }

    pub fn duration(&self) -> Duration {
        self.duration
    }

    pub fn run(&self) -> usize {
        self.run
    }

    pub fn target(&self) -> [f32; N] {
        self.to
    }

    /// Whether the target has been reached by `now`.
    pub fn at_rest(&self, now: Instant) -> bool {
        self.from == self.to || now.saturating_duration_since(self.started) >= self.duration
    }

    /// The values `eased` of the way from where the transition started to
    /// its target.
    pub fn values(&self, eased: f32) -> [f32; N] {
        std::array::from_fn(|index| self.from[index] + (self.to[index] - self.from[index]) * eased)
    }

    fn values_at(&self, now: Instant) -> [f32; N] {
        let elapsed = now.saturating_duration_since(self.started);
        if elapsed >= self.duration {
            return self.to;
        }

        self.values(ease_out(
            elapsed.as_secs_f32() / self.duration.as_secs_f32(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DURATION: Duration = Duration::from_millis(200);

    #[test]
    fn ease_out_starts_fast_and_ends_at_the_target() {
        assert_eq!(ease_out(0.), 0.);
        assert_eq!(ease_out(1.), 1.);
        // cubic-bezier(0, 0, 0.2, 1) at a quarter and at half of the time.
        assert!((ease_out(0.25) - 0.5776).abs() < 1e-3);
        assert!((ease_out(0.5) - 0.8392).abs() < 1e-3);
    }

    #[test]
    fn a_resting_transition_stays_at_its_values() {
        let transition = Transition::at([4., 6.], DURATION);

        assert_eq!(transition.values(0.), [4., 6.]);
        assert_eq!(transition.values(1.), [4., 6.]);
        assert_eq!(transition.run(), 0);
    }

    #[test]
    fn a_new_target_is_approached_from_the_start_values() {
        let mut transition = Transition::at([4.], DURATION);
        transition.retarget([8.], transition.started);

        assert_eq!(transition.values(0.), [4.]);
        assert_eq!(transition.values(0.5), [6.]);
        assert_eq!(transition.values(1.), [8.]);
        assert_eq!(transition.run(), 1);
    }

    #[test]
    fn an_interrupted_transition_continues_from_where_it_was() {
        let mut transition = Transition::at([0.], DURATION);
        let start = transition.started;
        transition.retarget([10.], start);

        let midway = start + Duration::from_millis(100);
        let reached = transition.values_at(midway)[0];
        assert!((reached - 10. * ease_out(0.5)).abs() < 1e-4);

        transition.retarget([0.], midway);
        assert_eq!(transition.values(0.), [reached]);
        assert_eq!(transition.values(1.), [0.]);
        assert_eq!(transition.run(), 2);
    }

    #[test]
    fn a_finished_transition_restarts_from_its_target() {
        let mut transition = Transition::at([0.], DURATION);
        let start = transition.started;
        transition.retarget([10.], start);
        transition.retarget([20.], start + Duration::from_millis(500));

        assert_eq!(transition.values(0.), [10.]);
    }

    #[test]
    fn the_same_target_does_not_restart_the_transition() {
        let mut transition = Transition::at([0.], DURATION);
        let start = transition.started;
        transition.retarget([10.], start);
        transition.retarget([10.], start + Duration::from_millis(100));

        assert_eq!(transition.run(), 1);
        assert_eq!(transition.values(0.), [0.]);
    }

    #[test]
    fn a_transition_rests_once_its_duration_has_passed() {
        let mut transition = Transition::at([0.], DURATION);
        let start = transition.started;
        assert!(transition.at_rest(start));

        transition.retarget([10.], start);
        assert!(!transition.at_rest(start + Duration::from_millis(199)));
        assert!(transition.at_rest(start + Duration::from_millis(200)));
    }

    #[test]
    fn a_jump_skips_the_transition() {
        let mut transition = Transition::at([0.], DURATION);
        transition.jump_to([7.]);

        assert_eq!(transition.values(0.), [7.]);
        assert_eq!(transition.target(), [7.]);
    }
}
