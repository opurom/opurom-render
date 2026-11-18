//! Scroll physics parity harness (SCROLL_PHYSICS_PROPOSAL.md §4).
//! The harness below re-implements the cited formulas line-for-line and
//! compares them with the render driver; it never calls the driver's math.
//! R1: cross-target libm bit equality is not claimed, hence ±0.5 px gates.

use opurom_render::{digest_hex, parse_ir, scroll_physics::*, Renderer, StateView, SETTLED_MS};

fn compat_renderer(width: u32, height: u32) -> Renderer {
    let mut renderer = Renderer::new(width, height);
    renderer.set_quality(opurom_render::QualityProfile::Compat);
    renderer
}

const COUNTER: &str = "../opurom-examples/golden/counter.opuir.json";
const START_MS: i64 = 10_000;

// Verified blobs at flutter-golden HEAD ab2912035e. Appendix A of the proposal
// lists older hashes for SP/SS/GS/TOL/GC/VT; the anchored lines were re-read.
const SP_BLOB: &str = "a514baf880b1a0e2d204a9e0ec584d6a839b580d";
const SS_BLOB: &str = "2197765bf4b832b7661e42dcf3bc65adc3462d6c";
const GS_BLOB: &str = "62a89b8b6e14342ea5903cc339e3696f250253e1";
const TOL_BLOB: &str = "ab4a7a5036283824d7cf9d87d96f58fbc78a529b";

fn canonical_ms(i: usize) -> i64 {
    (i as f64 * 1000.0 / 60.0).round() as i64
}

// -- Harness: independent copy of the SS:201-264 formulas. -----------------
fn harness_fling(v0: f64, friction: f64) -> (f64, f64) {
    let rate = 0.78_f64.ln() / 0.9_f64.ln(); // SS:201
    let inflexion = 0.35_f64; // SS:204
    let physical = 9.80665 * 39.37 * 160.0 * 0.84; // SS:208-215
    let reference = friction * physical / inflexion; // SS:221
    let duration = rate * inflexion * (v0.abs() / reference).powf(1.0 / (rate - 1.0)); // SS:224-229
    let distance = v0 * duration / rate; // SS:234-248
    (duration, distance)
}

fn harness_fling_x(position: f64, distance: f64, duration: f64, time: f64) -> f64 {
    let rate = 0.78_f64.ln() / 0.9_f64.ln();
    let t = (time / duration).clamp(0.0, 1.0);
    position + distance * (1.0 - (1.0 - t).powf(rate)) // SS:250-254
}

fn harness_fling_dx(v0: f64, duration: f64, time: f64) -> f64 {
    let rate = 0.78_f64.ln() / 0.9_f64.ln();
    let t = (time / duration).clamp(0.0, 1.0);
    v0 * (1.0 - t).powf(rate - 1.0) // SS:256-260
}

#[derive(Clone, Copy)]
enum HarnessSpring {
    Critical { r: f64, c1: f64, c2: f64 },
    Over { r1: f64, r2: f64, c1: f64, c2: f64 },
    Under { r: f64, w: f64, c1: f64, c2: f64 },
}

impl HarnessSpring {
    fn new(mass: f64, stiffness: f64, damping: f64, distance: f64, velocity: f64) -> Self {
        let disc = damping * damping - 4.0 * mass * stiffness; // GS:286
        if disc > 0.0 {
            let r1 = (-damping - disc.sqrt()) / (2.0 * mass); // GS:332
            let r2 = (-damping + disc.sqrt()) / (2.0 * mass); // GS:333
            let c2 = (velocity - r1 * distance) / (r2 - r1); // GS:334
            Self::Over {
                r1,
                r2,
                c1: distance - c2,
                c2,
            } // GS:335
        } else if disc < 0.0 {
            let w = (4.0 * mass * stiffness - damping * damping).sqrt() / (2.0 * mass); // GS:363
            let r = -(damping / 2.0 / mass); // GS:364
            Self::Under {
                r,
                w,
                c1: distance,
                c2: (velocity - r * distance) / w,
            } // GS:365-366
        } else {
            let r = -damping / (2.0 * mass); // GS:304
            Self::Critical {
                r,
                c1: distance,
                c2: velocity - r * distance,
            } // GS:305-306
        }
    }

    fn x(&self, t: f64) -> f64 {
        match *self {
            Self::Critical { r, c1, c2 } => (c1 + c2 * t) * (r * t).exp(), // GS:318-320
            Self::Over { r1, r2, c1, c2 } => c1 * (r1 * t).exp() + c2 * (r2 * t).exp(), // GS:346-348
            Self::Under { r, w, c1, c2 } => {
                (r * t).exp() * (c1 * (w * t).cos() + c2 * (w * t).sin())
            } // GS:378-381
        }
    }

