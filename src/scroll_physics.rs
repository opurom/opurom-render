//! Deterministic Android scroll physics driver, phase 1.
//!
//! Closed-form evaluations only: no frame stepping, no wall clock, no IR or
//! host ABI change. Units are logical pixels, seconds and pixels/second.
//!
//! Source anchors (verified blobs at flutter-golden HEAD ab2912035e):
//! - SS scroll_simulation.dart `2197765bf4b832b7661e42dcf3bc65adc3462d6c`:
//!   `201`, `204`, `208-215`, `218-260`.
//! - SP scroll_physics.dart `a514baf880b1a0e2d204a9e0ec584d6a839b580d`:
//!   `434-438`, `462-467`, `1021-1053`.
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

/// The render-local motion selected for one scroll path (proposal §3.3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScrollMotion {
    Clamping(ClampingScrollSimulation),
    SpringBack(SpringScrollSimulation),
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

    pub fn x(&self, now_ms: i64) -> f64 {
        let t = self.seconds(now_ms);
        match self.motion {
            ScrollMotion::Clamping(s) => s.x(t),
            ScrollMotion::SpringBack(s) => s.x(t),
        }
    }

    pub fn dx(&self, now_ms: i64) -> f64 {
        let t = self.seconds(now_ms);
        match self.motion {
            ScrollMotion::Clamping(s) => s.dx(t),
            ScrollMotion::SpringBack(s) => s.dx(t),
        }
    }

    pub fn is_done(&self, now_ms: i64) -> bool {
        let t = self.seconds(now_ms);
        match self.motion {
            ScrollMotion::Clamping(s) => s.is_done(t),
            ScrollMotion::SpringBack(s) => s.is_done(t),
        }
    }

    /// R2: round once, at the integer pixel boundary, before layout uses it.
    /// `SP:1000-1014`: a clamping fling is bounded by the scroll extent;
    /// the simulation velocity keeps decaying, so no hard velocity cut.
    pub fn offset_px(&self, now_ms: i64) -> i64 {
        let x = self.x(now_ms);
        let x = match self.motion {
            ScrollMotion::Clamping(_) => x.clamp(self.min as f64, self.max as f64),
            ScrollMotion::SpringBack(_) => x,
        };
        x.round() as i64
    }
}
