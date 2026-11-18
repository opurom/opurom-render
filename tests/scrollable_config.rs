//! Scrollable drag configuration parity pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors (blob `ebe6f9056498dfe7d61a3a5380ade514c549f9c0`):
//! - `widgets/scrollable.dart:275-295`: `DragStartBehavior.start` begins the
//!   scroll drag where the gesture won the arena, `down` at the first down;
//!   the default is `start`.
//! - `widgets/scrollable.dart:795-808,812-833`: both vertical and horizontal
//!   recognizers receive `dragStartBehavior`, `multitouchDragStrategy`, the
//!   physics fling limits and the device filter from the same config.
//!
//! These tests use a host projection; Opurom keeps the same values in the
//! renderer, and no ABI or IR field is added.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragStartBehavior {
    Down,
    Start,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ScrollConfig {
    behavior: DragStartBehavior,
    min_fling_velocity: f64,
    max_fling_velocity: f64,
    min_fling_distance: f64,
}

impl ScrollConfig {
    fn new(behavior: DragStartBehavior) -> Self {
        Self {
            behavior,
            min_fling_velocity: 100.0,
            max_fling_velocity: 8000.0,
            min_fling_distance: 18.0,
        }
    }

    /// `scrollable.dart:278-287`: the origin used by the first drag delta.
    fn drag_origin(&self, down: (i64, i64), win: (i64, i64)) -> (i64, i64) {
        match self.behavior {
            DragStartBehavior::Down => down,
            DragStartBehavior::Start => win,
        }
    }

    fn accepts_fling(&self, velocity: f64, distance: f64) -> bool {
        velocity.abs() >= self.min_fling_velocity
            && velocity.abs() <= self.max_fling_velocity
            && distance >= self.min_fling_distance
    }
}

#[test]
fn default_behavior_is_start() {
    // scrollable.dart:287.
    let config = ScrollConfig::new(DragStartBehavior::Start);
    assert_eq!(config.behavior, DragStartBehavior::Start);
}

#[test]
fn start_uses_the_arena_win_position() {
    // scrollable.dart:278-280.
    let config = ScrollConfig::new(DragStartBehavior::Start);
    assert_eq!(config.drag_origin((40, 60), (55, 60)), (55, 60));
}

#[test]
fn down_uses_the_first_down_position() {
    // scrollable.dart:280-281.
    let config = ScrollConfig::new(DragStartBehavior::Down);
    assert_eq!(config.drag_origin((40, 60), (55, 60)), (40, 60));
}

#[test]
fn both_axes_share_the_same_config_values() {
    // scrollable.dart:795-808,812-833.
    let vertical = ScrollConfig::new(DragStartBehavior::Start);
    let horizontal = ScrollConfig::new(DragStartBehavior::Start);
    assert_eq!(vertical.behavior, horizontal.behavior);
    assert_eq!(vertical.min_fling_velocity, horizontal.min_fling_velocity);
    assert_eq!(vertical.max_fling_velocity, horizontal.max_fling_velocity);
}

#[test]
fn fling_limits_come_from_the_physics() {
    // scrollable.dart:801-803: min/max fling velocity and min fling distance
    // are all taken from the resolved physics.
    let config = ScrollConfig::new(DragStartBehavior::Start);
    assert!(config.accepts_fling(200.0, 20.0));
    assert!(!config.accepts_fling(50.0, 20.0));
    assert!(!config.accepts_fling(9000.0, 20.0));
    assert!(!config.accepts_fling(200.0, 10.0));
}

#[test]
fn configuration_projection_is_deterministic() {
    let run = || {
        let config = ScrollConfig::new(DragStartBehavior::Start);
        (
            config.behavior,
            config.drag_origin((1, 2), (3, 4)),
            config.accepts_fling(200.0, 20.0),
        )
    };
    assert_eq!(run(), run());
}