    fn dx(&self, t: f64) -> f64 {
        match *self {
            Self::Critical { r, c1, c2 } => {
                let power = (r * t).exp();
                r * (c1 + c2 * t) * power + c2 * power
            } // GS:323-326
            Self::Over { r1, r2, c1, c2 } => c1 * r1 * (r1 * t).exp() + c2 * r2 * (r2 * t).exp(), // GS:351-353
            Self::Under { r, w, c1, c2 } => {
                let power = (r * t).exp();
                let cosine = (w * t).cos();
                let sine = (w * t).sin();
                power * (c2 * w * cosine - c1 * w * sine) + r * power * (c2 * sine + c1 * cosine)
            } // GS:384-391
        }
    }
}

fn harness_near_zero(v: f64, eps: f64) -> bool {
    v == 0.0 || (v > -eps && v < eps) // TOL:14-23, foundation/utils.dart:8-21
}

// -- V1 ---------------------------------------------------------------------
#[test]
fn v1_android_fling_matches_closed_form() {
    let (v0, x0) = (2000.0, 0.0);
    let driver = ScrollDriver::begin_fling(START_MS, x0, v0, i64::MIN / 4, i64::MAX / 4, 1.0)
        .expect("2000 px/s above min fling");
    let (duration, distance) = harness_fling(v0, FLING_FRICTION);
    assert!((duration - 0.7633484872).abs() < 1e-6);
    assert!((distance - 647.3987784).abs() < 1e-4);
    let mut max_x = 0.0f64;
    let mut max_dx = 0.0f64;
    let mut max_t_ms = 0.0f64;
    for i in 0..=180 {
        let expected_ms = i as f64 * 1000.0 / 60.0;
        let now_ms = START_MS + expected_ms.round() as i64;
        max_t_ms = max_t_ms.max((now_ms - START_MS) as f64 - expected_ms);
        let t = (now_ms - START_MS) as f64 / 1000.0;
        let want_x = harness_fling_x(x0, distance, duration, t);
        let want_dx = harness_fling_dx(v0, duration, t);
        max_x = max_x.max((driver.x(now_ms) - want_x).abs());
        max_dx = max_dx.max((driver.dx(now_ms) - want_dx).abs());
    }
    assert!(
        max_t_ms <= 1000.0 / 60.0,
        "t error {max_t_ms} ms > one frame"
    );
    assert!(max_x <= 0.5, "x error {max_x} px > 0.5");
    assert!(max_dx <= 0.5, "dx error {max_dx} px/s > 0.5");
    assert!(!driver.is_done(START_MS));
    assert!(driver.is_done(START_MS + 1000));
}

// -- V4 ---------------------------------------------------------------------
#[test]
fn v4_spring_three_regimes_and_snap_to_extent() {
    let tol = ScrollTolerance::for_dpr(1.0);
    // SP:434-438 default: mass 0.5, stiffness 100, ratio 1.1 -> overdamped.
    let default = SpringDescription::android_default();
    assert!((default.mass - 0.5).abs() < f64::EPSILON);
    assert!((default.stiffness - 100.0).abs() < f64::EPSILON);
    assert!((default.damping - 1.1 * 2.0 * (0.5_f64 * 100.0_f64).sqrt()).abs() < 1e-12);

    let end = 600.0;
    let start = 640.0;
    let velocity = -200.0;
    let driver = ScrollDriver::begin_fling(START_MS, start, velocity, 0, 600, 1.0)
        .expect("out-of-range release springs back");
    assert_eq!(
        driver.motion,
        ScrollMotion::SpringBack(SpringScrollSimulation::new(
            default, start, end, velocity, tol,
        ))
    );

    // Harness: all three regimes from GS:284-295, independent of the driver.
    for ratio in [0.5_f64, 1.0, 1.1] {
        let spring = SpringDescription::with_damping_ratio(0.5, 100.0, ratio);
        let sim = SpringScrollSimulation::new(spring, start, end, velocity, tol);
        let harness = HarnessSpring::new(
            spring.mass,
            spring.stiffness,
            spring.damping,
            start - end,
            velocity,
        );
        let disc = spring.damping * spring.damping - 4.0 * spring.mass * spring.stiffness;
        let expected_variant = if disc > 0.0 {
            "over"
        } else if disc < 0.0 {
            "under"
        } else {
            "critical"
        };
        let actual_variant = match sim.solution {
            SpringSolution::Underdamped { .. } => "under",
            SpringSolution::Critical { .. } => "critical",
            SpringSolution::Overdamped { .. } => "over",
        };
        assert_eq!(actual_variant, expected_variant);
        for i in 0..=180 {
            let now_ms = START_MS + canonical_ms(i);
            let t = (now_ms - START_MS) as f64 / 1000.0;
            let done = harness_near_zero(harness.x(t), tol.distance)
                && harness_near_zero(harness.dx(t), tol.velocity);
            let want_x = if done { end } else { end + harness.x(t) };
            let want_dx = if done { 0.0 } else { harness.dx(t) };
            assert!((sim.x(t) - want_x).abs() <= 0.5, "spring x at {t}s");
            assert!((sim.dx(t) - want_dx).abs() <= 0.5, "spring dx at {t}s");
        }
    }

    // Exact critical discriminant (mass 1, stiffness 100, damping 20): GS:302.
    let exact_critical = SpringDescription {
        mass: 1.0,
        stiffness: 100.0,
        damping: 20.0,
    };
    let sim = SpringScrollSimulation::new(exact_critical, start, end, velocity, tol);
    assert!(matches!(sim.solution, SpringSolution::Critical { .. }));
    let harness = HarnessSpring::new(1.0, 100.0, 20.0, start - end, velocity);
    for i in 0..=60 {
        let now_ms = START_MS + canonical_ms(i);
        let t = (now_ms - START_MS) as f64 / 1000.0;
        let done = harness_near_zero(harness.x(t), tol.distance)
            && harness_near_zero(harness.dx(t), tol.velocity);
        let want_x = if done { end } else { end + harness.x(t) };
        assert!((sim.x(t) - want_x).abs() <= 0.5, "critical x at {t}s");
    }

    // V4 hard close: after settle, the position snaps exactly to the extent.
    let settled = START_MS + 1000;
    assert!(
        driver.is_done(settled),
        "default spring must settle within 1 s"
    );
    assert_eq!(driver.x(settled), end, "snap exactly to extent");
    assert_eq!(driver.offset_px(settled), end as i64);
    assert_eq!(driver.dx(settled), 0.0);
    // SP:1043-1047: fling into an already reached edge starts no activity.
    assert!(ScrollDriver::begin_fling(START_MS, 600.0, 200.0, 0, 600, 1.0).is_none());
}

