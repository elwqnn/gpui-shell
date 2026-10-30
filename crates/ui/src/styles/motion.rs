//! [`Motion`] - semantic durations and shared easing curves.
//!
//! Every animation in the shell picks its duration from this ladder rather
//! than spelling out a [`Duration`], so the whole surface accelerates and
//! settles at the same rate.
//!
//! ## The `delta` clamp
//!
//! gpui asserts that an easing function stays within `0.0..=1.0`
//! (`gpui::elements::animation`), so a spring that overshoots its target
//! *cannot* be passed to [`Animation::with_easing`]. [`spring`] is therefore
//! applied inside the animator closure, where nothing clamps it:
//!
//! ```ignore
//! el.with_animation(
//!     "knob",
//!     Animation::new(Motion::Quick.duration()),
//!     move |el, delta| el.left(travel * motion::spring(delta)),
//! )
//! ```
//!
//! [`Animation::with_easing`]: gpui::Animation::with_easing

use std::time::Duration;

/// A decelerating curve for one-shot transitions - fast out of the gate, easing
/// into the target. The workhorse for anything that moves once and stops.
pub use gpui::ease_out_quint as decelerate;

/// Semantic animation duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Motion {
    // -- One-shot: a state change playing out once. --
    /// 150ms - a small local flip: a knob sliding, a checkmark landing.
    Quick,
    /// 220ms - a shape or size morph, a surface arriving.
    Standard,

    // -- Ambient: loops that run while something is pending. --
    /// 1s - indeterminate rotation.
    Spin,
    /// 1.5s - a placeholder breathing while content loads.
    Pulse,
    /// 2s - slow background motion that must not draw the eye.
    Drift,
}

impl Motion {
    pub const fn duration(self) -> Duration {
        Duration::from_millis(self.millis())
    }

    /// The raw millisecond count.
    pub const fn millis(self) -> u64 {
        match self {
            Self::Quick => 150,
            Self::Standard => 220,
            Self::Spin => 1000,
            Self::Pulse => 1500,
            Self::Drift => 2000,
        }
    }
}

/// How fast the oscillation dies out. Higher settles sooner, overshoots less.
const SPRING_DECAY: f32 = 3.5;
/// Angular frequency, tuned so the single overshoot peaks near the midpoint.
const SPRING_FREQUENCY: f32 = 5.65;

/// A damped-spring progress curve: overshoots its target by ~6%, then settles.
///
/// Takes the animation's `delta` and returns a progress multiplier. The
/// `(1.0 - t)` factor pins the endpoints exactly, so the animated value lands
/// on its target rather than a rounding error away from it.
///
/// The return value exceeds `1.0` around the peak - see the module docs for
/// why that keeps it out of `with_easing`.
pub fn spring(t: f32) -> f32 {
    1.0 - (1.0 - t) * (-SPRING_DECAY * t).exp() * (SPRING_FREQUENCY * t).cos()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motion_ladder_is_monotonically_increasing() {
        let steps = [
            Motion::Quick,
            Motion::Standard,
            Motion::Spin,
            Motion::Pulse,
            Motion::Drift,
        ];
        for window in steps.windows(2) {
            assert!(
                window[0].millis() < window[1].millis(),
                "{:?} ({}ms) should be shorter than {:?} ({}ms)",
                window[0],
                window[0].millis(),
                window[1],
                window[1].millis(),
            );
        }
    }

    /// An animated value must start where it was and land exactly on target,
    /// or a switch knob settles half a pixel off its track.
    #[test]
    fn spring_pins_both_endpoints() {
        assert_eq!(spring(0.0), 0.0);
        assert_eq!(spring(1.0), 1.0);
    }

    /// The overshoot is the whole point, but it has to stay a flourish: past
    /// ~15% it reads as a bug rather than as weight.
    #[test]
    fn spring_overshoots_once_and_modestly() {
        let peak = (0..=100)
            .map(|i| spring(i as f32 / 100.0))
            .fold(f32::MIN, f32::max);
        assert!(peak > 1.0, "spring should overshoot, peaked at {peak}");
        assert!(peak < 1.15, "overshoot too violent: {peak}");
    }

    /// `with_easing` clamps to `0.0..=1.0`; the spring is only ever safe
    /// inside an animator closure, and undershooting below zero would make it
    /// unusable there too.
    #[test]
    fn spring_never_goes_negative() {
        for i in 0..=100 {
            let t = i as f32 / 100.0;
            assert!(spring(t) >= 0.0, "spring({t}) went negative");
        }
    }
}
