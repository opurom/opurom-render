//! Nested scroll coordination pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb` (verified against the `3.50.0`
//! tag), engine `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `widgets/nested_scroll_view.dart:1068-1130` (`applyUserOffset`): a drag
//!   up removes the inner overscroll first and then moves the outer view, a
//!   drag down gives the inner positions what they can and hands the rest to
//!   the outer view.
//! - `widgets/nested_scroll_view.dart:1262-1313` (`applyClampedDragUpdate`):
//!   a clamped update never enters an overscroll and returns the delta that
//!   was not used.
//! - `widgets/nested_scroll_view.dart:1315-1340` (`applyFullDragUpdate`): a
//!   full update applies the whole delta and may enter an overscroll.
//! - `widgets/scroll_notification.dart:31-56,183-226`: a notification carries
//!   a depth, zero for the local notification.
//! - `widgets/notification_listener.dart:92-138`: a notification is delivered
//!   to the target and to every ancestor until a listener consumes it.
//!
//! These tests use the public scroll physics API and add no ABI or IR field.

use opurom_render::scroll_physics::{
    bubble_notifications, NestedScrollCoordinator, ScrollAxisPosition, ScrollNotificationKind,
    ScrollNotificationRecord, PRECISION_ERROR_TOLERANCE,
};

fn coordinator(
    outer_pixels: f64,
    outer_max: f64,
    inner_pixels: f64,
    inner_max: f64,
) -> NestedScrollCoordinator {
    NestedScrollCoordinator::new(
        ScrollAxisPosition::new(outer_pixels, 0.0, outer_max),
        vec![ScrollAxisPosition::new(inner_pixels, 0.0, inner_max)],
    )
}

#[test]
fn drag_down_hands_the_remainder_to_the_outer() {
    // widgets/nested_scroll_view.dart:1097-1128.
    let mut nested = coordinator(200.0, 500.0, 0.0, 100.0);
    let outcome = nested.apply_user_offset(40.0);
    // The inner is at its leading edge, so it consumes nothing and the outer
    // takes the whole drag.
    assert_eq!(nested.outer().pixels, 160.0);
    assert_eq!(nested.inner()[0].pixels, 0.0);
    assert_eq!(outcome.outer_applied, 40.0);
    assert_eq!(outcome.inner_applied, 0.0);
    assert!(outcome.unconsumed.abs() < 1e-9);
}

#[test]
fn drag_up_moves_the_outer_view_first() {
    // widgets/nested_scroll_view.dart:1074-1096: dragging up prioritizes the
    // outer view so the app bar scrolls out of the way as soon as possible.
    let mut nested = coordinator(200.0, 500.0, 50.0, 100.0);
    let outcome = nested.apply_user_offset(-40.0);
    assert_eq!(nested.outer().pixels, 240.0);
    assert_eq!(nested.inner()[0].pixels, 50.0);
    assert_eq!(outcome.outer_applied, -40.0);
    assert_eq!(outcome.inner_applied, 0.0);
}

#[test]
fn drag_up_reaches_the_inner_once_the_outer_is_at_its_edge() {
    // widgets/nested_scroll_view.dart:1089-1095.
    let mut nested = coordinator(500.0, 500.0, 50.0, 100.0);
    let outcome = nested.apply_user_offset(-40.0);
    assert_eq!(nested.outer().pixels, 500.0);
    assert_eq!(nested.inner()[0].pixels, 90.0);
    assert_eq!(outcome.outer_applied, 0.0);
    assert_eq!(outcome.inner_applied, -40.0);
}

#[test]
fn inner_overscroll_is_removed_before_the_outer_moves() {
    // widgets/nested_scroll_view.dart:1079-1088.
    let mut nested = coordinator(200.0, 500.0, -20.0, 100.0);
    let outcome = nested.apply_user_offset(-40.0);
    assert_eq!(
        nested.inner()[0].pixels,
        0.0,
        "the overscroll is consumed first"
    );
    assert_eq!(nested.outer().pixels, 220.0);
    assert_eq!(outcome.inner_applied, -20.0);
    assert_eq!(outcome.outer_applied, -20.0);
    assert!(outcome.unconsumed.abs() < 1e-9);
}

#[test]
fn clamped_drag_update_reports_the_unused_delta() {
    // widgets/nested_scroll_view.dart:1262-1313.
    let mut position = ScrollAxisPosition::new(50.0, 0.0, 100.0);
    // Dragging down inside the extent is fully consumed.
    assert_eq!(position.apply_clamped_drag_update(20.0), 0.0);
    assert_eq!(position.pixels, 30.0);
    // Dragging past the leading edge leaves the rest of the delta unused:
    // the position moves 30 and 30 units of the drag are left over.
    let unused = position.apply_clamped_drag_update(60.0);
    assert_eq!(unused, 30.0);
    assert_eq!(position.pixels, 0.0);
}

#[test]
fn full_drag_update_can_enter_an_overscroll() {
    // widgets/nested_scroll_view.dart:1315-1340.
    let mut position = ScrollAxisPosition::new(0.0, 0.0, 100.0);
    assert_eq!(position.apply_full_drag_update(40.0), 0.0);
    assert_eq!(position.pixels, -40.0, "the full update may overscroll");
}

#[test]
fn notifications_bubble_with_depth_and_stop_when_consumed() {
    // widgets/scroll_notification.dart:31-56 and
    // widgets/notification_listener.dart:92-138.
    let chain = [
        ScrollNotificationRecord::new(ScrollNotificationKind::Update, 0, 42.0),
        ScrollNotificationRecord::new(ScrollNotificationKind::Update, 0, 42.0),
        ScrollNotificationRecord::new(ScrollNotificationKind::Update, 0, 42.0),
    ];
    let all = bubble_notifications(&chain, None);
    assert_eq!(all.len(), 3);
    assert_eq!(
        all.iter().map(|record| record.depth).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert!(all.iter().all(|record| record.pixels == 42.0));
    let consumed = bubble_notifications(&chain, Some(1));
    assert_eq!(consumed.len(), 2, "a consumed notification stops bubbling");
    assert_eq!(consumed[1].depth, 1);
}

#[test]
fn nested_drag_is_deterministic() {
    let run = || {
        let mut nested = coordinator(200.0, 500.0, 0.0, 100.0);
        let mut outcome = Vec::new();
        for delta in [40.0, -25.0, 10.0, -60.0, 5.0] {
            let result = nested.apply_user_offset(delta);
            outcome.push((
                result.outer_applied,
                result.inner_applied,
                result.unconsumed,
                nested.outer().pixels,
                nested.inner()[0].pixels,
            ));
        }
        outcome
    };
    assert_eq!(run(), run());
    assert!(PRECISION_ERROR_TOLERANCE > 0.0);
}
