//! Deterministic Android scroll physics driver, phase 1.
//!
//! Closed-form evaluations only: no frame stepping, no wall clock, no IR or
//! host ABI change. Units are logical pixels, seconds and pixels/second.
//!
//! Source anchors (verified blobs at flutter-golden HEAD ab2912035e):
//! - SS scroll_simulation.dart `2197765bf4b832b7661e42dcf3bc65adc3462d6c`:
//!   `201`, `204`, `208-215`, `218-260` (clamping), `18-132` (bouncing),
//!   `50-57` (iOS friction), `59-75` (friction to spring handover),
//!   `79-81` (`maxSpringTransferVelocity`), `99-126` (phase selection).
//! - FS friction_simulation.dart `8c043f9da40cee1dae8282a1348b9681ac33c365`:
//!   `35-58`, `117-141`, `144-160`.
//! - SP scroll_physics.dart `a514baf880b1a0e2d204a9e0ec584d6a839b580d`:
//!   `434-438`, `462-467`, `1021-1053`, `748-946` (bouncing physics),
//!   `759-800` (deceleration rate, rubber band spring).
//! - GS spring_simulation.dart `62a89b8b6e14342ea5903cc339e3696f250253e1`:
//!   `284-295`, `302-392`, `257-280`.
//! - TOL tolerance.dart `ab4a7a5036283824d7cf9d87d96f58fbc78a529b:14-23`.
//! - GC constants.dart `ae23eac1178aff50cbb5b0f29566a0e45589dd99:65,90,95`.
//! - VT velocity_tracker.dart `322dde6ce31bee32f933d44f41cd08767f6e6866:87-128,142-220,295-390`.
//! - LSQ lsq_solver.dart `9e7d438a39cd45c012cb9192591cd559fd42e82d:88-205`.
//! - ACT scroll_activity.dart `051158be15443297fd5c7d59b0a58a63499af238:418-440`.
//!
//! Appendix A of SCROLL_PHYSICS_PROPOSAL.md lists older hashes for SP/SS/GS/
//! TOL/GC/VT; the commit actually checked out here has the hashes above. The
//! formula line anchors were re-read and match the proposal.
//!
//! R1: cross-target libm bit equality is not claimed. The driver evaluates the
//! same closed forms from injected integer milliseconds; positions are rounded
//! once at the pixel boundary (R2) before layout consumes them.

/// `kTouchSlop` (GC:65, SP:480).
pub const TOUCH_SLOP_PX: i64 = 18;
/// `kMinFlingVelocity` (GC:90, SP:492).
pub const MIN_FLING_VELOCITY: f64 = 50.0;
/// `kMaxFlingVelocity` (GC:95, SP:495).
pub const MAX_FLING_VELOCITY: f64 = 8000.0;
/// `ClampingScrollSimulation` friction default (SS:166-171).
pub const FLING_FRICTION: f64 = 0.015;
/// iOS bounce friction: `UIScrollView.decelerationRate` normal, `0.998^1000`
/// is about 0.135 (SS:50-57).
pub const BOUNCE_FRICTION: f64 = 0.135;
/// `BouncingScrollSimulation.maxSpringTransferVelocity` (SS:79-81).
pub const MAX_SPRING_TRANSFER_VELOCITY: f64 = 5000.0;
/// `BouncingScrollPhysics._rubberBandHalfLifeSeconds` (SP:762).
pub const RUBBER_BAND_HALF_LIFE_SECONDS: f64 = 0.07;
/// `BouncingScrollPhysics._rubberBandLambda = log(2) / halfLife` (SP:765).
pub const RUBBER_BAND_LAMBDA: f64 = 9.902102579427789;
/// `_kDecelerationRate = log(0.78) / log(0.9)` (SS:201), 2.3582018154259448.
/// Stored as a literal because `f64::ln` is not const on this toolchain.
pub const DECELERATION_RATE: f64 = 2.3582018154259448;
/// `_kInflexion` (SS:204).
pub const INFLEXION: f64 = 0.35;
/// `_physicalCoeff` (SS:208-215).
pub const PHYSICAL_COEFF: f64 = 9.80665 * 39.37 * 160.0 * 0.84;

/// Flutter scroll tolerance at a device pixel ratio (SP:462-467).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollTolerance {
    pub distance: f64,
    pub velocity: f64,
}

impl ScrollTolerance {
    pub fn for_dpr(dpr: f64) -> Self {
        Self {
            velocity: 1.0 / (0.050 * dpr),
            distance: 1.0 / dpr,
        }
    }
}

fn near_zero(value: f64, epsilon: f64) -> bool {
    value == 0.0 || (value > -epsilon && value < epsilon)
}

/// One injected pointer sample for velocity estimation. Time is in the host's
/// monotonic millisecond clock; position is in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VelocitySample {
    pub time_ms: i64,
    pub x: f64,
    pub y: f64,
}

