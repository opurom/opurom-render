//! Deterministic fixed-timestep animation.
//!
//! Curves, page transitions, gesture progress and particles are integer-only
//! (no floats, no wall clock, no external randomness): the clock only advances
//! when the caller ticks it, so two identical tick sequences always produce
//! identical frame digests.
//!
//! Transitions compose two existing RGBA frames: `from` is the outgoing
//! page, `to` is the incoming page. Slide translates both pages by an integer
//! number of columns, fade blends channels in fixed point, and scale draws a
//! nearest-neighbour copy of the incoming page over the outgoing page.

use crate::{DirtyRect, Frame, Ir, Renderer, Rgba, StateView};

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
pub const ONE: i64 = 1024;
/// Duration of the ease curve.
pub const EASE_MS: i64 = 320;
/// Integration step of the spring solver.
pub const STEP_MS: i64 = 16;

/// Spring constants: stiffness and damping (per second, unit mass).
const K: i64 = 144;
const C: i64 = 17;

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
    ONE - (inv * inv * inv) / (ONE * ONE)
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
    // Integer truncation would leave the spring oscillating by one unit forever,
    // so snap once it is within tolerance.
    if (x - ONE).abs() <= 8 && v.abs() <= 128 {
        x = ONE;
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

/// Duration of a page transition.
pub const TRANSITION_MS: i64 = 320;
/// Integer coordinate space used by particles before mapping to a frame.
pub const PARTICLE_FIELD: i64 = 1024;
/// Maximum particle lifetime, in `step()` calls.
pub const PARTICLE_MAX_LIFE: i64 = 64;

/// The three deterministic page transitions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionKind {
    Slide,
    Fade,
    Scale,
}

impl TransitionKind {
    pub fn parse(s: &str) -> Option<TransitionKind> {
        Some(match s {
            "slide" => TransitionKind::Slide,
            "fade" => TransitionKind::Fade,
            "scale" => TransitionKind::Scale,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            TransitionKind::Slide => "slide",
            TransitionKind::Fade => "fade",
            TransitionKind::Scale => "scale",
        }
    }
}

/// A page transition whose only clock is `tick(dt_ms)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transition {
    pub kind: TransitionKind,
    /// Fixed-point progress in `0..=ONE`.
    pub progress: i64,
}

impl Transition {
    pub fn new(kind: TransitionKind) -> Transition {
        Transition { kind, progress: 0 }
    }

    /// Advance by a caller-supplied millisecond delta and return the progress.
    pub fn tick(&mut self, dt_ms: i64) -> i64 {
        let step = dt_ms.max(0).saturating_mul(ONE) / TRANSITION_MS;
        self.progress = self.progress.clamp(0, ONE).saturating_add(step).min(ONE);
        self.progress
    }

    /// Compose two equally sized RGBA frames at the current progress.
    pub fn compose(&self, from: &Frame, to: &Frame) -> Result<Frame, String> {
        if from.width == 0 || from.height == 0 {
            return Err("transition frames must have non-zero dimensions".to_string());
        }
        if from.width != to.width || from.height != to.height {
            return Err("transition frames must have the same dimensions".to_string());
        }
        let expected = rgba_bytes(to.width, to.height)
            .ok_or_else(|| "transition frame is too large".to_string())?;
        if from.pixels.len() != expected || to.pixels.len() != expected {
            return Err("transition frame pixel buffer has the wrong length".to_string());
        }
        let p = self.progress.clamp(0, ONE);
        let pixels = match self.kind {
            TransitionKind::Slide => slide(from, to, p),
            TransitionKind::Fade => fade(from, to, p),
            TransitionKind::Scale => scale(from, to, p),
        };
        Ok(Frame {
            width: to.width,
            height: to.height,
            pixels,
            dirty: full_dirty(to.width, to.height),
        })
    }
}

fn rgba_bytes(width: u32, height: u32) -> Option<usize> {
    (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)
}

fn full_dirty(width: u32, height: u32) -> Vec<DirtyRect> {
    vec![DirtyRect {
        x: 0,
        y: 0,
        w: width,
        h: height,
    }]
}