// -- V5 ---------------------------------------------------------------------
#[test]
fn v5_120_hz_and_60_hz_are_byte_equal() {
    let params = (0.0_f64, 2000.0_f64, 0_i64, 1_000_000_i64);
    let at60 =
        ScrollDriver::begin_fling(START_MS, params.0, params.1, params.2, params.3, 1.0).unwrap();
    let at120 =
        ScrollDriver::begin_fling(START_MS, params.0, params.1, params.2, params.3, 1.0).unwrap();
    // Sample the 120 Hz driver at its own interleaved timestamps first; the
    // closed form must not accumulate any frame-step state.
    for i in 1..360 {
        let _ = at120.offset_px(START_MS + (i as f64 * 1000.0 / 120.0).round() as i64);
    }
    for i in 0..=180 {
        let now_ms = START_MS + canonical_ms(i);
        assert_eq!(
            at60.x(now_ms).to_bits().to_le_bytes(),
            at120.x(now_ms).to_bits().to_le_bytes(),
            "position bytes at {now_ms}"
        );
        assert_eq!(
            at60.offset_px(now_ms).to_le_bytes(),
            at120.offset_px(now_ms).to_le_bytes(),
            "integer scroll offset bytes at {now_ms}"
        );
    }
}

// -- V6 ---------------------------------------------------------------------
#[test]
fn v6_repeat_render_and_driver_are_byte_equal() {
    let source = include_str!("../src/scroll_physics.rs");
    assert!(
        !source.contains("SystemTime") && !source.contains("Instant::now"),
        "driver must not read a wall clock"
    );
    assert!(
        !source.contains("crate::anim"),
        "driver must not use anim.rs"
    );
    for anchor in [SP_BLOB, SS_BLOB, GS_BLOB, TOL_BLOB] {
        assert!(source.contains(anchor), "missing source anchor {anchor}");
    }
    assert!(
        source.contains("Appendix A"),
        "stale spec anchors must be recorded"
    );

    let text = std::fs::read_to_string(COUNTER).unwrap();
    let ir = parse_ir(&text).unwrap();
    let state = StateView::from_ir(&ir);
    let mut digests = [[0u8; 8]; 2];
    for run in 0..2 {
        let driver = ScrollDriver::begin_fling(START_MS, 0.0, 2000.0, 0, 1_000_000, 1.0).unwrap();
        let mut renderer = compat_renderer(320, 240);
        let mut out = Vec::new();
        for i in 0..=60 {
            let now_ms = START_MS + canonical_ms(i);
            let offset = driver.offset_px(now_ms);
            assert_eq!(offset, driver.offset_px(now_ms), "offset query is pure");
            let frame = renderer.render_at(&ir, &state, offset as i64 + SETTLED_MS);
            out.extend_from_slice(digest_hex(&frame.pixels).as_bytes());
        }
        digests[run] = opurom_render::digest(&out).to_le_bytes();
    }
    assert_eq!(digests[0], digests[1], "two runs must be byte-equal");
}

// -- V7 ---------------------------------------------------------------------
#[test]
fn v7_no_fling_keeps_goldens_unchanged() {
    assert!(
        ScrollDriver::begin_fling(START_MS, 0.0, 10.0, 0, 1000, 1.0).is_none(),
        "below min fling velocity -> no activity"
    );
    let text = std::fs::read_to_string(COUNTER).unwrap();
    let ir = parse_ir(&text).unwrap();
    let state = StateView::from_ir(&ir);
    assert_eq!(
        digest_hex(&compat_renderer(320, 240).render(&ir, &state).pixels),
        "8d45f412996a99c6"
    );
    assert_eq!(
        digest_hex(&compat_renderer(800, 600).render(&ir, &state).pixels),
        "318beb1dd337bc4e"
    );
}