/// Flutter's `VelocityEstimate` (`VT:87-128`): pixels/second and the window
/// that produced the estimate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VelocityEstimate {
    pub pixels_per_second: (f64, f64),
    pub confidence: f64,
    pub duration_ms: i64,
    pub offset: (f64, f64),
}

/// `VelocityTracker` history size, horizon, minimum sample count and stopped
/// timeout (`VT:142-145`). The generic tracker is the Android/default one.
#[derive(Clone, Debug, PartialEq)]
pub struct VelocityTracker {
    samples: [Option<VelocitySample>; 20],
    index: usize,
    last_sample_ms: i64,
}

impl Default for VelocityTracker {
    fn default() -> Self {
        Self {
            samples: [None; 20],
            index: 0,
            last_sample_ms: 0,
        }
    }
}

impl VelocityTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pointer cancel (`ACT:443-445`) discards the sample history.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// `VT:163-171`: ring-buffer one sample; non-monotonic input is ignored so
    /// the caller cannot make the estimator read backwards.
    pub fn add_position(&mut self, time_ms: i64, x: f64, y: f64) {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        if self.samples[self.index].is_some_and(|s| s.time_ms > time_ms) {
            return;
        }
        self.index = (self.index + 1) % 20;
        self.samples[self.index] = Some(VelocitySample { time_ms, x, y });
        self.last_sample_ms = time_ms;
    }

    /// `VT:179-250`: quadratic least squares over the newest contiguous
    /// 100 ms window; the slope coefficient is pixels/ms, scaled to px/s.
    /// `now_ms` replaces Flutter's sampling Stopwatch so no wall clock is read.
    pub fn estimate(&self, now_ms: i64) -> Option<VelocityEstimate> {
        let newest = self.samples[self.index]?;
        if now_ms.saturating_sub(self.last_sample_ms) > 40 {
            return Some(VelocityEstimate {
                pixels_per_second: (0.0, 0.0),
                confidence: 1.0,
                duration_ms: 0,
                offset: (0.0, 0.0),
            });
        }
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        let mut previous = newest;
        let mut index = self.index;
        while xs.len() < 20 {
            let Some(s) = self.samples[index] else { break };
            let age = (newest.time_ms - s.time_ms) as f64;
            let delta = (s.time_ms - previous.time_ms).unsigned_abs() as f64;
            previous = s;
            if age > 100.0 || delta > 40.0 {
                break;
            }
            xs.push(-age);
            ys.push((s.x, s.y));
            index = if index == 0 { 19 } else { index - 1 };
        }
        let oldest = previous;
        let duration_ms = newest.time_ms - oldest.time_ms;
        let offset = (newest.x - oldest.x, newest.y - oldest.y);
        if xs.len() < 3 {
            return Some(VelocityEstimate {
                pixels_per_second: (0.0, 0.0),
                confidence: 1.0,
                duration_ms,
                offset,
            });
        }
        let fit_x = quadratic_fit(&xs, &ys.iter().map(|p| p.0).collect::<Vec<_>>());
        let fit_y = quadratic_fit(&xs, &ys.iter().map(|p| p.1).collect::<Vec<_>>());
        match (fit_x, fit_y) {
            (Some(fx), Some(fy)) => Some(VelocityEstimate {
                pixels_per_second: (fx.slope * 1000.0, fy.slope * 1000.0),
                confidence: fx.confidence * fy.confidence,
                duration_ms,
                offset,
            }),
            _ => Some(VelocityEstimate {
                pixels_per_second: (0.0, 0.0),
                confidence: 1.0,
                duration_ms,
                offset,
            }),
        }
    }
}

/// Flutter's iOS fling tracker (`VT:295-390`): weighted mean of the last three
/// sample intervals, with the same 20-sample ring and 40 ms stop rule.
#[derive(Clone, Debug, PartialEq)]
pub struct IosFlingVelocityTracker {
    samples: [Option<VelocitySample>; 20],
    index: usize,
    last_sample_ms: i64,
}

impl Default for IosFlingVelocityTracker {
    fn default() -> Self {
        Self {
            samples: [None; 20],
            index: 0,
            last_sample_ms: 0,
        }
    }
}

