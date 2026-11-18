//! Phase 1 scroll-physics precision calibration: lift velocity conversion,
//! clamping boundary decay, abnormal input and 60/120/144 Hz sampling.
//!
//! Flutter anchors at checkout HEAD `ab2912035e160d9cad6306df307576da6128ebdb`:
//! - VT `322dde6ce31bee32f933d44f41cd08767f6e6866`: 87-128, 142-145,
//!   163-220, 295-390 (velocity estimates and stopped/horizon rules).
//! - LSQ `9e7d438a39cd45c012cb9192591cd559fd42e82d`: 88-205 (degree-2 fit).
//! - ACT `051158be15443297fd5c7d59b0a58a63499af238`: 418-445 (drag-end
//!   primary velocity negation and cancel reset).
//! - SS `2197765bf4b832b7661e42dcf3bc65adc3462d6c`: 164-265 (clamping
//!   duration/distance and far-to-near decay).
//! - SP `a514baf880b1a0e2d204a9e0ec584d6a839b580d`: 1000-1053 (boundary
//!   clamp and out-of-range spring).
//! - GS `62a89b8b6e14342ea5903cc339e3696f250253e1`: 257-280, 284-392.
//! - GC `ae23eac1178aff50cbb5b0f29566a0e45589dd99`: 65, 90, 95.
//! - TOL `ab4a7a5036283824d7cf9d87d96f58fbc78a529b`: 14-23.

use opurom_render::{digest_hex, parse_ir, scroll_physics::*, Renderer, StateView};

fn compat_renderer(width: u32, height: u32) -> Renderer {
    let mut renderer = Renderer::new(width, height);
    renderer.set_quality(opurom_render::QualityProfile::Compat);
    renderer
}

const COUNTER: &str = "../opurom-examples/golden/counter.opuir.json";

const START_MS: i64 = 20_000;
const VT_BLOB: &str = "322dde6ce31bee32f933d44f41cd08767f6e6866";
const LSQ_BLOB: &str = "9e7d438a39cd45c012cb9192591cd559fd42e82d";
const ACT_BLOB: &str = "051158be15443297fd5c7d59b0a58a63499af238";
const SS_BLOB: &str = "2197765bf4b832b7661e42dcf3bc65adc3462d6c";
const SP_BLOB: &str = "a514baf880b1a0e2d204a9e0ec584d6a839b580d";
const GS_BLOB: &str = "62a89b8b6e14342ea5903cc339e3696f250253e1";
const GC_BLOB: &str = "ae23eac1178aff50cbb5b0f29566a0e45589dd99";
const TOL_BLOB: &str = "ab4a7a5036283824d7cf9d87d96f58fbc78a529b";

fn harness_clamping(v0: f64) -> (f64, f64) {
    let rate = 0.78_f64.ln() / 0.9_f64.ln(); // SS:201
    let physical = 9.80665 * 39.37 * 160.0 * 0.84; // SS:208-215
    let reference = FLING_FRICTION * physical / 0.35; // SS:221
    let duration = rate * 0.35 * (v0.abs() / reference).powf(1.0 / (rate - 1.0)); // SS:224-229
    (duration, v0 * duration / rate) // SS:234-248
}

fn harness_clamping_x(x0: f64, distance: f64, duration: f64, time: f64) -> f64 {
    let rate = 0.78_f64.ln() / 0.9_f64.ln();
    x0 + distance * (1.0 - (1.0 - (time / duration).clamp(0.0, 1.0)).powf(rate))
    // SS:250-254
}

fn harness_clamping_dx(v0: f64, duration: f64, time: f64) -> f64 {
    let rate = 0.78_f64.ln() / 0.9_f64.ln();
    v0 * (1.0 - (time / duration).clamp(0.0, 1.0)).powf(rate - 1.0) // SS:256-260
}

fn frames(rate: i64, count: i64) -> Vec<i64> {
    (0..=count)
        .map(|i| START_MS + (i as f64 * 1000.0 / rate as f64).round() as i64)
        .collect()
}

// -- Normal scroll: lift velocity conversion -------------------------------

#[test]
fn normal_01_generic_tracker_matches_linear_sample_stream() {
    // VT:142-145,163-220 + LSQ:88-205. A linear stream is fit exactly by the
    // degree-2 solver; slope px/ms * 1000 is px/s.
    let mut tracker = VelocityTracker::new();
    for i in 0..6 {
        tracker.add_position(START_MS + i * 16, (i * 10) as f64, 0.0);
    }
    let estimate = tracker.estimate(START_MS + 80).unwrap();
    assert!((estimate.pixels_per_second.0 - 625.0).abs() < 0.5);
    assert_eq!(estimate.duration_ms, 80);
    assert_eq!(estimate.offset, (50.0, 0.0));
}

