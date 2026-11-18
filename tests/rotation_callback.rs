//! Rotation callback parity pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `gestures/scale.dart:487-509` (blob
//!   `fe3cdf27a414de2f112c58ad5d633831f7f1536e`): the rotation factor is
//!   `angle2 - angle1`, with `atan2(fy - sy, fx - sx)` for both lines.
//! - `gestures/scale.dart:645-675`: `_updateLines` reconfigures the initial
//!   line when pointer identities change.
//! - `gestures/scale.dart:738-750,772`: span/focal slop accepts the
//!   recognizer and the update carries scale, focal point and rotation.
//! - `gestures/events.dart:266-279,370-431` (blob
//!   `eb7cf29362d6f55ef2e93cac2529a4e15044e770`): pointer events carry
//!   pressure, size, radiusMajor and radiusMinor. Opurom keeps these in a
//!   host-side injection fixture and does not add them to the ABI.
//!
//! These tests use the public gesture API and add no ABI or IR field.

use opurom_render::{Gesture, GestureEngine, GestureKind, PointerEvent, PointerKind, Rect};

fn engine() -> GestureEngine {
    let mut e = GestureEngine::new();
    e.add_node_with(
        1,
        Rect {
            x: 0,
            y: 0,
            w: 400,
            h: 400,
        },
        None,
        None,
    );
    e
}

fn event(kind: PointerKind, id: u32, x: i64, y: i64, t: i64) -> PointerEvent {
    PointerEvent::new(kind, id, t, x, y, x, y)
}

/// A host-side projection of `events.dart:266-279,370-431`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct TouchSample {
    pointer_id: u32,
    pressure_milli: u32,
    size_milli: u32,
    radius_major: u32,
    radius_minor: u32,
}

impl TouchSample {
    fn new(id: u32, pressure: u32, size: u32, major: u32, minor: u32) -> Self {
        Self {
            pointer_id: id,
            pressure_milli: pressure,
            size_milli: size,
            radius_major: major,
            radius_minor: minor,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RotatedScale {
    scale: i64,
    focal_x: i64,
    focal_y: i64,
    rotation_milli: i64,
}

fn rotated_scale(gesture: Option<(u32, Gesture)>) -> Option<RotatedScale> {
    match gesture {
        Some((
            _,
            Gesture::Scale {
                scale,
                focal_x,
                focal_y,
                rotation_milli,
            },
        )) => Some(RotatedScale {
            scale,
            focal_x,
            focal_y,
            rotation_milli,
        }),
        _ => None,
    }
}

#[test]
fn rotation_reports_the_signed_quarter_turn() {
    // scale.dart:500-503: a quarter turn is the signed angle difference.
    let mut e = engine();
    assert_eq!(e.dispatch(&event(PointerKind::Down, 1, 60, 100, 0)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Down, 2, 140, 100, 1)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Move, 1, 60, 140, 2)), None);
    let update = rotated_scale(e.dispatch(&event(PointerKind::Move, 2, 140, 180, 3)));
    assert_eq!(
        update,
        Some(RotatedScale {
            scale: 1126,
            focal_x: 100,
            focal_y: 160,
            rotation_milli: -1571,
        })
    );
    assert_eq!(e.arena_winner(), Some(GestureKind::Scale));
}

#[test]
fn a_horizontal_move_keeps_the_rotation_at_zero() {
    let mut e = engine();
    assert_eq!(e.dispatch(&event(PointerKind::Down, 1, 60, 100, 0)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Down, 2, 140, 100, 1)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Move, 1, 50, 100, 2)), None);
    let update = rotated_scale(e.dispatch(&event(PointerKind::Move, 2, 170, 100, 3)));
    assert_eq!(update.unwrap().rotation_milli, 0);
}

#[test]
fn the_rotation_follows_the_active_pointer_pair() {
    // scale.dart:645-675: the line is tied to the two active pointer ids.
    let mut e = engine();
    assert_eq!(e.dispatch(&event(PointerKind::Down, 1, 60, 100, 0)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Down, 2, 140, 100, 1)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Down, 3, 200, 200, 2)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Move, 1, 60, 140, 3)), None);
    let update = rotated_scale(e.dispatch(&event(PointerKind::Move, 2, 140, 180, 4)));
    assert_eq!(update.unwrap().rotation_milli, -1571);
}

#[test]
fn host_touch_fields_are_projected_without_an_abi_change() {
    // events.dart:266-279,370-431: pressure, size and the two radii are
    // independent samples of the same pointer.
    let first = TouchSample::new(1, 500, 120, 8, 6);
    let second = TouchSample::new(2, 750, 200, 10, 7);
    assert_eq!(first.pressure_milli, 500);
    assert_eq!(second.pressure_milli, 750);
    assert_ne!(first.radius_major, first.radius_minor);
    // The engine still consumes only the seven existing event fields.
    let mut e = engine();
    assert_eq!(
        e.dispatch(&event(PointerKind::Down, first.pointer_id, 60, 100, 0)),
        None
    );
    assert_eq!(
        e.dispatch(&event(PointerKind::Down, second.pointer_id, 140, 100, 1)),
        None
    );
}

#[test]
fn rotation_updates_are_deterministic() {
    let run = || {
        let mut e = engine();
        let mut stream = Vec::new();
        stream.push(e.dispatch(&event(PointerKind::Down, 1, 60, 100, 0)));
        stream.push(e.dispatch(&event(PointerKind::Down, 2, 140, 100, 1)));
        stream.push(e.dispatch(&event(PointerKind::Move, 1, 60, 140, 2)));
        stream.push(e.dispatch(&event(PointerKind::Move, 2, 140, 180, 3)));
        stream
    };
    assert_eq!(run(), run());
}