impl IosFlingVelocityTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pointer cancel discards the fling sample history (`ACT:443-445`).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// `VT:308-318`: monotonic samples only; non-finite positions are ignored.
    pub fn add_position(&mut self, time_ms: i64, x: f64, y: f64) {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        if self.samples[self.index].is_some_and(|s| s.time_ms > time_ms) {
            return;
        }
        self.index = (self.index + 1) % 20;
        self.samples[self.index] = Some(VelocitySample { time_ms, x, y });
        self.last_sample_ms = time_ms;
    }

    fn previous_velocity(&self, back: usize) -> (f64, f64) {
        let end = (self.index + 20 - back) % 20;
        let start = (end + 19) % 20;
        let (Some(a), Some(b)) = (self.samples[start], self.samples[end]) else {
            return (0.0, 0.0);
        };
        let dt = b.time_ms - a.time_ms;
        if dt <= 0 {
            return (0.0, 0.0);
        }
        let dt = dt as f64;
        ((b.x - a.x) * 1000.0 / dt, (b.y - a.y) * 1000.0 / dt)
    }

    /// `VT:348-390`: 0.6/0.35/0.05 weights over intervals -2, -1, 0.
    pub fn estimate(&self, now_ms: i64) -> Option<VelocityEstimate> {
        let newest = self.samples[self.index]?;
        if now_ms.saturating_sub(self.last_sample_ms) > 40 {
            return Some(VelocityEstimate {
                pixels_per_second: (0.0, 0.0),
                confidence: 1.0,
                duration_ms: 0,
                offset: (0.0, 0.0),
            });
        }
        let mut oldest = newest;
        for step in 1..=20 {
            if let Some(s) = self.samples[(self.index + step) % 20] {
                oldest = s;
                break;
            }
        }
        let a = self.previous_velocity(2);
        let b = self.previous_velocity(1);
        let c = self.previous_velocity(0);
        Some(VelocityEstimate {
            pixels_per_second: (
                0.6 * a.0 + 0.35 * b.0 + 0.05 * c.0,
                0.6 * a.1 + 0.35 * b.1 + 0.05 * c.1,
            ),
            confidence: 1.0,
            duration_ms: newest.time_ms - oldest.time_ms,
            offset: (newest.x - oldest.x, newest.y - oldest.y),
        })
    }
}

/// Converts a drag-end primary velocity to the scroll velocity passed to
/// `goBallistic` (`ACT:418-440`): vertical scroll negates pointer velocity;
/// a reversed axis negates it again.
pub fn drag_end_scroll_velocity(primary_velocity: f64, reversed: bool) -> f64 {
    let v = -primary_velocity;
    if reversed {
        -v
    } else {
        v
    }
}

#[derive(Clone, Copy, Debug)]
struct PolynomialFit {
    slope: f64,
    confidence: f64,
}

/// `LSQ:88-205`: degree-2 QR/Gram-Schmidt fit with unit weights, specialized
/// to the `w[h] = 1` case used by `VT:216-218`.
fn quadratic_fit(x: &[f64], y: &[f64]) -> Option<PolynomialFit> {
    let m = x.len();
    if m < 3 {
        return None;
    }
    let n = 3usize;
    let mut a = vec![vec![0.0; m]; n];
    for h in 0..m {
        a[0][h] = 1.0;
        for i in 1..n {
            a[i][h] = a[i - 1][h] * x[h];
        }
    }
    let mut q = vec![vec![0.0; m]; n];
    let mut r = vec![vec![0.0; n]; n];
    for j in 0..n {
        q[j].copy_from_slice(&a[j]);
        for i in 0..j {
            let dot: f64 = (0..m).map(|h| q[j][h] * q[i][h]).sum();
            for h in 0..m {
                q[j][h] -= dot * q[i][h];
            }
        }
        let norm = q[j].iter().map(|v| v * v).sum::<f64>().sqrt();
        if norm < 1e-10 {
            return None;
        }
        let inverse = 1.0 / norm;
        for v in &mut q[j] {
            *v *= inverse;
        }
        for i in 0..n {
            r[j][i] = if i < j {
                0.0
            } else {
                (0..m).map(|h| q[j][h] * a[i][h]).sum()
            };
        }
    }
    let mut coefficients = [0.0; 3];
    for i in (0..n).rev() {
        let mut value: f64 = (0..m).map(|h| q[i][h] * y[h]).sum();
        for j in (i + 1..n).rev() {
            value -= r[i][j] * coefficients[j];
        }
        coefficients[i] = value / r[i][i];
    }
    let y_mean = y.iter().sum::<f64>() / m as f64;
    let mut error = 0.0;
    let mut total = 0.0;
    for h in 0..m {
        let mut term = 1.0;
        let mut residual = y[h] - coefficients[0];
        for coefficient in coefficients.iter().skip(1) {
            term *= x[h];
            residual -= term * coefficient;
        }
        error += residual * residual;
        let v = y[h] - y_mean;
        total += v * v;
    }
    let confidence = if total <= 1e-10 {
        1.0
    } else {
        1.0 - error / total
    };
    Some(PolynomialFit {
        slope: coefficients[1],
        confidence,
    })
}

