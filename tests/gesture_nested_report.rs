//! Machine-readable nested scroll coordination report for the CI gate.
//!
//! Flutter 3.50.0 anchors (checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`):
//! - `widgets/nested_scroll_view.dart:1068-1130`: the drag split between the
//!   inner and the outer scrollable.
//! - `widgets/nested_scroll_view.dart:1262-1340`: the clamped and the full
//!   drag update.
//! - `widgets/scroll_notification.dart:31-56`: the notification depth.
//! - `widgets/notification_listener.dart:92-138`: the notification bubbling.
//!
//! The metrics are logical counts, never wall-clock timings. The test writes
//! one deterministic JSON report and never mutates shared sources or CI
//! files.

use opurom_render::scroll_physics::{
    bubble_notifications, NestedScrollCoordinator, ScrollAxisPosition, ScrollNotificationKind,
    ScrollNotificationRecord,
};

fn coordinator() -> NestedScrollCoordinator {
    NestedScrollCoordinator::new(
        ScrollAxisPosition::new(200.0, 0.0, 500.0),
        vec![ScrollAxisPosition::new(0.0, 0.0, 100.0)],
    )
}

#[test]
fn nested_drag_split_is_stable() {
    // The same drag sequence must always split the same way.
    let run = || {
        let mut nested = coordinator();
        let mut steps = Vec::new();
        for delta in [40.0, -25.0, 10.0, -60.0, 5.0] {
            let outcome = nested.apply_user_offset(delta);
            steps.push((outcome.outer_applied, outcome.inner_applied));
        }
        steps
    };
    assert_eq!(run(), run());
}

#[test]
fn nested_notification_depth_is_stable() {
    let chain = [
        ScrollNotificationRecord::new(ScrollNotificationKind::Update, 0, 42.0),
        ScrollNotificationRecord::new(ScrollNotificationKind::Update, 0, 42.0),
        ScrollNotificationRecord::new(ScrollNotificationKind::Update, 0, 42.0),
    ];
    assert_eq!(bubble_notifications(&chain, None).len(), 3);
    assert_eq!(bubble_notifications(&chain, Some(1)).len(), 2);
}

#[test]
fn write_gesture_nested_report() {
    let mut nested = coordinator();
    let mut handoff_steps: u64 = 0;
    let mut outer_consumed = 0.0f64;
    let mut inner_consumed = 0.0f64;
    let mut unconsumed = 0.0f64;
    let mut overscroll = 0.0f64;
    // Phase one: the outer view takes the drag while the inner sits at its
    // leading edge (widgets/nested_scroll_view.dart:1097-1128).
    for delta in [40.0, -25.0, 10.0, -60.0, 5.0] {
        let outcome = nested.apply_user_offset(delta);
        if outcome.outer_applied != 0.0 && outcome.inner_applied == 0.0 {
            handoff_steps += 1;
        }
        outer_consumed += outcome.outer_applied.abs();
        inner_consumed += outcome.inner_applied.abs();
        unconsumed += outcome.unconsumed.abs();
        overscroll = overscroll.max(-nested.inner()[0].pixels);
    }
    // Phase two: the outer view is at its edge, so the inner position takes
    // the drag (widgets/nested_scroll_view.dart:1089-1095).
    nested.set_outer_pixels(500.0);
    let outcome = nested.apply_user_offset(-40.0);
    inner_consumed += outcome.inner_applied.abs();
    outer_consumed += outcome.outer_applied.abs();
    unconsumed += outcome.unconsumed.abs();
    // Phase three: both positions sit at their leading edge, so the inner
    // position enters an overscroll (widgets/nested_scroll_view.dart:1122-1126).
    nested.set_outer_pixels(0.0);
    nested.set_inner_pixels(0, 0.0);
    let outcome = nested.apply_user_offset(40.0);
    inner_consumed += outcome.inner_applied.abs();
    outer_consumed += outcome.outer_applied.abs();
    unconsumed += outcome.unconsumed.abs();
    overscroll = overscroll.max(-nested.inner()[0].pixels);
    let chain = [
        ScrollNotificationRecord::new(ScrollNotificationKind::Update, 0, 42.0),
        ScrollNotificationRecord::new(ScrollNotificationKind::Update, 0, 42.0),
        ScrollNotificationRecord::new(ScrollNotificationKind::Update, 0, 42.0),
    ];
    let notification_depth = bubble_notifications(&chain, None)
        .iter()
        .map(|record| record.depth)
        .max()
        .unwrap_or(0) as u64;

    let outer_milli = (outer_consumed * 1000.0).round() as u64;
    let inner_milli = (inner_consumed * 1000.0).round() as u64;
    let unconsumed_milli = (unconsumed * 1000.0).round() as u64;
    let overscroll_px = overscroll.round() as u64;

    assert!(handoff_steps >= 1);
    assert!(outer_milli >= 1);
    assert!(notification_depth >= 1);

    let report = format!(
        concat!(
            "{{\n",
            "  \"schema\": \"opurom.gesture.nested/1\",\n",
            "  \"source\": \"flutter-3.50.0:ab2912035e160d9cad6306df307576da6128ebdb\",\n",
            "  \"clock\": \"logical\",\n",
            "  \"metrics\": {{\n",
            "    \"nested_handoff_steps\": {handoff},\n",
            "    \"nested_outer_consumed_milli\": {outer},\n",
            "    \"nested_inner_consumed_milli\": {inner},\n",
            "    \"nested_unconsumed_milli\": {unconsumed},\n",
            "    \"nested_overscroll_px\": {overscroll},\n",
            "    \"nested_notification_depth\": {depth}\n",
            "  }}\n",
            "}}\n"
        ),
        handoff = handoff_steps,
        outer = outer_milli,
        inner = inner_milli,
        unconsumed = unconsumed_milli,
        overscroll = overscroll_px,
        depth = notification_depth,
    );
    let dir = std::path::PathBuf::from(
        std::env::var("GESTURE_NESTED_REPORT_DIR")
            .or_else(|_| std::env::var("CARGO_TARGET_TMPDIR"))
            .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().into_owned()),
    );
    std::fs::create_dir_all(&dir).expect("create nested report directory");
    let path = dir.join("gesture-nested-report.json");
    std::fs::write(&path, &report).expect("write gesture nested report");
    println!("GESTURE_NESTED_REPORT={}", path.display());
}
