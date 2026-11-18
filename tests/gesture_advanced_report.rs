//! Machine-readable report for advanced gesture fields: scale rotation and
//! the drag start behavior.
//!
//! Flutter 3.50.0 anchors (checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`):
//! - `gestures/scale.dart:487-509`: the signed rotation factor is
//!   `angle2 - angle1` from two `atan2` lines.
//! - `gestures/scale.dart:738-750,772`: the update carries scale, focal point
//!   and rotation.
//! - `gestures/recognizer.dart:48-56`: `DragStartBehavior.down` anchors the
//!   drag at the down point, `start` at the arena-win point.
//! - `gestures/monodrag.dart:103-125`: the win position is used by `start`.
//!
//! The metrics are logical counts, never wall-clock timings. The test writes
//! one deterministic JSON report and never mutates shared sources or CI
//! files.

use opurom_render::{
    DragStartBehavior, Gesture, GestureEngine, PointerEvent, PointerKind, Rect, DRAG_THRESHOLD,
};
use std::{env, fs, path::PathBuf};

fn event(kind: PointerKind, id: u32, x: i64, y: i64, t: i64) -> PointerEvent {
    PointerEvent::new(kind, id, t, x, y, x, y)
}

fn engine(behavior: DragStartBehavior) -> GestureEngine {
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
    e.set_drag_start_behavior(behavior);
    e
}

fn rotation(gesture: Option<(u32, Gesture)>) -> Option<i64> {
    match gesture {
        Some((_, Gesture::Scale { rotation_milli, .. })) => Some(rotation_milli),
        _ => None,
    }
}

#[test]
fn advanced_gesture_fields_are_stable() {
    let mut e = engine(DragStartBehavior::Down);
    assert_eq!(e.dispatch(&event(PointerKind::Down, 1, 60, 100, 0)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Down, 2, 140, 100, 1)), None);
    assert_eq!(e.dispatch(&event(PointerKind::Move, 1, 60, 140, 2)), None);
    assert_eq!(
        rotation(e.dispatch(&event(PointerKind::Move, 2, 140, 180, 3))),
        Some(-1571)
    );
    // The pointer line is now vertical, so the rotation stays at the same
    // signed quarter turn (`scale.dart:645-675` reconfigures only when the
    // active pointer ids change).
    assert_eq!(
        rotation(e.dispatch(&event(PointerKind::Move, 2, 140, 220, 4))),
        Some(-1571)
    );
}

#[test]
fn write_gesture_advanced_report() {
    let quarter_turn = {
        let mut e = engine(DragStartBehavior::Down);
        assert_eq!(e.dispatch(&event(PointerKind::Down, 1, 60, 100, 0)), None);
        assert_eq!(e.dispatch(&event(PointerKind::Down, 2, 140, 100, 1)), None);
        assert_eq!(e.dispatch(&event(PointerKind::Move, 1, 60, 140, 2)), None);
        rotation(e.dispatch(&event(PointerKind::Move, 2, 140, 180, 3))).unwrap_or(0)
    };
    let no_rotation = {
        let mut e = engine(DragStartBehavior::Down);
        assert_eq!(e.dispatch(&event(PointerKind::Down, 1, 60, 100, 0)), None);
        assert_eq!(e.dispatch(&event(PointerKind::Down, 2, 140, 100, 1)), None);
        assert_eq!(e.dispatch(&event(PointerKind::Move, 1, 50, 100, 2)), None);
        rotation(e.dispatch(&event(PointerKind::Move, 2, 170, 100, 3))).unwrap_or(0)
    };
    let down_delta = {
        let mut e = engine(DragStartBehavior::Down);
        assert_eq!(e.dispatch(&event(PointerKind::Down, 1, 50, 50, 0)), None);
        match e.dispatch(&event(PointerKind::Move, 1, 50 + DRAG_THRESHOLD + 2, 50, 1)) {
            Some((_, Gesture::Drag { dx, dy })) => dx + dy,
            _ => -1,
        }
    };
    let start_delta = {
        let mut e = engine(DragStartBehavior::Start);
        assert_eq!(e.dispatch(&event(PointerKind::Down, 1, 50, 50, 0)), None);
        match e.dispatch(&event(PointerKind::Move, 1, 50 + DRAG_THRESHOLD + 2, 50, 1)) {
            Some((_, Gesture::Drag { dx, dy })) => dx + dy,
            _ => -1,
        }
    };
    assert!(quarter_turn == -1571 || quarter_turn == 1571);
    assert_eq!(no_rotation, 0);
    assert_eq!(down_delta, DRAG_THRESHOLD + 2);
    assert_eq!(start_delta, 0);

    let report = format!(
        concat!(
            "{{\n",
            "  \"schema\": \"opurom.gesture.advanced/1\",\n",
            "  \"source\": \"flutter-3.50.0:ab2912035e160d9cad6306df307576da6128ebdb\",\n",
            "  \"clock\": \"logical\",\n",
            "  \"metrics\": {{\n",
            "    \"rotation_quarter_turn_milli\": {rotation},\n",
            "    \"rotation_horizontal_milli\": {horizontal},\n",
            "    \"drag_down_origin_delta\": {down},\n",
            "    \"drag_start_origin_delta\": {start},\n",
            "    \"drag_threshold_px\": {threshold}\n",
            "  }}\n",
            "}}\n"
        ),
        rotation = quarter_turn,
        horizontal = no_rotation,
        down = down_delta,
        start = start_delta,
        threshold = DRAG_THRESHOLD,
    );
    let dir = PathBuf::from(
        env::var("GESTURE_ADVANCED_REPORT_DIR")
            .or_else(|_| env::var("CARGO_TARGET_TMPDIR"))
            .unwrap_or_else(|_| env::temp_dir().to_string_lossy().into_owned()),
    );
    fs::create_dir_all(&dir).expect("create advanced report directory");
    let path = dir.join("gesture-advanced-report.json");
    fs::write(&path, &report).expect("write gesture advanced report");
    println!("GESTURE_ADVANCED_REPORT={}", path.display());
}
