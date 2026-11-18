//! Machine-readable report for the iOS bounce scroll physics.
//!
//! Flutter 3.50.0 anchors (checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`):
//! - `widgets/scroll_simulation.dart:18-132`: the bouncing simulation, the
//!   iOS friction coefficient, the friction to spring handover and the
//!   `maxSpringTransferVelocity` cap.
//! - `physics/friction_simulation.dart:35-165`: the friction closed form,
//!   `finalX` and `timeAtX`.
//! - `widgets/scroll_physics.dart:748-946`: the rubber band spring and its
//!   70 ms half life.
//!
//! The metrics are logical counts and scaled constants, never wall-clock
//! timings. The test writes one deterministic JSON report and never mutates
//! shared sources or CI files.

use opurom_render::scroll_physics::{
    BouncingScrollSimulation, ScrollTolerance, SpringDescription, SpringSolution,
    BOUNCE_FRICTION, MAX_SPRING_TRANSFER_VELOCITY, RUBBER_BAND_HALF_LIFE_SECONDS,
    RUBBER_BAND_LAMBDA,
};
use std::{env, fs, path::PathBuf};

const DENSITY: f64 = 1.0;
const LEADING: f64 = 0.0;
const TRAILING: f64 = 1000.0;

fn tolerance() -> ScrollTolerance {
    ScrollTolerance::for_dpr(DENSITY)
}

/// The rubber band displacement ratio after one half life, in parts per
/// million (`widgets/scroll_physics.dart:761-765`).
fn rubber_band_decay_ppm() -> u64 {
    let spring = SpringDescription::ios_rubber_band();
    let solution = SpringSolution::new(spring, 200.0, 0.0);
    let start = solution.x(0.0);
    let half_life = solution.x(RUBBER_BAND_HALF_LIFE_SECONDS);
    ((half_life / start) * 1e6).round() as u64
}

/// The peak overshoot, in logical pixels, that the capped spring transfer can
/// produce (`widgets/scroll_simulation.dart:59-65`, `:79-81`).
fn bounce_overshoot_px() -> u64 {
    let simulation = BouncingScrollSimulation::new(
        0.0,
        20_000.0,
        LEADING,
        TRAILING,
        SpringDescription::ios_default(),
        0.0,
        tolerance(),
    )
    .expect("bouncing simulation");
    let spring_time = simulation.spring_time();
    let mut peak = simulation.x(spring_time) - TRAILING;
    let mut step = 1;
    while step <= 4000 {
        let value = simulation.x(spring_time + step as f64 * 0.001) - TRAILING;
        if value > peak {
            peak = value;
        }
        step += 1;
    }
    peak.round() as u64
}

/// The number of 1/60 s frames the rubber band bounce needs to settle.
fn bounce_settle_steps() -> u64 {
    let simulation = BouncingScrollSimulation::new(
        1200.0,
        0.0,
        LEADING,
        TRAILING,
        SpringDescription::ios_rubber_band(),
        0.0,
        tolerance(),
    )
    .expect("bouncing simulation");
    let mut steps = 0;
    while steps < 600 {
        steps += 1;
        if simulation.is_done(steps as f64 / 60.0) {
            return steps;
        }
    }
    steps
}

/// The number of milliseconds the friction phase needs to reach the trailing
/// extent before the spring takes over.
fn bounce_handover_steps() -> u64 {
    let simulation = BouncingScrollSimulation::new(
        0.0,
        20_000.0,
        LEADING,
        TRAILING,
        SpringDescription::ios_default(),
        0.0,
        tolerance(),
    )
    .expect("bouncing simulation");
    let spring_time = simulation.spring_time();
    (spring_time * 1000.0).round() as u64
}

#[test]
fn write_gesture_bounce_report() {
    let friction_ppm = (BOUNCE_FRICTION * 1e6).round() as u64;
    let transfer_cap = MAX_SPRING_TRANSFER_VELOCITY.round() as u64;
    let half_life_ms = (RUBBER_BAND_HALF_LIFE_SECONDS * 1000.0).round() as u64;
    let lambda_micro = (RUBBER_BAND_LAMBDA * 1e6).round() as u64;
    let decay_ppm = rubber_band_decay_ppm();
    let overshoot = bounce_overshoot_px();
    let settle = bounce_settle_steps();
    let handover = bounce_handover_steps();

    assert_eq!(friction_ppm, 135_000);
    assert_eq!(transfer_cap, 5_000);
    assert_eq!(half_life_ms, 70);
    assert!(decay_ppm >= 499_900 && decay_ppm <= 500_100);

    let report = format!(
        concat!(
            "{{\n",
            "  \"schema\": \"opurom.gesture.bounce/1\",\n",
            "  \"source\": \"flutter-3.50.0:ab2912035e160d9cad6306df307576da6128ebdb\",\n",
            "  \"clock\": \"logical\",\n",
            "  \"metrics\": {{\n",
            "    \"bounce_friction_ppm\": {friction},\n",
            "    \"bounce_transfer_cap\": {transfer},\n",
            "    \"rubber_band_half_life_ms\": {half_life},\n",
            "    \"rubber_band_lambda_micro\": {lambda},\n",
            "    \"rubber_band_decay_ppm\": {decay},\n",
            "    \"bounce_overshoot_px\": {overshoot},\n",
            "    \"bounce_settle_steps\": {settle},\n",
            "    \"bounce_handover_steps\": {handover}\n",
            "  }}\n",
            "}}\n"
        ),
        friction = friction_ppm,
        transfer = transfer_cap,
        half_life = half_life_ms,
        lambda = lambda_micro,
        decay = decay_ppm,
        overshoot = overshoot,
        settle = settle,
        handover = handover,
    );
    let dir = PathBuf::from(
        env::var("GESTURE_BOUNCE_REPORT_DIR")
            .or_else(|_| env::var("CARGO_TARGET_TMPDIR"))
            .unwrap_or_else(|_| env::temp_dir().to_string_lossy().into_owned()),
    );
    fs::create_dir_all(&dir).expect("create bounce report directory");
    let path = dir.join("gesture-bounce-report.json");
    fs::write(&path, &report).expect("write gesture bounce report");
    println!("GESTURE_BOUNCE_REPORT={}", path.display());
}