/// `ClampingScrollSimulation` (SS:164-265) evaluated at absolute time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClampingScrollSimulation {
    pub position: f64,
    pub velocity: f64,
    pub friction: f64,
    pub duration: f64,
    pub distance: f64,
}

impl ClampingScrollSimulation {
    /// Returns `None` below Flutter's fling velocity tolerance (SP:1049-1050).
    pub fn new(
        position: f64,
        velocity: f64,
        friction: f64,
        tolerance: ScrollTolerance,
    ) -> Option<Self> {
        if velocity.abs() < tolerance.velocity {
            return None;
        }
        let reference = friction * PHYSICAL_COEFF / INFLEXION; // SS:221
        let duration = DECELERATION_RATE
            * INFLEXION
            * (velocity.abs() / reference).powf(1.0 / (DECELERATION_RATE - 1.0)); // SS:224-229
        let distance = velocity * duration / DECELERATION_RATE; // SS:234-248
        Some(Self {
            position,
            velocity,
            friction,
            duration,
            distance,
        })
    }

    /// SS:250-254.
    pub fn x(&self, time: f64) -> f64 {
        let t = (time / self.duration).clamp(0.0, 1.0);
        self.position + self.distance * (1.0 - (1.0 - t).powf(DECELERATION_RATE))
    }

    /// SS:256-260.
    pub fn dx(&self, time: f64) -> f64 {
        let t = (time / self.duration).clamp(0.0, 1.0);
        self.velocity * (1.0 - t).powf(DECELERATION_RATE - 1.0)
    }

    /// SS:262-264.
    pub fn is_done(&self, time: f64) -> bool {
        time >= self.duration
    }
}

/// `SpringDescription` (GS:15-18, 34-38).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpringDescription {
    pub mass: f64,
    pub stiffness: f64,
    pub damping: f64,
}

impl SpringDescription {
    pub fn with_damping_ratio(mass: f64, stiffness: f64, ratio: f64) -> Self {
        Self {
            mass,
            stiffness,
            damping: ratio * 2.0 * (mass * stiffness).sqrt(),
        }
    }

    /// Default `ClampingScrollPhysics` spring (SP:434-438).
    pub fn android_default() -> Self {
        Self::with_damping_ratio(0.5, 100.0, 1.1)
    }

    /// The spring `BouncingScrollPhysics` uses at the normal deceleration
    /// rate; it is the framework default spring (SP:434-441, SP:938-945).
    pub fn ios_default() -> Self {
        Self::with_damping_ratio(0.5, 100.0, 1.1)
    }

    /// The fast deceleration rate spring (SP:938-945).
    pub fn ios_fast() -> Self {
        Self::with_damping_ratio(0.3, 75.0, 1.3)
    }

    /// The iOS rubber band spring for a stationary release outside the
    /// extents (SP:761-800).
    ///
    /// The stiffness and damping are derived so the visible decay is
    /// `exp(-lambda * t)` with `lambda = log(2) / 0.07`, which is why the
    /// displacement halves every 70 ms.
    pub fn ios_rubber_band() -> Self {
        let lambda = RUBBER_BAND_LAMBDA;
        Self {
            mass: 1.0,
            stiffness: 1e5 * lambda * lambda,
            damping: (1e5 + 1.0) * lambda,
        }
    }
}

/// Flutter's scroll deceleration rates (SP:759, SP:892-895, SP:932-945).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollDecelerationRate {
    Normal,
    Fast,
}

impl ScrollDecelerationRate {
    /// SP:892-895: the constant deceleration handed to the friction model.
    pub fn constant_deceleration(self) -> f64 {
        match self {
            Self::Normal => 0.0,
            Self::Fast => 1400.0,
        }
    }

    /// SP:938-945.
    pub fn spring(self) -> SpringDescription {
        match self {
            Self::Normal => SpringDescription::ios_default(),
            Self::Fast => SpringDescription::ios_fast(),
        }
    }

    /// SP:932-934.
    pub fn max_fling_velocity(self) -> f64 {
        match self {
            Self::Normal => MAX_FLING_VELOCITY,
            Self::Fast => MAX_FLING_VELOCITY * 8.0,
        }
    }

    /// SP:904-905: bouncing physics needs a deliberate fling.
    pub fn min_fling_velocity(self) -> f64 {
        MIN_FLING_VELOCITY * 2.0
    }
}

/// The three analytic regimes selected by `c^2 - 4mk` (GS:284-295).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpringSolution {
    Critical { r: f64, c1: f64, c2: f64 },
    Overdamped { r1: f64, r2: f64, c1: f64, c2: f64 },
    Underdamped { r: f64, w: f64, c1: f64, c2: f64 },
}

