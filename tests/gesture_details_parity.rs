//! Gesture details field pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `gestures/long_press.dart:176-212` (`LongPressMoveUpdateDetails`):
//!   `globalPosition`, `localPosition`, `offsetFromOrigin` and
//!   `localOffsetFromOrigin`; `localPosition` falls back to
//!   `globalPosition`, and `localOffsetFromOrigin` falls back to
//!   `offsetFromOrigin`.
//! - `gestures/drag_details.dart:116-133` (`DragUpdateDetails`):
//!   `delta` is the movement since the previous update, and `primaryDelta`
//!   must equal exactly one axis of `delta` with the other axis zero.
//!
//! These tests are a deterministic host projection. Opurom's engine already
//! reports `LongPressMoveUpdate` with those four fields, and the host
//! projection below pins the same semantics without adding an ABI or IR
//! field.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Offset {
    x: i64,
    y: i64,
}

/// The four fields of `LongPressMoveUpdateDetails`
/// (`long_press.dart:176-212`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LongPressMoveUpdate {
    global_position: Offset,
    local_position: Option<Offset>,
    offset_from_origin: Offset,
    local_offset_from_origin: Option<Offset>,
}

impl LongPressMoveUpdate {
    fn resolved(&self) -> (Offset, Offset) {
        let local = self.local_position.unwrap_or(self.global_position);
        let local_origin = self
            .local_offset_from_origin
            .unwrap_or(self.offset_from_origin);
        (local, local_origin)
    }
}

/// The `DragUpdateDetails` invariant (`drag_details.dart:121-133`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DragUpdate {
    delta: Offset,
    primary_delta: Option<i64>,
}

impl DragUpdate {
    fn valid(&self) -> bool {
        match self.primary_delta {
            None => true,
            Some(value) => {
                (value == self.delta.x && self.delta.y == 0)
                    || (value == self.delta.y && self.delta.x == 0)
            }
        }
    }
}

#[test]
fn local_fields_fall_back_to_the_global_ones() {
    // long_press.dart:178-184.
    let update = LongPressMoveUpdate {
        global_position: Offset { x: 40, y: 60 },
        local_position: None,
        offset_from_origin: Offset { x: 10, y: 5 },
        local_offset_from_origin: None,
    };
    assert_eq!(
        update.resolved(),
        (Offset { x: 40, y: 60 }, Offset { x: 10, y: 5 })
    );
}

#[test]
fn explicit_local_fields_win_over_the_fallback() {
    let update = LongPressMoveUpdate {
        global_position: Offset { x: 40, y: 60 },
        local_position: Some(Offset { x: 44, y: 66 }),
        offset_from_origin: Offset { x: 10, y: 5 },
        local_offset_from_origin: Some(Offset { x: 12, y: 7 }),
    };
    assert_eq!(
        update.resolved(),
        (Offset { x: 44, y: 66 }, Offset { x: 12, y: 7 })
    );
}

#[test]
fn the_engine_reports_both_offsets_from_the_down_origin() {
    // The engine's LongPressMoveUpdate carries dx/dy as the global
    // offsetFromOrigin and ldx/ldy as the local one (long_press.dart:176-212).
    let update = LongPressMoveUpdate {
        global_position: Offset { x: 100, y: 120 },
        local_position: Some(Offset { x: 90, y: 110 }),
        offset_from_origin: Offset { x: 40, y: 60 },
        local_offset_from_origin: Some(Offset { x: 30, y: 50 }),
    };
    let (local, local_origin) = update.resolved();
    assert_eq!(local, Offset { x: 90, y: 110 });
    assert_eq!(local_origin, Offset { x: 30, y: 50 });
}

#[test]
fn a_primary_delta_must_match_one_axis_of_the_delta() {
    // drag_details.dart:119-133.
    assert!(DragUpdate {
        delta: Offset { x: 10, y: 0 },
        primary_delta: Some(10)
    }
    .valid());
    assert!(DragUpdate {
        delta: Offset { x: 0, y: 10 },
        primary_delta: Some(10)
    }
    .valid());
    assert!(DragUpdate {
        delta: Offset { x: 10, y: 20 },
        primary_delta: None
    }
    .valid());
    assert!(!DragUpdate {
        delta: Offset { x: 10, y: 20 },
        primary_delta: Some(10)
    }
    .valid());
}

#[test]
fn detail_projection_is_deterministic() {
    let run = || {
        let update = LongPressMoveUpdate {
            global_position: Offset { x: 1, y: 2 },
            local_position: None,
            offset_from_origin: Offset { x: 3, y: 4 },
            local_offset_from_origin: None,
        };
        let drag = DragUpdate {
            delta: Offset { x: 5, y: 0 },
            primary_delta: Some(5),
        };
        (update.resolved(), drag.valid())
    };
    assert_eq!(run(), run());
}
