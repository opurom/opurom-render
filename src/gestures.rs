//! Deterministic, headless gesture recognition.
//!
//! The engine is a pure integer-only state machine modelled on Flutter's
//! `TapGestureRecognizer` (tap), `DoubleTapGestureRecognizer` (double tap),
//! `MultiTapGestureRecognizer` long tap and `DragGestureRecognizer` (drag).
//! Every timing and threshold is an integer millisecond or pixel value and no
//! wall-clock or host query is used, so the same event stream always yields the
//! same [`Gesture`] on every host.
//!
//! Recognition semantics:
//!
//! - A **tap** is a `down` followed by an `up` on the same node within
//!   [`TAP_TIMEOUT_MS`], with the up still inside [`DRAG_THRESHOLD`] of the
//!   down point. The down point must hit the node's laid-out [`Rect`].
//! - A **double tap** is a second tap on the same node within
//!   [`DOUBLE_TAP_INTERVAL_MS`] of the first, at least [`DOUBLE_TAP_MIN_MS`]
//!   apart and within [`DOUBLE_TAP_SLOP`] of it. The first tap reports
//!   [`Gesture::Tap`] and the second reports [`Gesture::DoubleTap`]; a caller
//!   whose node is configured for `doubleTap` dispatches only the second.
//! - A **long press** fires when a down is still active at
//!   [`LONG_PRESS_MS`] after the down. The engine has no timer, so the check is
//!   evaluated lazily on each event while the pointer is down; once it fires the
//!   active down is consumed and further events on it report nothing.
//! - A **drag** fires once the pointer moves more than [`DRAG_THRESHOLD`] from
//!   the down point. Every later move on that down reports
//!   [`Gesture::Drag`] with the delta from the down point.
//!
//! The engine tracks a single active pointer. A second `down` while a pointer
//! is still down, a `move`/`up` before any `down`, a `move`/`up` on a different
//! node, and any event for an unregistered node are all ignored and report
//! `None`.

use crate::layout::Rect;

/// How the node's `gesture` prop selects which recognized gesture it accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GestureKind {
    Tap,
    DoubleTap,
    LongPress,
    Drag,
}

impl GestureKind {
    /// Strict parse of the IR prop value; unknown values return `None`.
    pub fn parse(s: &str) -> Option<GestureKind> {
        match s {
            "tap" => Some(GestureKind::Tap),
            "doubleTap" => Some(GestureKind::DoubleTap),
            "longPress" => Some(GestureKind::LongPress),
            "drag" => Some(GestureKind::Drag),
            _ => None,
        }
    }

    /// Canonical prop spelling.
    pub fn name(self) -> &'static str {
        match self {
            GestureKind::Tap => "tap",
            GestureKind::DoubleTap => "doubleTap",
            GestureKind::LongPress => "longPress",
            GestureKind::Drag => "drag",
        }
    }
}

/// A gesture recognized by [`GestureEngine`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gesture {
    Tap,
    DoubleTap,
    LongPress,
    /// A drag whose `dx`/`dy` are the delta from the down point.
    Drag {
        dx: i64,
        dy: i64,
    },
}

/// Maximum time from a down to its up for the pair to be a tap.
pub const TAP_TIMEOUT_MS: i64 = 150;
/// Time a pointer must stay down before a long press fires.
pub const LONG_PRESS_MS: i64 = 500;
/// Maximum time between the two taps of a double tap.
pub const DOUBLE_TAP_INTERVAL_MS: i64 = 300;
/// Minimum time between the two taps of a double tap.
pub const DOUBLE_TAP_MIN_MS: i64 = 40;
/// Maximum distance between the two taps of a double tap, in logical px.
pub const DOUBLE_TAP_SLOP: i64 = 100;
/// Distance past which a move becomes a drag, in logical px.
pub const DRAG_THRESHOLD: i64 = 18;

/// The active pointer's down sample.
#[derive(Clone, Copy, Debug)]
struct Down {
    node: u32,
    x: i64,
    y: i64,
    t: i64,
}

/// The most recent completed tap that may start a double tap.
#[derive(Clone, Copy, Debug)]
struct TapMark {
    node: u32,
    x: i64,
    y: i64,
    t: i64,
}

/// Pure, deterministic gesture recognizer.
///
/// Register nodes in draw order (the order returned by `layout`); the last
/// registered node containing a point is the top-most one.
#[derive(Clone, Debug, Default)]
pub struct GestureEngine {
    nodes: Vec<(u32, Rect)>,
    down: Option<Down>,
    last_tap: Option<TapMark>,
    long_pressed: bool,
    drag_active: bool,
}

impl GestureEngine {
    pub fn new() -> GestureEngine {
        GestureEngine::default()
    }

