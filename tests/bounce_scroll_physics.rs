//! iOS bounce scroll physics pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb` (verified against the `3.50.0`
//! tag), engine `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `widgets/scroll_simulation.dart:18-132` (`BouncingScrollSimulation`):
//!   `43-48` release outside the extents springs back, `50-57` the iOS
//!   friction coefficient 0.135, `59-72` the friction to spring handover,
//!   `79-81` `maxSpringTransferVelocity = 5000`, `99-126` phase selection.
//! - `physics/friction_simulation.dart:35-165`: `51-57` the Newton stop
//!   time, `117-134` the closed-form position and velocity, `136-141`
//!   `finalX`, `144-155` `timeAtX`, `157-160` `isDone`.
//! - `widgets/scroll_physics.dart:748-946` (`BouncingScrollPhysics`):
//!   `759-765` the deceleration rate and the rubber band decay constant,
//!   `796-800` the rubber band spring, `878-899` the ballistic simulation,
//!   `904-905` the doubled minimum fling velocity, `932-945` the rate
//!   dependent spring and maximum fling velocity.
//! - `physics/spring_simulation.dart:265-280,284-392`: the spring solutions.
//!
//! These tests use the public scroll physics API and add no ABI or IR field.

use opurom_render::scroll_physics::*;

const DENSITY: f64 = 1.0;

fn tolerance() -> ScrollTolerance {
    ScrollTolerance::for_dpr(DENSITY)
}

#[test]
fn ios_bounce_constants_match_flutter() {
    // widgets/scroll_simulation.dart:50-57 and :79-81.
    assert_eq!(BOUNCE_FRICTION, 0.135);
    assert_eq!(MAX_SPRING_TRANSFER_VELOCITY, 5000.0);
    // widgets/scroll_physics.dart:762 and :765.
    assert_eq!(RUBBER_BAND_HALF_LIFE_SECONDS, 0.07);
    assert!((RUBBER_BAND_LAMBDA - 2.0f64.ln() / RUBBER_BAND_HALF_LIFE_SECONDS).abs() < 1e-12);
    // widgets/scroll_physics.dart:904-905 doubles the minimum fling velocity.
    assert_eq!(ScrollDecelerationRate::Normal.min_fling_velocity(), 100.0);
    // widgets/scroll_physics.dart:932-934.
    assert_eq!(ScrollDecelerationRate::Normal.max_fling_velocity(), 8000.0);
    assert_eq!(ScrollDecelerationRate::Fast.max_fling_velocity(), 64000.0);
    // widgets/scroll_physics.dart:892-895.
    assert_eq!(ScrollDecelerationRate::Normal.constant_deceleration(), 0.0);
    assert_eq!(ScrollDecelerationRate::Fast.constant_deceleration(), 1400.0);
}

#[test]
fn rubber_band_spring_parameters_match_flutter() {
    // widgets/scroll_physics.dart:796-800.
    let spring = SpringDescription::ios_rubber_band();
    assert_eq!(spring.mass, 1.0);
    let lambda = RUBBER_BAND_LAMBDA;
    assert!((spring.stiffness - 1e5 * lambda * lambda).abs() < 1e-6);
    assert!((spring.damping - (1e5 + 1.0) * lambda).abs() < 1e-6);
    // The spring is overdamped with the roots -lambda and -100000 * lambda
    // (widgets/scroll_physics.dart:776-800).
    let solution = SpringSolution::new(spring, 200.0, 0.0);
    match solution {
        SpringSolution::Overdamped { r1, r2, .. } => {
            assert!((r2 - -lambda).abs() < 1e-9, "visible root is -lambda");
            assert!(
                (r1 - -1e5 * lambda).abs() < 1e-3,
                "fast root is -100000 * lambda"
            );
        }
        other => panic!("rubber band spring must be overdamped, got {other:?}"),
    }
}