impl SpringSolution {
    pub fn new(spring: SpringDescription, distance: f64, velocity: f64) -> Self {
        let disc = spring.damping * spring.damping - 4.0 * spring.mass * spring.stiffness;
        if disc > 0.0 {
            // GS:329-343.
            let r1 = (-spring.damping - disc.sqrt()) / (2.0 * spring.mass);
            let r2 = (-spring.damping + disc.sqrt()) / (2.0 * spring.mass);
            let c2 = (velocity - r1 * distance) / (r2 - r1);
            Self::Overdamped {
                r1,
                r2,
                c1: distance - c2,
                c2,
            }
        } else if disc < 0.0 {
            // GS:361-375.
            let w = (4.0 * spring.mass * spring.stiffness - spring.damping * spring.damping).sqrt()
                / (2.0 * spring.mass);
            let r = -(spring.damping / 2.0 / spring.mass);
            let c1 = distance;
            Self::Underdamped {
                r,
                w,
                c1,
                c2: (velocity - r * distance) / w,
            }
        } else {
            // GS:302-315.
            let r = -spring.damping / (2.0 * spring.mass);
            Self::Critical {
                r,
                c1: distance,
                c2: velocity - r * distance,
            }
        }
    }

    pub fn x(&self, t: f64) -> f64 {
        match *self {
            Self::Critical { r, c1, c2 } => (c1 + c2 * t) * (r * t).exp(), // GS:318-320
            Self::Overdamped { r1, r2, c1, c2 } => c1 * (r1 * t).exp() + c2 * (r2 * t).exp(), // GS:346-348
            Self::Underdamped { r, w, c1, c2 } => {
                (r * t).exp() * (c1 * (w * t).cos() + c2 * (w * t).sin())
            } // GS:378-381
        }
    }

    pub fn dx(&self, t: f64) -> f64 {
        match *self {
            Self::Critical { r, c1, c2 } => {
                let power = (r * t).exp();
                r * (c1 + c2 * t) * power + c2 * power
            } // GS:323-326
            Self::Overdamped { r1, r2, c1, c2 } => {
                c1 * r1 * (r1 * t).exp() + c2 * r2 * (r2 * t).exp()
            } // GS:351-353
            Self::Underdamped { r, w, c1, c2 } => {
                let power = (r * t).exp();
                let cosine = (w * t).cos();
                let sine = (w * t).sin();
                power * (c2 * w * cosine - c1 * w * sine) + r * power * (c2 * sine + c1 * cosine)
            } // GS:384-391
        }
    }
}

/// `ScrollSpringSimulation` with Flutter's snap-to-end (GS:265-280).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpringScrollSimulation {
    pub end: f64,
    pub solution: SpringSolution,
    pub tolerance: ScrollTolerance,
}

impl SpringScrollSimulation {
    pub fn new(
        spring: SpringDescription,
        start: f64,
        end: f64,
        velocity: f64,
        tolerance: ScrollTolerance,
    ) -> Self {
        Self {
            end,
            solution: SpringSolution::new(spring, start - end, velocity),
            tolerance,
        }
    }

    pub fn is_done(&self, t: f64) -> bool {
        // GS:257-260 tests the solution's relative position and velocity.
        near_zero(self.solution.x(t), self.tolerance.distance)
            && near_zero(self.solution.dx(t), self.tolerance.velocity)
    }

    pub fn x(&self, t: f64) -> f64 {
        if self.is_done(t) {
            self.end
        } else {
            self.end + self.solution.x(t)
        }
    }

    pub fn dx(&self, t: f64) -> f64 {
        if self.is_done(t) {
            0.0
        } else {
            self.solution.dx(t)
        }
    }
}

/// `FrictionSimulation` (FS:35-165) with the iOS drag coefficient.
///
/// The model is `x(t) = x0 + v0 * drag^t / dragLog - v0 / dragLog -
/// (c / 2) * t^2` and `dx(t) = v0 * drag^t - c * t` (FS:117-134). Flutter
/// solves the stop time with ten Newton iterations (FS:51-57, FS:154); the
/// same iteration is used here with the analytic derivative so the solve stays
/// finite, which agrees with Flutter inside the valid range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrictionSimulation {
    drag: f64,
    drag_log: f64,
    x0: f64,
    v0: f64,
    constant_deceleration: f64,
    final_time: f64,
    tolerance: ScrollTolerance,
}

impl FrictionSimulation {
    pub fn new(
        drag: f64,
        position: f64,
        velocity: f64,
        constant_deceleration: f64,
        tolerance: ScrollTolerance,
    ) -> Option<Self> {
        if !drag.is_finite()
            || drag <= 0.0
            || !position.is_finite()
            || !velocity.is_finite()
            || !constant_deceleration.is_finite()
        {
            return None;
        }
        let mut simulation = Self {
            drag,
            drag_log: drag.ln(),
            x0: position,
            v0: velocity,
            constant_deceleration: constant_deceleration * velocity.signum(),
            final_time: f64::INFINITY,
            tolerance,
        };
        simulation.final_time = simulation.solve_final_time();
        Some(simulation)
    }

