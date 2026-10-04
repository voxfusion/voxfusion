//! The timing of the wizard's transitions, which behave like CSS ones.

use gpui_kit::{Animation, ElementId};
use std::time::{Duration, Instant};

use crate::ui::motion::ease;
use crate::ui::widgets::animations_frozen;

/// A value on its way to a target. As with a CSS transition, a new target is
/// approached from wherever the value is at that moment.
#[derive(Debug, Clone, Copy)]
pub struct Tween {
    from: f32,
    to: f32,
    started: Instant,
    duration: Duration,
    /// Counts the targets given, so each one gets an animation of its own.
    generation: usize,
}

impl Tween {
    /// A value at rest that takes `duration` to reach each new target.
    pub fn resting(value: f32, duration: Duration) -> Self {
        Self {
            from: value,
            to: value,
            started: Instant::now(),
            duration,
            generation: 0,
        }
    }

    /// Heads for `to` from the current value.
    pub fn retarget(&mut self, to: f32) {
        self.retarget_at(to, Instant::now());
    }

    fn retarget_at(&mut self, to: f32, now: Instant) {
        if to == self.to {
            return;
        }

        let elapsed = now.duration_since(self.started).as_secs_f32();

        self.from = self.at((elapsed / self.duration.as_secs_f32()).min(1.));
        self.to = to;
        self.started = now;
        self.generation += 1;
    }

    /// Takes the value `to` at once.
    pub fn jump_to(&mut self, to: f32) {
        self.from = to;
        self.to = to;
    }

    /// The value when `progress` (0 to 1) of the duration has passed.
    fn at(&self, progress: f32) -> f32 {
        self.from + (self.to - self.from) * ease(progress)
    }

    /// The identity of the animation towards the current target. Elements
    /// animated by this value take it, so that a new target restarts them.
    pub fn animation_id(&self, name: &'static str) -> ElementId {
        (name, self.generation).into()
    }

    pub fn animation(&self) -> Animation {
        Animation::new(self.duration)
    }

    /// The value at `progress` of that animation. Frozen animations show
    /// where the value ends up.
    pub fn frame(&self, progress: f32) -> f32 {
        self.at(if animations_frozen() { 1. } else { progress })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_moves_from_where_it_rests_to_its_target() {
        let start = Instant::now();
        let mut tween = Tween::resting(1., Duration::from_millis(200));
        tween.retarget_at(0., start);

        assert_eq!(tween.at(0.), 1.);
        assert!(tween.at(1.).abs() < 1e-4);
    }

    #[test]
    fn a_new_target_is_approached_from_the_current_value() {
        let start = Instant::now();
        let mut tween = Tween::resting(0., Duration::from_millis(200));
        tween.retarget_at(32., start);

        // Halfway through, the value turns back.
        tween.retarget_at(0., start + Duration::from_millis(100));
        assert!((tween.at(0.) - 32. * ease(0.5)).abs() < 1e-3);
        assert!(tween.at(1.).abs() < 1e-3);

        // Long after the transition ended, it starts from the target.
        tween.retarget_at(8., start + Duration::from_secs(5));
        assert!(tween.at(0.).abs() < 1e-3);
    }

    #[test]
    fn only_a_new_target_starts_a_new_animation() {
        let mut tween = Tween::resting(672., Duration::from_millis(300));
        let resting = tween.animation_id("width");

        tween.retarget(672.);
        assert_eq!(tween.animation_id("width"), resting);

        tween.retarget(896.);
        assert_ne!(tween.animation_id("width"), resting);
    }

    #[test]
    fn a_jump_leaves_nothing_to_animate() {
        let mut tween = Tween::resting(0., Duration::from_millis(300));
        tween.retarget(37.);
        tween.jump_to(37.);

        assert_eq!(tween.at(0.), 37.);
        assert_eq!(tween.at(1.), 37.);
    }
}