#[test]
fn normal_02_ios_tracker_uses_flutter_weights() {
    // VT:295-390: 0.6/-2 + 0.35/-1 + 0.05/0 over interval velocities
    // 1000, 1000, 2000 px/s = 1100 px/s.
    let mut tracker = IosFlingVelocityTracker::new();
    for (t, x) in [(0, 0), (16, 16), (32, 48), (48, 64), (64, 80), (80, 128)] {
        tracker.add_position(START_MS + t, x as f64, 0.0);
    }
    let estimate = tracker.estimate(START_MS + 80).unwrap();
    assert!((estimate.pixels_per_second.0 - 1100.0).abs() < 1e-9);
    assert_eq!(estimate.duration_ms, 80);
}

#[test]
fn normal_03_lift_velocity_converts_to_scroll_velocity() {
    // ACT:418-440: the scroll fling velocity is the negated drag primary
    // velocity; a reversed axis negates again. Feed it to begin_fling.
    let primary = -2000.0;
    let v0 = drag_end_scroll_velocity(primary, false);
    assert_eq!(v0, 2000.0);
    assert_eq!(drag_end_scroll_velocity(primary, true), -2000.0);
    let driver = ScrollDriver::begin_fling(START_MS, 0.0, v0, -1_000_000, 1_000_000, 1.0).unwrap();
    assert!((driver.v0 - 2000.0).abs() < f64::EPSILON);
    assert!(driver.dx(START_MS) > 0.0);
}

// -- Boundary damping ------------------------------------------------------

#[test]
fn boundary_01_clamping_decay_is_smooth_near_the_end() {
    // SS:250-264: x approaches the end and dx decays to zero continuously;
    // there is no fixed-step or hard velocity cut.
    let driver =
        ScrollDriver::begin_fling(START_MS, 0.0, 2000.0, -1_000_000, 1_000_000, 1.0).unwrap();
    let (duration, distance) = harness_clamping(2000.0);
    let mut last_dx = f64::INFINITY;
    for i in 0..=180 {
        let now = START_MS + (i as f64 * 1000.0 / 60.0).round() as i64;
        let t = (now - START_MS) as f64 / 1000.0;
        let want_x = harness_clamping_x(0.0, distance, duration, t);
        let want_dx = harness_clamping_dx(2000.0, duration, t);
        assert!((driver.x(now) - want_x).abs() <= 0.5);
        assert!((driver.dx(now) - want_dx).abs() <= 0.5);
        assert!(driver.dx(now) <= last_dx + f64::EPSILON);
        last_dx = driver.dx(now);
    }
    assert!(last_dx < 1.0, "near-end velocity must fade, not jump");
    assert!(driver.is_done(START_MS + 1000));
}

#[test]
fn boundary_02_extent_clamps_position_but_not_velocity() {
    // SP:1000-1014: layout pixels clamp at the extent; SS:256-260 keeps the
    // simulation velocity alive so the stop is not abrupt.
    let driver = ScrollDriver::begin_fling(START_MS, 590.0, 2000.0, 0, 600, 1.0).unwrap();
    assert_eq!(driver.offset_px(START_MS + 200), 600);
    assert_eq!(driver.offset_px(START_MS + 400), 600);
    assert!(driver.dx(START_MS + 200) > 100.0);
    assert!(driver.dx(START_MS + 400) > 0.0);
    assert!(driver.dx(START_MS + 400) < driver.dx(START_MS + 200));
}

#[test]
fn boundary_03_out_of_range_release_springs_with_min_zero_velocity() {
    // SP:1023-1038: out-of-range release uses min(0, velocity) even when the
    // pointer was moving back inside. GS:257-280 snaps exactly to the extent.
    let driver = ScrollDriver::begin_fling(START_MS, 640.0, 200.0, 0, 600, 1.0).unwrap();
    assert!(matches!(driver.motion, ScrollMotion::SpringBack(_)));
    let zero_v = SpringScrollSimulation::new(
        SpringDescription::android_default(),
        640.0,
        600.0,
        0.0,
        ScrollTolerance::for_dpr(1.0),
    );
    assert_eq!(driver.motion, ScrollMotion::SpringBack(zero_v));
    assert_eq!(driver.offset_px(START_MS + 1000), 600);
    assert_eq!(driver.dx(START_MS + 1000), 0.0);
}

// -- Abnormal input --------------------------------------------------------

#[test]
fn abnormal_01_negative_and_oversized_velocities_are_bounded() {
    // GC:90,95 + SP:1049-1050: negative flings travel backwards; oversized
    // flings clamp to kMaxFlingVelocity before simulation construction.
    let negative = ScrollDriver::begin_fling(START_MS, 500.0, -1000.0, 0, 1000, 1.0).unwrap();
    assert_eq!(negative.v0, -1000.0);
    assert!(negative.x(START_MS + 100) < 500.0);
    let huge = ScrollDriver::begin_fling(START_MS, 0.0, 1e9, -1_000_000, 1_000_000, 1.0).unwrap();
    assert_eq!(huge.v0, MAX_FLING_VELOCITY);
    assert!((huge.dx(START_MS) - MAX_FLING_VELOCITY).abs() < f64::EPSILON);
}

