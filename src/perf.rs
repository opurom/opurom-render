//! Deterministic measured-frame auto-degrade policy.
//!
//! The host injects one integer frame duration through `FrameBudget::push` or
//! `Renderer::render_auto_at`; this module never reads a wall clock. A rolling
//! `VecDeque` holds the last `WINDOW_FRAMES` samples and is evaluated once every
//! `WINDOW_FRAMES` injected frames. Percentiles use the integer nearest-rank
//! rule `ceil(percent * len / 100)`, so p95 of 20 samples is its 19th value.
//!
//! A strict p95 above `FrameRate::budget_ms()` drops one tier. Promotion is
//! stricter: p95 must stay below three quarters of the target for
//! `PROMOTION_WINDOWS` consecutive evaluated windows. A neutral window resets
//! the streak. At 60 fps the drop/promotion thresholds are `>16` / `<12` ms;
//! at 120 fps they are `>8` / `<6` ms.
//!
//! Full is a no-op for valid IR: its caps are the parse-time maxima and
//! expensive glass/liquid effects remain enabled. Reduced and Minimal rebuild
//! from the baseline IR, so recovery restores complexity instead of compounding
//! earlier caps.

use crate::effects::{VisualEffectKind, EFFECT_MAX_RADIUS};
use crate::particles::PARTICLE_MAX_COUNT;
use crate::{Frame, FrameRate, Ir, Node, Renderer, StateView};
use std::collections::VecDeque;

/// Frames in one measured rolling window.
pub const WINDOW_FRAMES: usize = 20;
/// Consecutive good windows required before one tier promotion.
pub const PROMOTION_WINDOWS: usize = 3;
/// Numerator of the promotion threshold relative to the frame target.
pub const PROMOTION_NUM: i64 = 3;
/// Denominator of the promotion threshold relative to the frame target.
pub const PROMOTION_DEN: i64 = 4;

const P50_PERCENT: i64 = 50;
const P95_PERCENT: i64 = 95;
const FPS_X10_SCALE: i128 = 10_000;

/// Complexity tiers, ordered from most to least expensive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum QualityTier {
    #[default]
    Full,
    Reduced,
    Minimal,
}

impl QualityTier {
    pub const ALL: [QualityTier; 3] = [Self::Full, Self::Reduced, Self::Minimal];

    /// Drop one tier, saturating at Minimal.
    pub const fn degrade(self) -> QualityTier {
        match self {
            QualityTier::Full => QualityTier::Reduced,
            QualityTier::Reduced | QualityTier::Minimal => QualityTier::Minimal,
        }
    }

    /// Promote one tier, saturating at Full.
    pub const fn promote(self) -> QualityTier {
        match self {
            QualityTier::Full | QualityTier::Reduced => QualityTier::Full,
            QualityTier::Minimal => QualityTier::Reduced,
        }
    }

    /// Concrete bounds for this tier. Counts and radii are existing IR maxima
    /// divided by integer constants; only Full draws glass or liquid effects.
    pub const fn caps(self) -> TierCaps {
        match self {
            QualityTier::Full => TierCaps {
                particle_count_cap: PARTICLE_MAX_COUNT,
                effect_radius_cap: EFFECT_MAX_RADIUS,
                expensive_effects: true,
            },
            QualityTier::Reduced => TierCaps {
                particle_count_cap: PARTICLE_MAX_COUNT / 2,
                effect_radius_cap: EFFECT_MAX_RADIUS / 4,
                expensive_effects: false,
            },
            QualityTier::Minimal => TierCaps {
                particle_count_cap: PARTICLE_MAX_COUNT / 8,
                effect_radius_cap: EFFECT_MAX_RADIUS / 16,
                expensive_effects: false,
            },
        }
    }
}

/// Bounded work permitted by one quality tier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TierCaps {
    /// Maximum `Particles.count` for one particle field.
    pub particle_count_cap: i64,
    /// Maximum `VisualEffect.radius` for every effect.
    pub effect_radius_cap: i64,
    /// Whether `glass` prop effects and explicit glass/liquid passes are drawn.
    pub expensive_effects: bool,
}

/// A baseline IR plus the active IR presented to the renderer. The baseline is
/// immutable, so changing tiers can restore counts and removed effects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderConfig {
    base: Ir,
    active: Ir,
}

impl RenderConfig {
    pub fn new(base: Ir) -> RenderConfig {
        let active = base.clone();
        RenderConfig { base, active }
    }

    /// The active IR after the most recent `apply_policy` call.
    pub fn ir(&self) -> &Ir {
        &self.active
    }
}

/// Apply one tier to a render configuration and return the same configuration.
/// Full preserves valid IR exactly; lower tiers cap particle count and effect
/// radius and remove expensive glass/liquid effects throughout the tree.
pub fn apply_policy(quality: QualityTier, config: &mut RenderConfig) -> &RenderConfig {
    // shortcut: clone the baseline IR so promotions restore complexity; upgrade to a patch list for huge trees
    config.active = config.base.clone();
    let caps = quality.caps();
    apply_node(&mut config.active.tree, caps);
    config
}

fn apply_node(node: &mut Node, caps: TierCaps) {
    if let Some(particles) = node.props.particles.as_mut() {
        particles.count = particles.count.clamp(0, caps.particle_count_cap);
    }
    for effect in &mut node.props.effects {
        effect.radius = effect.radius.clamp(0, caps.effect_radius_cap);
    }
    if !caps.expensive_effects {
        node.props.glass = false;
        node.props.effects.retain(|effect| {
            !matches!(
                effect.kind,
                VisualEffectKind::Glass | VisualEffectKind::Liquid
            )
        });
    }
    for child in &mut node.children {
        apply_node(child, caps);
    }
}