    /// FS:51-57: Newton iteration on `dx(t) = 0`, ten steps from zero.
    fn solve_final_time(&self) -> f64 {
        if self.v0 == 0.0 && self.constant_deceleration == 0.0 {
            return f64::INFINITY;
        }
        let mut guess = 0.0f64;
        for _ in 0..10 {
            let value = self.dx_raw(guess);
            let derivative = self.v0 * (self.drag_log * guess).exp() * self.drag_log
                - self.constant_deceleration;
            if derivative == 0.0 || !derivative.is_finite() {
                return f64::INFINITY;
            }
            guess -= value / derivative;
        }
        if guess.is_finite() {
            guess
        } else {
            f64::INFINITY
        }
    }

    /// FS:118-126.
    pub fn x(&self, time: f64) -> f64 {
        if time > self.final_time {
            return self.final_x();
        }
        self.x0 + self.v0 * (self.drag_log * time).exp() / self.drag_log
            - self.v0 / self.drag_log
            - 0.5 * self.constant_deceleration * time * time
    }

    /// FS:128-134.
    pub fn dx(&self, time: f64) -> f64 {
        if time > self.final_time {
            return 0.0;
        }
        self.dx_raw(time)
    }

    fn dx_raw(&self, time: f64) -> f64 {
        self.v0 * (self.drag_log * time).exp() - self.constant_deceleration * time
    }

    /// FS:136-141.
    pub fn final_x(&self) -> f64 {
        if self.constant_deceleration == 0.0 {
            self.x0 - self.v0 / self.drag_log
        } else {
            self.x(self.final_time)
        }
    }

    /// FS:144-155: the time at which `x(t)` reaches `target`.
    ///
    /// Flutter's Newton iteration uses the simulated velocity as the
    /// derivative (FS:154), which is `dragLog` times the analytic slope; the
    /// iteration below keeps that form so it converges to the same root.
    pub fn time_at_x(&self, target: f64) -> f64 {
        if target == self.x0 {
            return 0.0;
        }
        if self.v0 == 0.0 {
            return f64::INFINITY;
        }
        let final_x = self.final_x();
        let unreachable = if self.v0 > 0.0 {
            target < self.x0 || target > final_x
        } else {
            target > self.x0 || target < final_x
        };
        if unreachable {
            return f64::INFINITY;
        }
        let mut guess = 0.0f64;
        for _ in 0..10 {
            let value = self.x_raw(guess) - target;
            let derivative = self.dx_raw(guess);
            if derivative == 0.0 || !derivative.is_finite() {
                return f64::INFINITY;
            }
            guess -= value / derivative;
        }
        if guess.is_finite() && guess >= 0.0 {
            guess
        } else {
            f64::INFINITY
        }
    }

    fn x_raw(&self, time: f64) -> f64 {
        self.x0 + self.v0 * (self.drag_log * time).exp() / self.drag_log
            - self.v0 / self.drag_log
            - 0.5 * self.constant_deceleration * time * time
    }

    /// FS:157-160.
    pub fn is_done(&self, time: f64) -> bool {
        self.dx(time).abs() < self.tolerance.velocity
    }
}

/// `BouncingScrollSimulation` (SS:18-132): iOS ballistic scrolling.
///
/// A fling inside the scroll extents decelerates with the iOS friction model
/// (SS:50-57). When that friction phase would carry the position past an
/// extent, the simulation hands over to a spring at the extent
/// (SS:59-72) whose start velocity is capped at
/// `maxSpringTransferVelocity` (SS:63, SS:79-81). A release that starts
/// outside the extents springs straight back to the nearest extent
/// (SS:43-48).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BouncingScrollSimulation {
    leading_extent: f64,
    trailing_extent: f64,
    friction: Option<FrictionSimulation>,
    spring: Option<SpringScrollSimulation>,
    spring_time: f64,
    tolerance: ScrollTolerance,
}

