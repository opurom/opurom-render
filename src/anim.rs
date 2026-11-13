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
    Inertia,
}

impl Curve {
    pub fn parse(s: &str) -> Option<Curve> {
        Some(match s {
            "none" => Curve::None,
            "spring" => Curve::Spring,
            "ease" => Curve::Ease,
            "inertia" => Curve::Inertia,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Curve::None => "none",
            Curve::Spring => "spring",
            Curve::Ease => "ease",
            Curve::Inertia => "inertia",
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

/// Inertia fling: start velocity and constant braking force per `STEP_MS`,
/// both in fixed-point progress units.
const FLING: i64 = 256;
const BRAKE: i64 = 32;

/// Curve progress at `elapsed_ms`, in `0..=ONE`.
pub fn progress(curve: Curve, elapsed_ms: i64) -> i64 {
    match curve {
        Curve::None => ONE,
        Curve::Ease => ease(elapsed_ms),
        Curve::Spring => spring(elapsed_ms),
        Curve::Inertia => inertia(elapsed_ms),
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

/// Fling under a constant integer braking force: every `STEP_MS` step travels
/// `v` distance units and then drops `v` by `BRAKE`, so step `k` travels
/// `FLING - k * BRAKE` and the velocity rests after `FLING / BRAKE` steps.
/// The travelled distance clamps at `ONE`, which the fling reaches in a finite
/// number of steps because the velocity is positive and strictly decreasing.
fn inertia(elapsed_ms: i64) -> i64 {
    let mut x: i64 = 0;
    let mut v: i64 = FLING;
    let mut steps = elapsed_ms.div_euclid(STEP_MS).max(0);
    while steps > 0 && v > 0 && x < ONE {
        x = x.saturating_add(v).min(ONE);
        // v stays a positive multiple of BRAKE while it brakes, so it rests at 0.
        v -= BRAKE;
        steps -= 1;
    }
    x
}

/// Deterministic integer frame rate of the animation clock.
///
/// The animation objective names 60 and 120 fps. Each frame advances the
/// fixed-timestep clock by the integer interval `1000 / hz` (truncated), so a
/// tick sequence depends only on the frame count and never on a wall clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameRate {
    Hz60,
    Hz120,
}

impl FrameRate {
    pub fn parse(s: &str) -> Option<FrameRate> {
        Some(match s {
            "60" => FrameRate::Hz60,
            "120" => FrameRate::Hz120,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            FrameRate::Hz60 => "60",
            FrameRate::Hz120 => "120",
        }
    }

    /// Integer millisecond interval between two frames: `1000 / hz` truncated.
    pub const fn frame_interval_ms(self) -> i64 {
        match self {
            FrameRate::Hz60 => 16,
            FrameRate::Hz120 => 8,
        }
    }

    /// The per-frame budget a caller hands to `choose_quality`. At 120 fps a
    /// frame has half the time of a 60 fps frame, so the same caller-supplied
    /// policy degrades automatically.
    pub const fn budget_ms(self) -> i64 {
        self.frame_interval_ms()
    }

    /// Millisecond clock position of the `frame`-th tick (0-based).
    pub const fn frame_ms(self, frame: i64) -> i64 {
        frame.saturating_mul(self.frame_interval_ms())
    }
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

    /// Advance the clock by one integer frame interval of `rate` and return the
    /// new elapsed time. `tick_frame(rate)` is exactly `tick(rate.frame_interval_ms())`.
    pub fn tick_frame(&mut self, rate: FrameRate) -> i64 {
        self.tick(rate.frame_interval_ms())
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

/// How `step_mode` treats the field boundary and neighbouring particles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum StepMode {
    /// Original behaviour: positions wrap around with `rem_euclid`.
    #[default]
    Wrap,
    /// Reflect off the field boundary with restitution and resolve bounded
    /// equal-mass elastic contacts between nearby particles.
    Bounce,
}

/// Restitution of a wall bounce, fixed point: `e * ONE` with `e = 3/4`, so a
/// reflected speed is `-(v * 768) / 1024`.
const RESTITUTION: i64 = ONE * 3 / 4;
/// Contact range: twice the maximum spawn speed (8), so a closing pair is
/// always still in contact when it is checked and can never tunnel past the
/// bounded neighbour window in one step.
const TOUCH: i64 = 16;
/// Neighbour window: particle `i` is paired only with the next `NEIGHBORS`
/// particles in index order.
// shortcut: index window instead of a spatial grid, upgrade to a spatial hash when a large field makes index order stop matching screen proximity
const NEIGHBORS: usize = 8;

/// Fixed-seed particles. `step()` and `step_mode()` are the only sources of
/// motion.
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

    /// Advance one step in the default `Wrap` mode.
    pub fn step(&mut self) {
        self.step_mode(StepMode::Wrap);
    }

    /// Advance one fixed step in the given mode. `step()` is exactly
    /// `step_mode(StepMode::Wrap)`; `Bounce` only adds boundary reflection and
    /// bounded pair contacts on top of the same movement and respawning.
    pub fn step_mode(&mut self, mode: StepMode) {
        if mode == StepMode::Bounce {
            resolve_contacts(&mut self.particles);
        }
        let rng = &mut self.rng;
        for p in self.particles.iter_mut() {
            if mode == StepMode::Bounce {
                p.x += p.vx;
                p.y += p.vy;
                bounce_axis(&mut p.x, &mut p.vx);
                bounce_axis(&mut p.y, &mut p.vy);
            } else {
                p.x = (p.x + p.vx).rem_euclid(PARTICLE_FIELD);
                p.y = (p.y + p.vy).rem_euclid(PARTICLE_FIELD);
            }
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

/// Mirror one axis past the wall it crossed and scale the reflected speed by
/// the restitution coefficient.
// shortcut: one reflection plus a clamp per step covers the spawn bound,
// upgrade to a multi-wall fold if velocities past 8 units/step become legal
fn bounce_axis(pos: &mut i64, vel: &mut i64) {
    if *pos >= PARTICLE_FIELD {
        *pos = 2 * (PARTICLE_FIELD - 1) - *pos;
        *vel = -*vel * RESTITUTION / ONE;
    } else if *pos < 0 {
        *pos = -*pos;
        *vel = -*vel * RESTITUTION / ONE;
    }
    *pos = (*pos).clamp(0, PARTICLE_FIELD - 1);
}

/// Equal-mass elastic response: when two particles are within `TOUCH` on both
/// axes, each axis on which they are closing exchanges velocities — the 1D
/// elastic collision at unit mass — so the pair separates with the same speed.
/// The neighbour window bounds the pass at `NEIGHBORS` checks per particle.
fn resolve_contacts(particles: &mut [Particle]) {
    let n = particles.len();
    for i in 0..n {
        for j in (i + 1)..n.min(i + 1 + NEIGHBORS) {
            let (a, b) = particles.split_at_mut(j);
            let (p, q) = (&mut a[i], &mut b[0]);
            if (q.x - p.x).abs() >= TOUCH || (q.y - p.y).abs() >= TOUCH {
                continue;
            }
            if (q.x - p.x) * (q.vx - p.vx) <= 0 && q.vx != p.vx {
                std::mem::swap(&mut p.vx, &mut q.vx);
            }
            if (q.y - p.y) * (q.vy - p.vy) <= 0 && q.vy != p.vy {
                std::mem::swap(&mut p.vy, &mut q.vy);
            }
        }
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
        for curve in [Curve::Spring, Curve::Ease, Curve::Inertia] {
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
        assert_eq!(progress(Curve::Inertia, 4000), ONE);
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

    #[test]
    fn inertia_settles_after_the_measured_braking_steps() {
        assert_eq!(Curve::parse("inertia"), Some(Curve::Inertia));
        assert_eq!(Curve::Inertia.name(), "inertia");
        // Derived from the braking rule: step k travels FLING - k * BRAKE.
        let mut travelled = 0i64;
        let mut velocity = FLING;
        let mut steps = 0i64;
        while velocity > 0 && travelled < ONE {
            travelled += velocity;
            velocity -= BRAKE;
            steps += 1;
        }
        assert!(
            travelled >= ONE,
            "the braking fling must reach ONE before resting"
        );
        // Counted the same way until the braking force floors the velocity.
        let mut rest_steps = steps;
        while velocity > 0 {
            velocity -= BRAKE;
            rest_steps += 1;
        }
        assert_eq!(
            velocity, 0,
            "the braking force must rest the velocity at zero"
        );
        let settle_ms = steps * STEP_MS;
        // Measured on the real curve: still moving one step earlier, settled at
        // settle_ms, and settled for every later tick up to the CLI range end.
        assert!(progress(Curve::Inertia, settle_ms - STEP_MS) < ONE);
        assert_eq!(progress(Curve::Inertia, settle_ms), ONE);
        assert_eq!(progress(Curve::Inertia, rest_steps * STEP_MS), ONE);
        assert_eq!(progress(Curve::Inertia, 4000), ONE);
        println!(
            "inertia settles after {steps} steps ({settle_ms} ms); velocity rests after {rest_steps} steps"
        );
    }

    #[test]
    fn wall_bounce_reflects_with_the_measured_restitution() {
        let right_wall = Particle {
            x: PARTICLE_FIELD - 4,
            y: 400,
            vx: 6,
            vy: 0,
            life: 40,
        };
        let mut right = ParticleSystem::new(1, 0);
        right.particles.push(right_wall);
        right.step_mode(StepMode::Bounce);
        let p = right.particles[0];
        let expected_right = -(right_wall.vx * RESTITUTION) / ONE;
        assert_eq!(p.vx, expected_right, "restitution formula must hold");
        assert!(p.vx < 0, "the right wall must reverse direction");
        assert!((0..PARTICLE_FIELD).contains(&p.x));

        let left_wall = Particle {
            x: 3,
            y: 400,
            vx: -6,
            vy: 0,
            life: 40,
        };
        let mut left = ParticleSystem::new(2, 0);
        left.particles.push(left_wall);
        left.step_mode(StepMode::Bounce);
        let q = left.particles[0];
        let expected_left = -(left_wall.vx * RESTITUTION) / ONE;
        assert_eq!(q.vx, expected_left, "restitution formula must hold");
        assert!(q.vx > 0, "the left wall must reverse direction");
        assert!((0..PARTICLE_FIELD).contains(&q.x));

        println!(
            "rebound: vx {} -> {} at x {}; vx {} -> {} at x {}",
            right_wall.vx, p.vx, p.x, left_wall.vx, q.vx, q.x
        );
    }

    #[test]
    fn equal_mass_pair_exchanges_closing_velocities() {
        let a0 = Particle {
            x: 500,
            y: 500,
            vx: 6,
            vy: 2,
            life: 40,
        };
        let b0 = Particle {
            x: 506,
            y: 510,
            vx: -6,
            vy: -2,
            life: 40,
        };
        let mut system = ParticleSystem::new(3, 0);
        system.particles.push(a0);
        system.particles.push(b0);
        system.step_mode(StepMode::Bounce);
        let (a1, b1) = (system.particles[0], system.particles[1]);
        // Equal-mass elastic response: exact exchange of both closing axes.
        assert_eq!((a1.vx, a1.vy), (b0.vx, b0.vy));
        assert_eq!((b1.vx, b1.vy), (a0.vx, a0.vy));
        // Momentum is preserved on each axis.
        assert_eq!(a1.vx + b1.vx, a0.vx + b0.vx);
        assert_eq!(a1.vy + b1.vy, a0.vy + b0.vy);
        // The response separates the pair: both gaps grow.
        assert!(b1.x - a1.x > b0.x - a0.x);
        assert!((b1.y - a1.y).abs() > (b0.y - a0.y).abs());
        assert!((0..PARTICLE_FIELD).contains(&a1.x));
        assert!((0..PARTICLE_FIELD).contains(&b1.x));
        assert!((0..PARTICLE_FIELD).contains(&a1.y));
        assert!((0..PARTICLE_FIELD).contains(&b1.y));
    }

    #[test]
    fn bounce_mode_is_byte_identical_for_equal_seeds_and_steps() {
        let mut first = ParticleSystem::new(0x5eed_4242, 512);
        let mut second = ParticleSystem::new(0x5eed_4242, 512);
        for _ in 0..100 {
            first.step_mode(StepMode::Bounce);
            second.step_mode(StepMode::Bounce);
        }
        // Every field, including the generator state, must compare equal.
        assert_eq!(first, second);
        for p in &first.particles {
            assert!((0..PARTICLE_FIELD).contains(&p.x));
            assert!((0..PARTICLE_FIELD).contains(&p.y));
            assert!(p.vx.abs() <= 8 && p.vy.abs() <= 8);
            assert!((1..=PARTICLE_MAX_LIFE).contains(&p.life));
        }
        let (w, h) = (32u32, 24u32);
        let mut frame_a = Frame {
            width: w,
            height: h,
            pixels: vec![0u8; (w * h * 4) as usize],
            dirty: Vec::new(),
        };
        let mut frame_b = frame_a.clone();
        first.render_into(&mut frame_a).unwrap();
        second.render_into(&mut frame_b).unwrap();
        assert_eq!(frame_a.pixels, frame_b.pixels);
        assert_eq!(
            crate::digest_hex(&frame_a.pixels),
            crate::digest_hex(&frame_b.pixels)
        );
    }

    #[test]
    fn wrap_stays_the_default_step_mode() {
        let mut default = ParticleSystem::new(0x5eed_9876, 12);
        let mut explicit = default.clone();
        for _ in 0..200 {
            default.step();
            explicit.step_mode(StepMode::Wrap);
        }
        assert_eq!(default, explicit, "step() must be exactly Wrap mode");

        // The default still wraps: a particle crossing the right edge reappears left.
        let mut edge = ParticleSystem::new(4, 0);
        edge.particles.push(Particle {
            x: PARTICLE_FIELD - 1,
            y: 10,
            vx: 5,
            vy: 0,
            life: 40,
        });
        edge.step();
        assert_eq!(edge.particles[0].x, 4);
        assert_eq!(edge.particles[0].vx, 5, "wrap must not touch velocity");

        // The same crossing in Bounce mode reflects instead of wrapping.
        let mut bounced = ParticleSystem::new(4, 0);
        bounced.particles.push(Particle {
            x: PARTICLE_FIELD - 1,
            y: 10,
            vx: 5,
            vy: 0,
            life: 40,
        });
        bounced.step_mode(StepMode::Bounce);
        assert!(bounced.particles[0].x < PARTICLE_FIELD - 1);
        assert!(bounced.particles[0].vx < 0);
    }

    #[test]
    fn frame_rate_parses_names_and_intervals() {
        assert_eq!(FrameRate::parse("60"), Some(FrameRate::Hz60));
        assert_eq!(FrameRate::parse("120"), Some(FrameRate::Hz120));
        assert_eq!(FrameRate::parse("30"), None);
        assert_eq!(FrameRate::Hz60.name(), "60");
        assert_eq!(FrameRate::Hz120.name(), "120");
        // The intervals are the truncated integer `1000 / hz`, matching the
        // fixed step the curves are already integrated with.
        assert_eq!(FrameRate::Hz60.frame_interval_ms(), 16);
        assert_eq!(FrameRate::Hz120.frame_interval_ms(), 8);
        assert_eq!(FrameRate::Hz60.budget_ms(), 16);
        assert_eq!(FrameRate::Hz120.budget_ms(), 8);
    }

    #[test]
    fn frame_ms_is_the_integer_multiple_of_the_interval() {
        for rate in [FrameRate::Hz60, FrameRate::Hz120] {
            for frame in 0..=500i64 {
                assert_eq!(rate.frame_ms(frame), frame * rate.frame_interval_ms());
            }
            assert_eq!(rate.frame_ms(0), 0);
            // A larger frame count never moves backwards.
            assert!(rate.frame_ms(120) > rate.frame_ms(119));
        }
        // 120 fps reaches a given clock position in half the frames.
        assert_eq!(FrameRate::Hz120.frame_ms(60), FrameRate::Hz60.frame_ms(30));
    }

    #[test]
    fn frame_ticks_advance_the_clock_deterministically() {
        let mut first = Animator::new();
        let mut second = Animator::new();
        for frame in 1..=64 {
            assert_eq!(
                first.tick_frame(FrameRate::Hz60),
                FrameRate::Hz60.frame_ms(frame)
            );
            second.tick_frame(FrameRate::Hz60);
        }
        assert_eq!(
            first.elapsed_ms(),
            second.elapsed_ms(),
            "the same frame count must tick identically"
        );
        assert_eq!(first.elapsed_ms(), 64 * 16);

        // `tick_frame` is exactly `tick(interval_ms)`: two clocks advanced in
        // lockstep stay at the same elapsed position.
        let mut via_frame = Animator::new();
        let mut via_tick = Animator::new();
        for _ in 0..10 {
            let frame_elapsed = via_frame.tick_frame(FrameRate::Hz120);
            let tick_elapsed = via_tick.tick(FrameRate::Hz120.frame_interval_ms());
            assert_eq!(frame_elapsed, tick_elapsed);
        }
    }

    #[test]
    fn frame_rate_budget_degrades_at_120fps() {
        // A 60 fps frame keeps the full tier; a 120 fps frame has half the
        // budget and the same caller-supplied policy degrades automatically.
        let at_60 = choose_quality(FrameRate::Hz60.budget_ms(), 8, true);
        let at_120 = choose_quality(FrameRate::Hz120.budget_ms(), 8, true);
        assert_eq!(at_60.effects_enabled, true);
        assert_eq!(at_120.effects_enabled, false);
        assert!(at_120.particle_count < at_60.particle_count);
        assert_eq!(at_60.particle_count, 8);
        assert_eq!(at_120.particle_count, 6);
        assert_eq!(at_60, choose_quality(16, 8, true));
        assert_eq!(at_120, choose_quality(8, 8, true));
    }
}
