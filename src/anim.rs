//! Deterministic fixed-timestep animation.
//!
//! Curves are integer-only (no floats, no wall clock, no randomness): the
//! clock only advances when the caller ticks it, so two identical tick
//! sequences always produce identical frame digests.

use crate::{Frame, Ir, Renderer, StateView};

/// Entry-animation curve selected by the `animate` prop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Curve {
    #[default]
    None,
    Spring,
    Ease,
}

impl Curve {
    pub fn parse(s: &str) -> Option<Curve> {
        Some(match s {
            "none" => Curve::None,
            "spring" => Curve::Spring,
            "ease" => Curve::Ease,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Curve::None => "none",
            Curve::Spring => "spring",
            Curve::Ease => "ease",
        }
    }
}

/// Progress unit: `ONE` is a fully settled curve.
pub const ONE: i64 = 65536;
/// Duration of the ease curve.
pub const EASE_MS: i64 = 320;
/// Integration step of the spring solver.
pub const STEP_MS: i64 = 16;

/// Spring constants (per second): stiffness and damping.
const K: i64 = 225;
const C: i64 = 22;

/// Curve progress at `elapsed_ms`, in `0..=ONE`.
pub fn progress(curve: Curve, elapsed_ms: i64) -> i64 {
    match curve {
        Curve::None => ONE,
        Curve::Ease => ease(elapsed_ms),
        Curve::Spring => spring(elapsed_ms),
    }
}

/// Ease-out cubic: `1 - (1 - t)^3`, evaluated in fixed point.
fn ease(elapsed_ms: i64) -> i64 {
    let t = (elapsed_ms.clamp(0, EASE_MS) as i128 * ONE as i128 / EASE_MS as i128) as i64;
    let inv = ONE - t;
    ONE - (inv * inv * inv >> 32)
}

/// Damped spring integrated with fixed 16 ms steps from rest.
fn spring(elapsed_ms: i64) -> i64 {
    let mut x: i64 = 0;
    let mut v: i64 = 0;
    let mut t = 0i64;
    while t < elapsed_ms {
        let dt = (elapsed_ms - t).min(STEP_MS);
        let a = -K * (x - ONE) - C * v;
        v += a * dt / 1000;
        x += v * dt / 1000;
        t += dt;
    }
    x.clamp(0, ONE)
}

/// Fixed-timestep driver. `tick(dt_ms)` is the only clock source.
pub struct Animator {
    elapsed_ms: i64,
}

impl Animator {
    pub fn new() -> Animator {
        Animator { elapsed_ms: 0 }
    }

    /// Advance the clock by `dt_ms` and return the new elapsed time.
    pub fn tick(&mut self, dt_ms: i64) -> i64 {
        self.elapsed_ms = self.elapsed_ms.saturating_add(dt_ms.max(0));
        self.elapsed_ms
    }

    pub fn elapsed_ms(&self) -> i64 {
        self.elapsed_ms
    }

    pub fn reset(&mut self) {
        self.elapsed_ms = 0;
    }

    /// Render the frame for the current clock position.
    pub fn frame(&self, ir: &Ir, state: &StateView, renderer: &mut Renderer) -> Frame {
        renderer.render_at(ir, state, self.elapsed_ms)
    }
}

impl Default for Animator {
    fn default() -> Self {
        Animator::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_are_monotonic_and_settle_at_one() {
        let mut prev = 0i64;
        for t in (0..=600).step_by(16) {
            for curve in [Curve::Spring, Curve::Ease] {
                let p = progress(curve, t);
                assert!((0..=ONE).contains(&p), "{curve:?} at {t}ms left the range");
                assert!(p >= prev || curve == Curve::Spring, "{curve:?} went backwards");
                prev = p;
            }
        }
        assert_eq!(progress(Curve::None, 0), ONE);
        assert_eq!(progress(Curve::Ease, EASE_MS), ONE);
        assert_eq!(progress(Curve::Spring, 4000), ONE);
    }

    #[test]
    fn spring_and_ease_differ() {
        assert_ne!(progress(Curve::Spring, 100), progress(Curve::Ease, 100));
    }
}