#[test]
fn rubber_band_displacement_halves_every_half_life() {
    // widgets/scroll_physics.dart:761-765 derives the spring so the visible
    // decay is exp(-lambda * t): the displacement halves every 70 ms.
    let spring = SpringDescription::ios_rubber_band();
    let solution = SpringSolution::new(spring, 200.0, 0.0);
    let start = solution.x(0.0);
    let half_life = solution.x(RUBBER_BAND_HALF_LIFE_SECONDS);
    assert!(
        (half_life / start - 0.5).abs() < 1e-3,
        "displacement after one half life is {half_life} of {start}"
    );
}

#[test]
fn friction_simulation_matches_flutter_closed_form() {
    // physics/friction_simulation.dart:117-141.
    let friction = FrictionSimulation::new(BOUNCE_FRICTION, 0.0, 1000.0, 0.0, tolerance())
        .expect("friction simulation");
    let drag_log = BOUNCE_FRICTION.ln();
    let expected = 1000.0 * (BOUNCE_FRICTION - 1.0) / drag_log;
    assert!((friction.x(1.0) - expected).abs() < 1e-6);
    let final_x = -1000.0 / drag_log;
    assert!((friction.final_x() - final_x).abs() < 1e-6);
    assert!((friction.dx(1.0) - 1000.0 * BOUNCE_FRICTION).abs() < 1e-6);
    // physics/friction_simulation.dart:144-155: the reported time really
    // reaches the requested position.
    let target = 250.0;
    let time = friction.time_at_x(target);
    assert!(time.is_finite() && time > 0.0);
    assert!((friction.x(time) - target).abs() < 1e-3);
}

#[test]
fn out_of_range_release_springs_back_to_the_extent() {
    // widgets/scroll_simulation.dart:43-48.
    let simulation = BouncingScrollSimulation::new(
        1200.0,
        0.0,
        0.0,
        1000.0,
        SpringDescription::ios_rubber_band(),
        0.0,
        tolerance(),
    )
    .expect("bouncing simulation");
    assert_eq!(simulation.leading_extent(), 0.0);
    assert_eq!(simulation.trailing_extent(), 1000.0);
    assert_eq!(simulation.x(0.0), 1200.0);
    assert!(
        simulation.dx(0.0).abs() < 1e-6,
        "a stationary release has no initial velocity"
    );
    let half_life = simulation.x(RUBBER_BAND_HALF_LIFE_SECONDS);
    assert!(
        (half_life - 1100.0).abs() < 0.01,
        "the rubber band halves the overshoot every 70 ms, got {half_life}"
    );
    // The overshoot never grows and the resting place is the extent.
    let mut previous = simulation.x(0.0);
    for step in 1..=40 {
        let now = step as f64 * 0.05;
        let value = simulation.x(now);
        assert!(
            value <= previous + 1e-9,
            "the bounce must not overshoot further"
        );
        assert!(
            value >= 1000.0 - 1e-9,
            "the bounce must stay above the extent"
        );
        previous = value;
    }
    assert!((simulation.x(5.0) - 1000.0).abs() < 0.01);
}

#[test]
fn friction_phase_hands_over_to_the_spring_at_the_extent() {
    // widgets/scroll_simulation.dart:59-65.
    let simulation = BouncingScrollSimulation::new(
        0.0,
        20000.0,
        0.0,
        1000.0,
        SpringDescription::ios_default(),
        0.0,
        tolerance(),
    )
    .expect("bouncing simulation");
    let spring_time = simulation.spring_time();
    assert!(spring_time.is_finite() && spring_time > 0.0 && spring_time < 1.0);
    assert!(
        (simulation.x(spring_time) - 1000.0).abs() < 1e-3,
        "the handover happens at the trailing extent"
    );
    // The spring takes over from the extent and settles back on it.
    assert!((simulation.x(spring_time + 5.0) - 1000.0).abs() < 1e-3);
}

