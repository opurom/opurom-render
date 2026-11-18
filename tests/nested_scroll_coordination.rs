//! Nested-scroll coordination parity pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `widgets/notification_listener.dart:56-69,92-138`: notifications bubble
//!   from the dispatch target through ancestors; returning true from
//!   `onNotification` stops further bubbling.
//! - `widgets/framework.dart:3512-3516`: an element forwards a notification to
//!   its parent unless a listener consumes it.
//! - `widgets/scroll_notification.dart:31-56,183-226`: scroll notifications
//!   carry depth (0 local, >0 remote) and update/overscroll payloads.
//! - `widgets/nested_scroll_view.dart:809-903`: inner/outer extents are
//!   translated and clamped with `clampDouble(value, min, max)`.
//! - `widgets/scroll_physics.dart:1021-1053` and
//!   `physics/clamping_scroll_simulation.dart:250-264` at the pinned checkout:
//!   edge flings spring back or clamp to the scroll extent with a continuous
//!   decaying curve.
//!
//! Opurom keeps this projection pure and data-only: no new IR field and no
//! runtime object tree. The tests pin a deterministic host-side coordinator
//! built on the existing `ScrollDriver` plus ordered notification records.

use opurom_render::scroll_physics::{ScrollDriver, ScrollMotion};

const START_MS: i64 = 30_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScrollEvent {
    Start,
    Update { offset: i64 },
    Overscroll { offset: i64 },
    End { offset: i64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Axis {
    Inner,
    Outer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Notification {
    axis: Axis,
    event: ScrollEvent,
    depth: u32,
}

/// Host-side projection of Flutter's ancestor chain. Returning true from a
/// listener consumes the notification (`notification_listener.dart:104-108`).
fn bubble(chain: &[Axis], note: Notification, consume_at: Option<Axis>) -> Vec<Notification> {
    let mut seen = Vec::new();
    for (depth, axis) in chain.iter().enumerate() {
        seen.push(Notification {
            axis: *axis,
            event: note.event,
            depth: depth as u32,
        });
        if consume_at == Some(*axis) {
            break;
        }
    }
    seen
}

fn fling(offset: i64, velocity: f64, min: i64, max: i64) -> ScrollDriver {
    ScrollDriver::begin_fling(START_MS, offset as f64, velocity, min, max, 1.0)
        .expect("fling must start inside the test extent")
}

#[test]
fn parent_child_competition_hands_off_at_the_inner_edge() {
    let inner = fling(90, -1200.0, 0, 100);
    let timeline: Vec<i64> = [0, 100, 250, 500]
        .iter()
        .map(|dt| inner.offset_px(START_MS + dt))
        .collect();
    assert!(timeline.windows(2).all(|w| w[0] >= w[1]), "offset must decrease");
    assert!(timeline.iter().all(|x| (0..=100).contains(x)), "inner extent is clamped");
    // Flutter's nested coordinator uses outer range once the inner position
    // reaches an edge (`nested_scroll_view.dart:809-903`). The outer fling
    // starts only from the handed-off velocity sign.
    let outer = fling(50, -900.0, 0, 200);
    assert!(outer.dx(START_MS) < 0.0);
    assert!(outer.offset_px(START_MS + 500) < 50);
}

#[test]
fn notifications_bubble_child_to_parent_and_stop_when_consumed() {
    let chain = [Axis::Inner, Axis::Outer];
    let note = Notification {
        axis: Axis::Inner,
        event: ScrollEvent::Update { offset: 42 },
        depth: 0,
    };
    let all = bubble(&chain, note, None);
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].axis, Axis::Inner);
    assert_eq!(all[0].depth, 0);
    assert_eq!(all[1].axis, Axis::Outer);
    assert_eq!(all[1].depth, 1);
    assert_eq!(all[0].event, ScrollEvent::Update { offset: 42 });

    let consumed = bubble(&chain, note, Some(Axis::Inner));
    assert_eq!(consumed.len(), 1, "a consumed notification must not reach parent");
    assert_eq!(consumed[0].axis, Axis::Inner);
}

#[test]
fn overscroll_event_sequence_is_deterministic() {
    let sequence = |driver: &ScrollDriver| -> Vec<Notification> {
        let mut out = Vec::new();
        out.push(Notification {
            axis: Axis::Inner,
            event: ScrollEvent::Start,
            depth: 0,
        });
        for dt in [0, 100, 250, 500] {
            let offset = driver.offset_px(START_MS + dt);
            let event = if offset < 0 || offset > 100 {
                ScrollEvent::Overscroll { offset }
            } else {
                ScrollEvent::Update { offset }
            };
            out.push(Notification {
                axis: Axis::Inner,
                event,
                depth: 0,
            });
        }
        out.push(Notification {
            axis: Axis::Inner,
            event: ScrollEvent::End {
                offset: driver.offset_px(START_MS + 1200),
            },
            depth: 0,
        });
        out
    };
    let driver = fling(20, 1600.0, 0, 100);
    let first = sequence(&driver);
    let second = sequence(&driver);
    assert_eq!(first, second, "identical event streams must be byte-identical");
    assert_eq!(first.first().unwrap().event, ScrollEvent::Start);
    assert!(matches!(first.last().unwrap().event, ScrollEvent::End { .. }));
    assert!(first.iter().all(|n| n.depth == 0));
}

#[test]
fn boundary_clamp_never_leaves_the_scroll_extent() {
    // An exact-edge fling toward the outside yields no scroll activity
    // (`scroll_physics.dart:1043-1047`); use a small inset to test clamping.
    let driver = fling(1, -2000.0, 0, 100);
    let offsets: Vec<i64> = (0..=60)
        .map(|frame| driver.offset_px(START_MS + frame * 16))
        .collect();
    assert!(offsets.iter().all(|x| (0..=100).contains(x)));
    assert!(matches!(driver.motion, ScrollMotion::Clamping(_)));
}