fn slide(from: &Frame, to: &Frame, p: i64) -> Vec<u8> {
    let w = from.width as usize;
    let row_bytes = w * 4;
    let shift = (from.width as i64 * p / ONE) as usize;
    let split = (w - shift) * 4;
    let mut out = vec![0u8; from.pixels.len()];
    for row in 0..from.height as usize {
        let base = row * row_bytes;
        out[base..base + split].copy_from_slice(&from.pixels[base + shift * 4..base + row_bytes]);
        out[base + split..base + row_bytes].copy_from_slice(&to.pixels[base..base + shift * 4]);
    }
    out
}

fn fade(from: &Frame, to: &Frame, p: i64) -> Vec<u8> {
    from.pixels
        .iter()
        .zip(&to.pixels)
        .map(|(&a, &b)| {
            let mixed = (a as i64 * (ONE - p) + b as i64 * p + ONE / 2) / ONE;
            mixed.clamp(0, 255) as u8
        })
        .collect()
}

fn scale(from: &Frame, to: &Frame, p: i64) -> Vec<u8> {
    let mut out = from.pixels.clone();
    let w = from.width as usize;
    let h = from.height as usize;
    let sw = (from.width as i64 * p / ONE) as usize;
    let sh = (from.height as i64 * p / ONE) as usize;
    if sw == 0 || sh == 0 {
        return out;
    }
    let (x0, y0) = ((w - sw) / 2, (h - sh) / 2);
    // shortcut: nearest-neighbour scale avoids a resampling filter, upgrade when a real presenter exists
    for dy in 0..sh {
        let sy = (dy as i64 * h as i64 / sh as i64) as usize;
        for dx in 0..sw {
            let sx = (dx as i64 * w as i64 / sw as i64) as usize;
            let src = (sy * w + sx) * 4;
            let dst = ((y0 + dy) * w + x0 + dx) * 4;
            out[dst..dst + 4].copy_from_slice(&to.pixels[src..src + 4]);
        }
    }
    out
}

/// Gesture progress from cumulative integer drag displacement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GestureAnim {
    /// Fixed-point progress in `0..=ONE`.
    pub progress: i64,
    x: i64,
    y: i64,
}

impl GestureAnim {
    pub fn new() -> GestureAnim {
        GestureAnim::default()
    }

    /// One input unit is one L1 distance unit; progress saturates at `ONE`.
    pub fn drag(&mut self, delta_x: i64, delta_y: i64) -> i64 {
        self.x = self.x.saturating_add(delta_x);
        self.y = self.y.saturating_add(delta_y);
        let distance = (self.x as i128).abs() + (self.y as i128).abs();
        self.progress = distance.min(ONE as i128) as i64;
        self.progress
    }

    /// A drag scrub is a slide transition at the gesture's progress.
    pub fn compose(&self, from: &Frame, to: &Frame) -> Result<Frame, String> {
        Transition {
            kind: TransitionKind::Slide,
            progress: self.progress.clamp(0, ONE),
        }
        .compose(from, to)
    }
}

/// One particle: integer position, velocity and bounded lifetime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Particle {
    pub x: i64,
    pub y: i64,
    pub vx: i64,
    pub vy: i64,
    pub life: i64,
}

/// Fixed-seed particles. `step()` is the only source of motion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParticleSystem {
    pub particles: Vec<Particle>,
    rng: u64,
}

impl ParticleSystem {
    pub fn new(seed: u64, count: usize) -> ParticleSystem {
        let mut rng = seed ^ 0x9e37_79b9_7f4a_7c15;
        if rng == 0 {
            rng = 0x2545_f491_4f6c_dd1d;
        }
        let mut particles = Vec::with_capacity(count);
        for _ in 0..count {
            particles.push(spawn(&mut rng));
        }
        ParticleSystem { particles, rng }
    }

    pub fn step(&mut self) {
        let rng = &mut self.rng;
        for p in self.particles.iter_mut() {
            p.x = (p.x + p.vx).rem_euclid(PARTICLE_FIELD);
            p.y = (p.y + p.vy).rem_euclid(PARTICLE_FIELD);
            p.life -= 1;
            if p.life <= 0 {
                *p = spawn(rng);
            }
        }
    }