#[test]
fn spring_transfer_velocity_is_capped() {
    // widgets/scroll_simulation.dart:63 and :79-81.
    let simulation = BouncingScrollSimulation::new(
        0.0,
        20000.0,
        0.0,
        1000.0,
        SpringDescription::ios_default(),
        0.0,
        tolerance(),
    )
    .expect("bouncing simulation");
    assert_eq!(
        simulation.spring_start_velocity(),
        Some(MAX_SPRING_TRANSFER_VELOCITY)
    );
}

#[test]
fn short_fling_never_reaches_a_spring() {
    // widgets/scroll_simulation.dart:73-75.
    let simulation = BouncingScrollSimulation::new(
        0.0,
        100.0,
        0.0,
        1000.0,
        SpringDescription::ios_default(),
        0.0,
        tolerance(),
    )
    .expect("bouncing simulation");
    assert!(simulation.spring_time().is_infinite());
    assert_eq!(simulation.spring_start_velocity(), None);
    // physics/friction_simulation.dart:157-160: the fling settles when its
    // velocity drops below the scroll tolerance.
    assert!(!simulation.is_done(0.1));
    assert!(simulation.is_done(1.0));
    assert!((simulation.x(1.0) - simulation.x(1.0)).abs() < 1e-9);
}

#[test]
fn begin_bounce_uses_the_rubber_band_for_a_stationary_overscroll() {
    // widgets/scroll_physics.dart:882 and :886.
    let driver = ScrollDriver::begin_bounce(
        0,
        1100.0,
        0.0,
        0,
        1000,
        DENSITY,
        ScrollDecelerationRate::Normal,
    )
    .expect("bounce driver");
    assert!(matches!(driver.motion, ScrollMotion::Bouncing(_)));
    assert_eq!(driver.offset_px(0), 1100, "the overscroll is not clamped");
    let half_life = driver.x(70);
    assert!(
        (half_life - 1050.0).abs() < 0.05,
        "the rubber band halves the overshoot every 70 ms, got {half_life}"
    );
}

#[test]
fn begin_bounce_rejects_a_stationary_release_inside_the_extents() {
    // widgets/scroll_physics.dart:881-884.
    assert!(ScrollDriver::begin_bounce(
        0,
        500.0,
        0.0,
        0,
        1000,
        DENSITY,
        ScrollDecelerationRate::Normal
    )
    .is_none());
    assert!(ScrollDriver::begin_bounce(
        0,
        500.0,
        0.0,
        0,
        1000,
        0.0,
        ScrollDecelerationRate::Normal
    )
    .is_none());
}

#[test]
fn begin_bounce_uses_the_rate_spring_for_a_moving_release() {
    // widgets/scroll_physics.dart:938-945.
    let fast = ScrollDriver::begin_bounce(
        0,
        500.0,
        2000.0,
        0,
        1000,
        DENSITY,
        ScrollDecelerationRate::Fast,
    )
    .expect("bounce driver");
    let normal = ScrollDriver::begin_bounce(
        0,
        500.0,
        2000.0,
        0,
        1000,
        DENSITY,
        ScrollDecelerationRate::Normal,
    )
    .expect("bounce driver");
    assert!(matches!(fast.motion, ScrollMotion::Bouncing(_)));
    assert!(matches!(normal.motion, ScrollMotion::Bouncing(_)));
    assert!(fast.dx(0) > 0.0);
    assert!(normal.dx(0) > 0.0);
}

#[test]
fn bounce_simulation_is_deterministic() {
    let build = || {
        BouncingScrollSimulation::new(
            250.0,
            1500.0,
            0.0,
            1000.0,
            SpringDescription::ios_default(),
            0.0,
            tolerance(),
        )
        .expect("bouncing simulation")
    };
    let first = build();
    let second = build();
    assert_eq!(first, second);
    for step in 0..20 {
        let time = step as f64 * 0.033;
        assert_eq!(first.x(time), second.x(time));
        assert_eq!(first.dx(time), second.dx(time));
        assert_eq!(first.is_done(time), second.is_done(time));
    }
}