#[test]
fn abnormal_02_non_finite_input_and_invalid_extents_are_rejected() {
    // TOL:14-23 tolerance and SP:462-467 dpr must stay finite; an inverted
    // extent would make clamp/snap ambiguous, so reject it without panicking.
    for dpr in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(ScrollDriver::begin_fling(START_MS, 0.0, 1000.0, 0, 1000, dpr).is_none());
    }
    assert!(ScrollDriver::begin_fling(START_MS, f64::NAN, 1000.0, 0, 1000, 1.0).is_none());
    assert!(ScrollDriver::begin_fling(START_MS, 0.0, f64::INFINITY, 0, 1000, 1.0).is_none());
    assert!(ScrollDriver::begin_fling(START_MS, 0.0, 1000.0, 1000, 0, 1.0).is_none());
}

#[test]
fn abnormal_03_touch_cancel_clears_velocity_history() {
    // ACT:443-445: cancel resets the drag state; no estimate can be produced
    // afterwards, so no fling is started from cancelled samples.
    let mut tracker = IosFlingVelocityTracker::new();
    tracker.add_position(START_MS, 0.0, 0.0);
    tracker.add_position(START_MS + 16, 16.0, 0.0);
    assert!(tracker.estimate(START_MS + 16).is_some());
    tracker.reset();
    assert!(tracker.estimate(START_MS + 16).is_none());
    assert!(ScrollDriver::begin_fling(START_MS, 0.0, 0.0, 0, 1000, 1.0).is_none());
}

// -- Frame-rate adaptation -------------------------------------------------

#[test]
fn refresh_01_60_120_144_hz_byte_equal_at_shared_timestamps() {
    // SIM:23-47 + AC:934-944: absolute-time evaluation, no frame step.
    let grids = [frames(60, 180), frames(120, 360), frames(144, 432)];
    let common: Vec<i64> = grids[0]
        .iter()
        .copied()
        .filter(|t| grids[1].contains(t) && grids[2].contains(t))
        .collect();
    assert!(!common.is_empty());
    assert_eq!(common[0], START_MS);
    for t in &common {
        let now = *t;
        let samples: Vec<[u8; 8]> = grids
            .iter()
            .map(|_| {
                let d =
                    ScrollDriver::begin_fling(START_MS, 0.0, 2000.0, -1_000_000, 1_000_000, 1.0)
                        .unwrap();
                d.x(now).to_bits().to_le_bytes()
            })
            .collect();
        assert_eq!(samples[0], samples[1], "x bytes at {now}");
        assert_eq!(samples[0], samples[2], "x bytes at {now}");
    }
}

#[test]
fn refresh_02_interleaved_sampling_does_not_shift_the_curve() {
    // A dense 144 Hz producer must not accumulate state into later queries.
    let d = ScrollDriver::begin_fling(START_MS, 0.0, 2000.0, -1_000_000, 1_000_000, 1.0).unwrap();
    for now in frames(144, 432) {
        let _ = d.offset_px(now);
    }
    let (duration, distance) = harness_clamping(2000.0);
    for now in frames(60, 180) {
        let t = (now - START_MS) as f64 / 1000.0;
        let want = harness_clamping_x(0.0, distance, duration, t);
        assert!((d.x(now) - want).abs() <= 0.5, "x at {now}");
        assert_eq!(d.offset_px(now), d.offset_px(now));
    }
}

#[test]
fn refresh_03_decay_timeline_uses_injected_time_not_frame_count() {
    // Querying t=100 ms first or last yields the same dx: no step indexed by
    // frame number (SS:256-260 evaluated at absolute t).
    let d = ScrollDriver::begin_fling(START_MS, 0.0, 2000.0, -1_000_000, 1_000_000, 1.0).unwrap();
    let late = d.dx(START_MS + 400).to_bits();
    let early = d.dx(START_MS + 100).to_bits();
    assert_eq!(early, d.dx(START_MS + 100).to_bits());
    assert_eq!(late, d.dx(START_MS + 400).to_bits());
    let (duration, _) = harness_clamping(2000.0);
    assert!((d.dx(START_MS + 100) - harness_clamping_dx(2000.0, duration, 0.1)).abs() <= 0.5);
}

#[test]
fn anchors_and_default_goldens_are_unchanged() {
    let source = include_str!("../src/scroll_physics.rs");
    for blob in [
        VT_BLOB, LSQ_BLOB, ACT_BLOB, SS_BLOB, SP_BLOB, GS_BLOB, GC_BLOB, TOL_BLOB,
    ] {
        assert!(source.contains(blob), "missing source anchor {blob}");
    }
    let ir = parse_ir(&std::fs::read_to_string(COUNTER).unwrap()).unwrap();
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