    /// Overlay 2x2 integer-mapped particles on an existing RGBA frame.
    pub fn render_into(&self, frame: &mut Frame) -> Result<(), String> {
        if frame.width == 0 || frame.height == 0 {
            return Err("particle frame must have non-zero dimensions".to_string());
        }
        let expected = rgba_bytes(frame.width, frame.height)
            .ok_or_else(|| "particle frame is too large".to_string())?;
        if frame.pixels.len() != expected {
            return Err("particle frame pixel buffer has the wrong length".to_string());
        }
        let w = frame.width as usize;
        let h = frame.height as usize;
        for (i, p) in self.particles.iter().enumerate() {
            let r = 0x40u8.wrapping_add((i as u8).wrapping_mul(37));
            let g = 0x30u8.wrapping_add((p.life as u8).wrapping_mul(3));
            let color = Rgba::rgb(r, g, 0xe5);
            let px = (p.x * w as i64 / PARTICLE_FIELD) as usize;
            let py = (p.y * h as i64 / PARTICLE_FIELD) as usize;
            for oy in 0..2usize {
                for ox in 0..2usize {
                    let (x, y) = (px + ox, py + oy);
                    if x >= w || y >= h {
                        continue;
                    }
                    let dst = (y * w + x) * 4;
                    frame.pixels[dst..dst + 4]
                        .copy_from_slice(&[color.r, color.g, color.b, color.a]);
                }
            }
        }
        frame.dirty = full_dirty(frame.width, frame.height);
        Ok(())
    }
}

// shortcut: xorshift64* is deterministic and reproducible, not cryptographic; upgrade only for adversarial seeds
fn next_rand(rng: &mut u64) -> u64 {
    let mut x = *rng;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *rng = x;
    x.wrapping_mul(0x2545_f491_4f6c_dd1d)
}

fn spawn(rng: &mut u64) -> Particle {
    Particle {
        x: (next_rand(rng) % (PARTICLE_FIELD as u64)) as i64,
        y: (next_rand(rng) % (PARTICLE_FIELD as u64)) as i64,
        vx: (next_rand(rng) % 17) as i64 - 8,
        vy: (next_rand(rng) % 17) as i64 - 8,
        life: 1 + (next_rand(rng) % (PARTICLE_MAX_LIFE as u64)) as i64,
    }
}

/// Caller-applied quality policy for particles and expensive effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quality {
    pub particle_count: usize,
    pub effects_enabled: bool,
}

/// Deterministic budget tiers: >=16 ms full, >=8 ms three quarters, >=4 ms
/// half, otherwise one quarter. `measured_frame_budget_ms` is supplied by the
/// caller (for example `opurom profile`); this function never reads a clock.
pub fn choose_quality(
    measured_frame_budget_ms: i64,
    particle_count: usize,
    effects_enabled: bool,
) -> Quality {
    let (num, den, effects) = if measured_frame_budget_ms >= 16 {
        (1, 1, effects_enabled)
    } else if measured_frame_budget_ms >= 8 {
        (3, 4, false)
    } else if measured_frame_budget_ms >= 4 {
        (1, 2, false)
    } else {
        (1, 4, false)
    };
    Quality {
        particle_count: particle_count.saturating_mul(num) / den,
        effects_enabled: effects,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_are_monotonic_and_settle_at_one() {
        for curve in [Curve::Spring, Curve::Ease] {
            let mut prev = 0i64;
            for t in (0..=4000).step_by(16) {
                let p = progress(curve, t);
                assert!((0..=ONE).contains(&p), "{curve:?} at {t}ms left the range");
                assert!(p >= prev, "{curve:?} went backwards at {t}ms");
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

    #[test]
    fn animator_only_moves_when_ticked() {
        let mut a = Animator::new();
        assert_eq!(a.elapsed_ms(), 0);
        assert_eq!(a.tick(16), 16);
        assert_eq!(a.tick(16), 32);
        a.reset();
        assert_eq!(a.elapsed_ms(), 0);
    }
}