    /// Registers a node's laid-out rect in draw order. Re-registering an id is
    /// a no-op so a stale rect cannot create a duplicate entry.
    pub fn add_node(&mut self, id: u32, rect: Rect) {
        if self.nodes.iter().any(|(node, _)| *node == id) {
            return;
        }
        self.nodes.push((id, rect));
    }

    /// Returns the top-most registered node whose rect contains `(x, y)`.
    pub fn hit_test(&self, x: i64, y: i64) -> Option<u32> {
        self.nodes
            .iter()
            .rev()
            .find(|(_, rect)| hit(rect, x, y))
            .map(|(node, _)| *node)
    }

    /// Starts a gesture on `node_id` at `(x, y)` at time `t`.
    ///
    /// Returns `None`: when a pointer is already down, the node is unknown, or
    /// the point does not hit the node's laid-out rect.
    pub fn on_down(&mut self, node_id: u32, x: i64, y: i64, t: i64) -> Option<Gesture> {
        if self.down.is_some() {
            return None;
        }
        let Some((_, rect)) = self.find(node_id) else {
            return None;
        };
        if !hit(&rect, x, y) {
            return None;
        }
        // A new down outside the double-tap window (or too close for touch
        // noise) starts a fresh series.
        if let Some(mark) = self.last_tap {
            let gap = t.saturating_sub(mark.t);
            if gap > DOUBLE_TAP_INTERVAL_MS
                || gap < DOUBLE_TAP_MIN_MS
                || !within(x, y, mark.x, mark.y, DOUBLE_TAP_SLOP)
            {
                self.last_tap = None;
            }
        }
        self.down = Some(Down {
            node: node_id,
            x,
            y,
            t,
        });
        self.long_pressed = false;
        self.drag_active = false;
        None
    }

    /// Moves the active pointer to `(x, y)` at time `t`.
    ///
    /// Returns `None` before a down, on a different node, while a long press
    /// has consumed the down, or while the move is still within the drag
    /// threshold. Otherwise returns [`Gesture::LongPress`] at the threshold or
    /// [`Gesture::Drag`] with the delta from the down point.
    pub fn on_move(&mut self, node_id: u32, x: i64, y: i64, t: i64) -> Option<Gesture> {
        let Some(down) = self.down else {
            return None;
        };
        if down.node != node_id {
            return None;
        }
        if let Some(gesture) = self.check_long_press(t) {
            return Some(gesture);
        }
        if self.long_pressed {
            return None;
        }
        if self.drag_active || !within(x, y, down.x, down.y, DRAG_THRESHOLD) {
            self.drag_active = true;
            return Some(Gesture::Drag {
                dx: x.saturating_sub(down.x),
                dy: y.saturating_sub(down.y),
            });
        }
        None
    }

    /// Lifts the active pointer at `(x, y)` at time `t`.
    ///
    /// Returns `None` before a down, on a different node, after a long press
    /// or drag has consumed the down, or when the pair is neither a tap nor a
    /// double tap. A completed tap reports [`Gesture::Tap`]; a second qualifying
    /// tap reports [`Gesture::DoubleTap`].
    pub fn on_up(&mut self, node_id: u32, x: i64, y: i64, t: i64) -> Option<Gesture> {
        let Some(down) = self.down else {
            return None;
        };
        // Events for another node are malformed input: ignore them and keep the
        // active down, so a later up on the right node still completes the tap.
        if down.node != node_id {
            return None;
        }
        if let Some(gesture) = self.check_long_press(t) {
            self.down = None;
            return Some(gesture);
        }
        if self.long_pressed || self.drag_active {
            self.down = None;
            return None;
        }
        if !within(x, y, down.x, down.y, DRAG_THRESHOLD)
            || t.saturating_sub(down.t) > TAP_TIMEOUT_MS
        {
            self.down = None;
            return None;
        }
        if let Some(mark) = self.last_tap {
            let gap = t.saturating_sub(mark.t);
            let in_window = gap >= DOUBLE_TAP_MIN_MS
                && gap <= DOUBLE_TAP_INTERVAL_MS
                && within(x, y, mark.x, mark.y, DOUBLE_TAP_SLOP)
                && mark.node == node_id;
            if in_window {
                self.last_tap = None;
                self.down = None;
                return Some(Gesture::DoubleTap);
            }
        }
        self.last_tap = Some(TapMark {
            node: node_id,
            x,
            y,
            t,
        });
        self.down = None;
        Some(Gesture::Tap)
    }

    /// Fires the long press once the active down reaches [`LONG_PRESS_MS`].
    fn check_long_press(&mut self, t: i64) -> Option<Gesture> {
        let Some(down) = self.down else {
            return None;
        };
        if !self.long_pressed && !self.drag_active && t.saturating_sub(down.t) >= LONG_PRESS_MS {
            self.long_pressed = true;
            return Some(Gesture::LongPress);
        }
        None
    }