impl Renderer {
    /// Inject one measured frame duration, update the budget, apply its tier to
    /// a fresh copy of the baseline IR, and render at the caller's clock.
    /// This opt-in path is additive: existing `render_at` and `pixels` calls
    /// are unchanged, and no wall clock is consulted.
    pub fn render_auto_at(
        &mut self,
        config: &mut RenderConfig,
        state: &StateView,
        elapsed_ms: i64,
        measured_frame_ms: i64,
        budget: &mut FrameBudget,
    ) -> Frame {
        let tier = budget.push(measured_frame_ms);
        apply_policy(tier, config);
        self.render_at(config.ir(), state, elapsed_ms)
    }
}

/// Pure hysteresis state machine over injected integer frame durations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameBudget {
    rate: FrameRate,
    tier: QualityTier,
    samples: VecDeque<i64>,
    frames: u64,
    good_windows: usize,
}

impl FrameBudget {
    pub fn new(rate: FrameRate) -> FrameBudget {
        FrameBudget {
            rate,
            tier: QualityTier::Full,
            samples: VecDeque::with_capacity(WINDOW_FRAMES),
            frames: 0,
            good_windows: 0,
        }
    }

    pub fn rate(&self) -> FrameRate {
        self.rate
    }

    pub fn tier(&self) -> QualityTier {
        self.tier
    }

    /// Target frame time: 16 ms at 60 fps, 8 ms at 120 fps.
    pub fn target_ms(&self) -> i64 {
        self.rate.budget_ms()
    }

    /// Strict lower threshold for promotion: 12 ms at 60 fps, 6 ms at 120 fps.
    pub fn promote_ms(&self) -> i64 {
        self.target_ms() * PROMOTION_NUM / PROMOTION_DEN
    }

    pub fn good_windows(&self) -> usize {
        self.good_windows
    }

    /// Add one injected duration. Negative inputs are clamped to zero at this
    /// boundary. Returns the tier after the possible end-of-window decision.
    pub fn push(&mut self, duration_ms: i64) -> QualityTier {
        if self.samples.len() == WINDOW_FRAMES {
            self.samples.pop_front();
        }
        self.samples.push_back(duration_ms.max(0));
        self.frames = self.frames.wrapping_add(1);
        if self.samples.len() == WINDOW_FRAMES && self.frames % (WINDOW_FRAMES as u64) == 0 {
            self.evaluate();
        }
        self.tier
    }

    fn evaluate(&mut self) {
        let sorted = sorted_samples(self.samples.iter().copied());
        let p95 = nearest_rank(&sorted, P95_PERCENT);
        if p95 > self.target_ms() {
            self.tier = self.tier.degrade();
            self.good_windows = 0;
        } else if p95 < self.promote_ms() && self.tier != QualityTier::Full {
            self.good_windows = self.good_windows.saturating_add(1);
            if self.good_windows >= PROMOTION_WINDOWS {
                self.tier = self.tier.promote();
                self.good_windows = 0;
            }
        } else {
            self.good_windows = 0;
        }
    }

    /// Measured samples and the current state-machine tier, ready for
    /// `perf_report`.
    pub fn window(&self) -> PerfWindow {
        PerfWindow {
            samples: self.samples.iter().copied().collect(),
            tier: self.tier,
        }
    }

    /// Report the current rolling window without changing the state machine.
    pub fn report(&self) -> PerfReport {
        perf_report(&self.window())
    }
}

/// A measured window plus the tier decided by its owning `FrameBudget`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PerfWindow {
    pub samples: Vec<i64>,
    pub tier: QualityTier,
}

/// Measured values derived from one window. `tier` is copied from the window,
/// not recomputed from a fabricated heuristic.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PerfReport {
    /// Nearest-rank 50th percentile in milliseconds.
    pub p50_ms: i64,
    /// Nearest-rank 95th percentile in milliseconds.
    pub p95_ms: i64,
    /// FPS multiplied by ten: truncated `10_000 * samples / total_ms`.
    /// A window with zero elapsed time reports zero.
    pub fps_x10: i64,
    /// The state-machine tier carried by the window.
    pub tier: QualityTier,
}

/// Pure integer report over injected samples. Negative durations are clamped
/// to zero; p50/p95 use nearest rank and fps uses the measured total only.
pub fn perf_report(window: &PerfWindow) -> PerfReport {
    let sorted = sorted_samples(window.samples.iter().copied());
    let total: i128 = sorted.iter().map(|&ms| ms as i128).sum();
    let fps_x10 = if total <= 0 {
        0
    } else {
        ((FPS_X10_SCALE * sorted.len() as i128) / total).min(i64::MAX as i128) as i64
    };
    PerfReport {
        p50_ms: nearest_rank(&sorted, P50_PERCENT),
        p95_ms: nearest_rank(&sorted, P95_PERCENT),
        fps_x10,
        tier: window.tier,
    }
}

fn sorted_samples(samples: impl IntoIterator<Item = i64>) -> Vec<i64> {
    let mut sorted: Vec<i64> = samples.into_iter().map(|ms| ms.max(0)).collect();
    sorted.sort_unstable();
    sorted
}

fn nearest_rank(sorted: &[i64], percent: i64) -> i64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = (percent as i128 * sorted.len() as i128 + 99).div_euclid(100) as usize;
    let index = rank.clamp(1, sorted.len()) - 1;
    sorted[index]
}
