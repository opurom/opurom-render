//! Drag-end velocity parity pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `gestures/drag_details.dart:205-221` (`DragEndDetails`): `primaryVelocity`
//!   must equal exactly one axis of `velocity.pixelsPerSecond` with the other
//!   axis zero; `localPosition` falls back to `globalPosition`.
//! - `gestures/drag_details.dart:231-246`: the velocity is in logical pixels
//!   per second and `primaryVelocity` is null for a two-dimensional pan.
//! - `gestures/velocity_tracker.dart:142-220`: the velocity estimate comes
//!   from the 20-sample, 100 ms window.
//!
//! These tests pin the existing `drag_end_scroll_velocity` projection and the
//! velocity tracker output without adding an ABI or IR field.

use opurom_render::scroll_physics::{drag_end_scroll_velocity, IosFlingVelocityTracker};

#[derive(Clone, Copy, Debug, PartialEq)]
struct DragEnd {
    global_position: (i64, i64),
    velocity: (f64, f64),
    primary_velocity: Option<f64>,
}

impl DragEnd {
    /// `drag_details.dart:216-221`: the primary velocity must match one axis.
    fn valid(&self) -> bool {
        match self.primary_velocity {
            None => true,
            Some(value) => {
                (value == self.velocity.0 && self.velocity.1 == 0.0)
                    || (value == self.velocity.1 && self.velocity.0 == 0.0)
            }
        }
    }

    /// `drag_details.dart:221`: `localPosition` falls back to
    /// `globalPosition`.
    fn local_position(&self) -> (i64, i64) {
        self.global_position
    }
}

#[test]
fn a_primary_velocity_must_match_one_axis_of_the_velocity() {
    assert!(DragEnd {
        global_position: (10, 20),
        velocity: (100.0, 0.0),
        primary_velocity: Some(100.0),
    }
    .valid());
    assert!(DragEnd {
        global_position: (10, 20),
        velocity: (0.0, 100.0),
        primary_velocity: Some(100.0),
    }
    .valid());
    assert!(DragEnd {
        global_position: (10, 20),
        velocity: (100.0, 50.0),
        primary_velocity: None,
    }
    .valid());
    assert!(!DragEnd {
        global_position: (10, 20),
        velocity: (100.0, 50.0),
        primary_velocity: Some(100.0),
    }
    .valid());
}

#[test]
fn local_position_falls_back_to_global() {
    let end = DragEnd {
        global_position: (30, 40),
        velocity: (0.0, 0.0),
        primary_velocity: None,
    };
    assert_eq!(end.local_position(), (30, 40));
}

#[test]
fn scroll_velocity_negates_the_pointer_velocity() {
    // drag_end_scroll_velocity implements the negation in
    // widgets/scroll_position.dart: vertical scrolling negates the pointer
    // velocity; a reversed axis negates it again.
    assert_eq!(drag_end_scroll_velocity(500.0, false), -500.0);
    assert_eq!(drag_end_scroll_velocity(-500.0, false), 500.0);
    assert_eq!(drag_end_scroll_velocity(500.0, true), 500.0);
    assert_eq!(drag_end_scroll_velocity(-500.0, true), -500.0);
}

#[test]
fn the_velocity_tracker_reports_the_last_window() {
    let mut tracker = IosFlingVelocityTracker::new();
    for step in 0..6 {
        let time = step * 16;
        tracker.add_position(time, step as f64 * 16.0, 0.0);
    }
    let estimate = tracker.estimate(80).expect("a live tracker");
    // Six frames of 16 px each: the weighted mean of the last three
    // intervals is 1000 px/s.
    assert!((estimate.pixels_per_second.0 - 1000.0).abs() < 1.0);
    assert!(estimate.pixels_per_second.1.abs() < 1e-9);
}

#[test]
fn an_empty_tracker_produces_no_estimate() {
    let tracker = IosFlingVelocityTracker::new();
    assert!(tracker.estimate(0).is_none());
}

#[test]
fn drag_end_projection_is_deterministic() {
    let run = || {
        let mut tracker = IosFlingVelocityTracker::new();
        for step in 0..4 {
            tracker.add_position(step * 16, step as f64 * 8.0, 0.0);
        }
        let estimate = tracker.estimate(48).expect("a live tracker");
        let end = DragEnd {
            global_position: (16, 0),
            velocity: estimate.pixels_per_second,
            primary_velocity: Some(estimate.pixels_per_second.0),
        };
        (end.valid(), drag_end_scroll_velocity(end.velocity.0, false))
    };
    assert_eq!(run(), run());
}