impl BouncingScrollSimulation {
    pub fn new(
        position: f64,
        velocity: f64,
        leading_extent: f64,
        trailing_extent: f64,
        spring: SpringDescription,
        constant_deceleration: f64,
        tolerance: ScrollTolerance,
    ) -> Option<Self> {
        if !position.is_finite()
            || !velocity.is_finite()
            || !leading_extent.is_finite()
            || !trailing_extent.is_finite()
            || leading_extent > trailing_extent
        {
            return None;
        }
        let mut simulation = Self {
            leading_extent,
            trailing_extent,
            friction: None,
            spring: None,
            spring_time: f64::NEG_INFINITY,
            tolerance,
        };
        if position < leading_extent {
            // SS:43-45.
            simulation.spring = Some(SpringScrollSimulation::new(
                spring,
                position,
                leading_extent,
                velocity,
                tolerance,
            ));
            return Some(simulation);
        }
        if position > trailing_extent {
            // SS:46-48.
            simulation.spring = Some(SpringScrollSimulation::new(
                spring,
                position,
                trailing_extent,
                velocity,
                tolerance,
            ));
            return Some(simulation);
        }
        let friction = FrictionSimulation::new(
            BOUNCE_FRICTION,
            position,
            velocity,
            constant_deceleration,
            tolerance,
        )?;
        let final_x = friction.final_x();
        if velocity > 0.0 && final_x > trailing_extent {
            // SS:59-65.
            let spring_time = friction.time_at_x(trailing_extent);
            let transfer = friction.dx(spring_time).min(MAX_SPRING_TRANSFER_VELOCITY);
            simulation.friction = Some(friction);
            simulation.spring_time = spring_time;
            simulation.spring = Some(SpringScrollSimulation::new(
                spring,
                trailing_extent,
                trailing_extent,
                transfer,
                tolerance,
            ));
        } else if velocity < 0.0 && final_x < leading_extent {
            // SS:66-72.
            let spring_time = friction.time_at_x(leading_extent);
            let transfer = friction.dx(spring_time).min(MAX_SPRING_TRANSFER_VELOCITY);
            simulation.friction = Some(friction);
            simulation.spring_time = spring_time;
            simulation.spring = Some(SpringScrollSimulation::new(
                spring,
                leading_extent,
                leading_extent,
                transfer,
                tolerance,
            ));
        } else {
            // SS:73-75.
            simulation.friction = Some(friction);
            simulation.spring_time = f64::INFINITY;
        }
        Some(simulation)
    }

    pub fn leading_extent(&self) -> f64 {
        self.leading_extent
    }

    pub fn trailing_extent(&self) -> f64 {
        self.trailing_extent
    }

    /// The time at which the friction phase hands over to the spring phase.
    /// Negative infinity means the simulation started outside the extents.
    pub fn spring_time(&self) -> f64 {
        self.spring_time
    }

    /// The velocity handed to the spring phase (SS:63, SS:70).
    pub fn spring_start_velocity(&self) -> Option<f64> {
        self.spring.map(|s| s.solution.dx(0.0))
    }

    /// SS:107-117: the phase selection and its time offset.
    fn phase(&self, time: f64) -> (bool, f64) {
        if time > self.spring_time {
            let offset = if self.spring_time.is_finite() {
                self.spring_time
            } else {
                0.0
            };
            (true, offset)
        } else {
            (false, 0.0)
        }
    }

    /// SS:119-120.
    pub fn x(&self, time: f64) -> f64 {
        let (spring_phase, offset) = self.phase(time);
        let t = time - offset;
        if spring_phase {
            match self.spring {
                Some(spring) => spring.x(t),
                None => self.friction.map(|f| f.x(t)).unwrap_or(f64::NAN),
            }
        } else {
            self.friction.map(|f| f.x(t)).unwrap_or(f64::NAN)
        }
    }

    /// SS:122-123.
    pub fn dx(&self, time: f64) -> f64 {
        let (spring_phase, offset) = self.phase(time);
        let t = time - offset;
        if spring_phase {
            match self.spring {
                Some(spring) => spring.dx(t),
                None => self.friction.map(|f| f.dx(t)).unwrap_or(f64::NAN),
            }
        } else {
            self.friction.map(|f| f.dx(t)).unwrap_or(f64::NAN)
        }
    }

    /// SS:125-126.
    pub fn is_done(&self, time: f64) -> bool {
        let (spring_phase, offset) = self.phase(time);
        let t = time - offset;
        if spring_phase {
            match self.spring {
                Some(spring) => spring.is_done(t),
                None => self.friction.map(|f| f.is_done(t)).unwrap_or(false),
            }
        } else {
            self.friction.map(|f| f.is_done(t)).unwrap_or(false)
        }
    }
}

/// The render-local motion selected for one scroll path (proposal §3.3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScrollMotion {
    Clamping(ClampingScrollSimulation),
    SpringBack(SpringScrollSimulation),
    Bouncing(BouncingScrollSimulation),
}

/// Closed-form driver keyed by an injected monotonic clock (proposal §3.2).
#[derive(Clone, Debug, PartialEq)]
pub struct ScrollDriver {
    pub start_ms: i64,
    pub x0: f64,
    pub v0: f64,
    pub min: i64,
    pub max: i64,
    pub motion: ScrollMotion,
}