    fn find(&self, id: u32) -> Option<(u32, Rect)> {
        self.nodes.iter().find(|(node, _)| *node == id).copied()
    }
}

/// Point-in-rect test for a laid-out rect.
///
/// Integer-only and panic-free: `saturating_add` bounds the upper edge so
/// extreme coordinates cannot overflow, and a negative width rejects the point.
pub fn hit(rect: &Rect, x: i64, y: i64) -> bool {
    x >= rect.x
        && y >= rect.y
        && x < rect.x.saturating_add(rect.w)
        && y < rect.y.saturating_add(rect.h)
}

/// Squared distance, computed in `i128` so extreme integer coordinates cannot
/// overflow or panic.
fn distance_sq(x: i64, y: i64, ox: i64, oy: i64) -> i128 {
    let dx = x as i128 - ox as i128;
    let dy = y as i128 - oy as i128;
    dx * dx + dy * dy
}

/// True when `(x, y)` is within `limit` logical px of `(ox, oy)`.
fn within(x: i64, y: i64, ox: i64, oy: i64, limit: i64) -> bool {
    distance_sq(x, y, ox, oy) <= (limit as i128) * (limit as i128)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Rect;

    fn rect(x: i64, y: i64, w: i64, h: i64) -> Rect {
        Rect { x, y, w, h }
    }

    /// A 200x200 node at the origin; one engine is enough for most cases.
    fn engine() -> GestureEngine {
        let mut engine = GestureEngine::new();
        engine.add_node(1, rect(0, 0, 200, 200));
        engine
    }

    #[test]
    fn tap_fires_once_on_down_up_on_the_same_node() {
        let mut engine = engine();
        assert_eq!(engine.on_down(1, 10, 10, 0), None);
        assert_eq!(engine.on_up(1, 10, 10, 10), Some(Gesture::Tap));
        // The down is consumed: a second up reports nothing.
        assert_eq!(engine.on_up(1, 10, 10, 20), None);
    }

    #[test]
    fn double_tap_sequence_reports_tap_then_double_tap() {
        let mut engine = engine();
        assert_eq!(engine.on_down(1, 10, 10, 0), None);
        assert_eq!(engine.on_up(1, 10, 10, 10), Some(Gesture::Tap));
        assert_eq!(engine.on_down(1, 10, 10, 60), None);
        assert_eq!(engine.on_up(1, 10, 10, 80), Some(Gesture::DoubleTap));
        // A third tap starts a fresh series.
        assert_eq!(engine.on_down(1, 10, 10, 200), None);
        assert_eq!(engine.on_up(1, 10, 10, 210), Some(Gesture::Tap));
    }

    #[test]
    fn double_tap_does_not_merge_across_the_interval() {
        let mut engine = engine();
        assert_eq!(engine.on_down(1, 10, 10, 0), None);
        assert_eq!(engine.on_up(1, 10, 10, 10), Some(Gesture::Tap));
        // The second down lands past the double-tap interval.
        assert_eq!(engine.on_down(1, 10, 10, 400), None);
        assert_eq!(engine.on_up(1, 10, 10, 410), Some(Gesture::Tap));
    }

    #[test]
    fn long_press_fires_at_threshold_and_not_before() {
        let mut engine = engine();
        assert_eq!(engine.on_down(1, 10, 10, 0), None);
        assert_eq!(engine.on_move(1, 10, 10, LONG_PRESS_MS - 1), None);
        assert_eq!(
            engine.on_move(1, 10, 10, LONG_PRESS_MS),
            Some(Gesture::LongPress)
        );
        // It fires once and ignores repeats.
        assert_eq!(engine.on_move(1, 10, 10, LONG_PRESS_MS + 1), None);
        assert_eq!(engine.on_up(1, 10, 10, LONG_PRESS_MS + 2), None);
    }

    #[test]
    fn long_press_fires_on_lift_after_the_threshold() {
        let mut engine = engine();
        assert_eq!(engine.on_down(1, 10, 10, 0), None);
        assert_eq!(
            engine.on_up(1, 10, 10, LONG_PRESS_MS),
            Some(Gesture::LongPress)
        );
    }

    #[test]
    fn drag_computes_the_delta_from_the_down_point() {
        let mut engine = engine();
        assert_eq!(engine.on_down(1, 100, 100, 0), None);
        // Still inside the slop: no drag yet.
        assert_eq!(engine.on_move(1, 110, 100, 10), None);
        assert_eq!(
            engine.on_move(1, 140, 90, 20),
            Some(Gesture::Drag { dx: 40, dy: -10 })
        );
        // Later moves keep reporting the delta from the original down point.
        assert_eq!(
            engine.on_move(1, 150, 120, 30),
            Some(Gesture::Drag { dx: 50, dy: 20 })
        );
        // The lift ends the drag without a second recognition.
        assert_eq!(engine.on_up(1, 150, 120, 40), None);
    }

    #[test]
    fn move_before_down_is_ignored() {
        let mut engine = engine();
        assert_eq!(engine.on_move(1, 50, 50, 10), None);
        assert_eq!(engine.on_down(1, 50, 50, 20), None);
        assert_eq!(engine.on_up(1, 50, 50, 30), Some(Gesture::Tap));
    }

    #[test]
    fn up_before_down_is_ignored() {
        let mut engine = engine();
        assert_eq!(engine.on_up(1, 50, 50, 10), None);
        // The engine is still idle and a later down/up still taps.
        assert_eq!(engine.on_down(1, 50, 50, 20), None);
        assert_eq!(engine.on_up(1, 50, 50, 30), Some(Gesture::Tap));
    }

    #[test]
    fn events_on_a_different_node_are_ignored() {
        let mut engine = GestureEngine::new();
        engine.add_node(1, rect(0, 0, 100, 100));
        engine.add_node(2, rect(0, 0, 100, 100));
        assert_eq!(engine.on_down(1, 10, 10, 0), None);
        assert_eq!(engine.on_move(2, 10, 10, 10), None);
        assert_eq!(engine.on_up(2, 10, 10, 20), None);
        // The original down is still live and can still tap.
        assert_eq!(engine.on_up(1, 10, 10, 30), Some(Gesture::Tap));
    }

    #[test]
    fn unknown_node_ids_are_ignored() {
        let mut engine = engine();
        assert_eq!(engine.on_down(99, 10, 10, 0), None);
        assert_eq!(engine.on_move(99, 10, 10, 10), None);
        assert_eq!(engine.on_up(99, 10, 10, 20), None);
    }

    #[test]
    fn a_down_outside_the_node_rect_is_ignored() {
        let mut engine = engine();
        assert_eq!(engine.on_down(1, 1000, 1000, 0), None);
        assert_eq!(engine.on_up(1, 1000, 1000, 10), None);
    }

    #[test]
    fn two_nodes_at_the_same_point_resolve_to_the_top_most() {
        let mut engine = GestureEngine::new();
        engine.add_node(1, rect(0, 0, 100, 100));
        engine.add_node(2, rect(0, 0, 100, 100));
        assert_eq!(engine.hit_test(50, 50), Some(2));
        // Outside both rects there is no hit.
        assert_eq!(engine.hit_test(150, 50), None);
        // Only the bottom node covers this point.
        assert_eq!(engine.hit_test(150, 150), None);
    }

    #[test]
    fn hit_helper_matches_inclusive_lower_and_exclusive_upper_edges() {
        assert!(hit(&rect(10, 10, 20, 20), 10, 10));
        assert!(hit(&rect(10, 10, 20, 20), 29, 29));
        assert!(!hit(&rect(10, 10, 20, 20), 9, 10));
        assert!(!hit(&rect(10, 10, 20, 20), 10, 30));
        assert!(!hit(&rect(10, 10, 20, 20), 30, 10));
        // Degenerate and extreme rects never panic.
        assert!(!hit(&rect(10, 10, 0, 0), 10, 10));
        assert!(!hit(
            &rect(i64::MIN, i64::MIN, i64::MAX, i64::MAX),
            i64::MAX,
            i64::MAX
        ));
    }

    #[test]
    fn gesture_kind_parses_the_four_prop_values() {
        assert_eq!(GestureKind::parse("tap"), Some(GestureKind::Tap));
        assert_eq!(
            GestureKind::parse("doubleTap"),
            Some(GestureKind::DoubleTap)
        );
        assert_eq!(
            GestureKind::parse("longPress"),
            Some(GestureKind::LongPress)
        );
        assert_eq!(GestureKind::parse("drag"), Some(GestureKind::Drag));
        assert_eq!(GestureKind::parse("swipe"), None);
    }

    #[test]
    fn extreme_coordinates_and_times_never_panic() {
        let mut engine = GestureEngine::new();
        engine.add_node(1, rect(0, 0, 100, 100));
        assert_eq!(engine.on_down(1, 50, 50, 0), None);
        // A huge move saturates the delta instead of overflowing.
        assert_eq!(
            engine.on_move(1, i64::MAX, i64::MIN, 10),
            Some(Gesture::Drag {
                dx: i64::MAX - 50,
                dy: i64::MIN
            })
        );
        assert_eq!(engine.on_up(1, i64::MAX, i64::MIN, 20), None);

        // Time going backwards saturates to zero and still resolves deterministically.
        let mut engine = GestureEngine::new();
        engine.add_node(1, rect(0, 0, 100, 100));
        assert_eq!(engine.on_down(1, 10, 10, i64::MAX), None);
        assert_eq!(engine.on_up(1, 10, 10, i64::MIN), Some(Gesture::Tap));
    }
}