impl ScrollDriver {
    /// SP:1021-1053: out-of-range springs back, an already-reached edge in the
    /// fling direction yields no activity, otherwise an in-range fling clamps.
    pub fn begin_fling(
        now_ms: i64,
        x0: f64,
        v0: f64,
        min: i64,
        max: i64,
        dpr: f64,
    ) -> Option<Self> {
        let tolerance = ScrollTolerance::for_dpr(dpr);
        if !x0.is_finite()
            || !v0.is_finite()
            || !dpr.is_finite()
            || dpr <= 0.0
            || !tolerance.distance.is_finite()
            || !tolerance.velocity.is_finite()
            || min > max
        {
            return None;
        }
        if x0 > max as f64 || x0 < min as f64 {
            let end = if x0 > max as f64 { max } else { min };
            return Some(Self {
                start_ms: now_ms,
                x0,
                v0,
                min,
                max,
                motion: ScrollMotion::SpringBack(SpringScrollSimulation::new(
                    SpringDescription::android_default(),
                    x0,
                    end as f64,
                    // SP:1035 passes min(0, velocity) for out-of-range release.
                    v0.min(0.0),
                    tolerance,
                )),
            });
        }
        let v0 = v0.clamp(-MAX_FLING_VELOCITY, MAX_FLING_VELOCITY);
        if v0.abs() < MIN_FLING_VELOCITY.max(tolerance.velocity) {
            return None;
        }
        if (v0 > 0.0 && x0 >= max as f64) || (v0 < 0.0 && x0 <= min as f64) {
            return None;
        }
        Some(Self {
            start_ms: now_ms,
            x0,
            v0,
            min,
            max,
            motion: ScrollMotion::Clamping(ClampingScrollSimulation::new(
                x0,
                v0,
                FLING_FRICTION,
                tolerance,
            )?),
        })
    }

    fn seconds(&self, now_ms: i64) -> f64 {
        (now_ms - self.start_ms) as f64 / 1000.0
    }

    /// `BouncingScrollPhysics.createBallisticSimulation` (SP:878-899).
    ///
    /// A stationary release inside the extents produces no motion. A
    /// stationary release outside the extents uses the rubber band spring
    /// (SP:882, SP:886), any other release uses the rate's spring.
    pub fn begin_bounce(
        now_ms: i64,
        x0: f64,
        v0: f64,
        min: i64,
        max: i64,
        dpr: f64,
        rate: ScrollDecelerationRate,
    ) -> Option<Self> {
        let tolerance = ScrollTolerance::for_dpr(dpr);
        if !x0.is_finite() || !v0.is_finite() || !dpr.is_finite() || dpr <= 0.0 || min > max {
            return None;
        }
        let is_stationary = v0.abs() < tolerance.velocity;
        let out_of_range = x0 < min as f64 || x0 > max as f64;
        if is_stationary && !out_of_range {
            return None;
        }
        let spring = if is_stationary {
            SpringDescription::ios_rubber_band()
        } else {
            rate.spring()
        };
        let simulation = BouncingScrollSimulation::new(
            x0,
            v0,
            min as f64,
            max as f64,
            spring,
            rate.constant_deceleration(),
            tolerance,
        )?;
        Some(Self {
            start_ms: now_ms,
            x0,
            v0,
            min,
            max,
            motion: ScrollMotion::Bouncing(simulation),
        })
    }

    pub fn x(&self, now_ms: i64) -> f64 {
        let t = self.seconds(now_ms);
        match self.motion {
            ScrollMotion::Clamping(s) => s.x(t),
            ScrollMotion::SpringBack(s) => s.x(t),
            ScrollMotion::Bouncing(s) => s.x(t),
        }
    }

    pub fn dx(&self, now_ms: i64) -> f64 {
        let t = self.seconds(now_ms);
        match self.motion {
            ScrollMotion::Clamping(s) => s.dx(t),
            ScrollMotion::SpringBack(s) => s.dx(t),
            ScrollMotion::Bouncing(s) => s.dx(t),
        }
    }

    pub fn is_done(&self, now_ms: i64) -> bool {
        let t = self.seconds(now_ms);
        match self.motion {
            ScrollMotion::Clamping(s) => s.is_done(t),
            ScrollMotion::SpringBack(s) => s.is_done(t),
            ScrollMotion::Bouncing(s) => s.is_done(t),
        }
    }

    /// R2: round once, at the integer pixel boundary, before layout uses it.
    /// `SP:1000-1014`: a clamping fling is bounded by the scroll extent;
    /// the simulation velocity keeps decaying, so no hard velocity cut.
    /// A bounce keeps the overshoot: it is not clamped to the extents.
    pub fn offset_px(&self, now_ms: i64) -> i64 {
        let x = self.x(now_ms);
        let x = match self.motion {
            ScrollMotion::Clamping(_) => x.clamp(self.min as f64, self.max as f64),
            ScrollMotion::SpringBack(_) => x,
            ScrollMotion::Bouncing(_) => x,
        };
        x.round() as i64
    }
}
