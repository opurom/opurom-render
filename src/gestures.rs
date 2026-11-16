//! Deterministic, headless gesture recognition.
//!
//! The engine is a pure integer-only state machine;
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
//! Gesture arena arbitration:
//!
//! The four recognizers (tap, double tap, long press, drag — drag being the
//! recognizer that drives a scroll) compete in one arena per pointer sequence,
//! opened at `down` and closed when the sequence ends:
//!
//! - The **first accept wins**. `drag` accepts on the first `move` past
//!   [`DRAG_THRESHOLD`]; `longPress` accepts once the elapsed time reaches
//!   [`LONG_PRESS_MS`]; `tap` and `doubleTap` stay pending to `up` and reject
//!   themselves the moment the displacement exceeds [`DRAG_THRESHOLD`], since
//!   they can no longer win.
//! - Rejecting the losers makes them inert: only the winner emits afterwards.
//!   A `drag` winner keeps emitting deltas; `longPress`/`tap`/`doubleTap`
//!   emit once and the sequence is consumed.
//! - At `up` with no winner, only members whose own conditions hold are
//!   eligible and the fixed priority table picks among them. A bare `up` never
//!   accepts a `drag`: a lift without a prior out-of-slop move is a tap or
//!   nothing.
//! - When several members become acceptable on the same event, the fixed
//!   priority resolves it: `longPress` (3) > `drag` (2) > `doubleTap` (1) >
//!   `tap` (0). This matches the engine's historical evaluation order, so a
//!   single-recognizer sequence behaves exactly as before the arena existed.
//!
//! The winner stays readable through [`GestureEngine::arena_winner`] after the
//! sequence ends, until the next `down` opens a fresh arena.
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

/// Fixed arbitration priority of one arena member: when several members become
/// acceptable on the same event, the highest value wins. The order mirrors the
/// engine's historical evaluation order (long press, then drag, then double
/// tap, then tap), so arena arbitration never changes a single-recognizer
/// sequence's output.
const fn arena_priority(kind: GestureKind) -> u8 {
    match kind {
        GestureKind::LongPress => 3,
        GestureKind::Drag => 2,
        GestureKind::DoubleTap => 1,
        GestureKind::Tap => 0,
    }
}

/// The arbitration state of one pointer sequence: which of the four members
/// has won, and which members are ruled out. Opened at `down`, decided at the
/// first accept or at `up`; the winner stays readable until the next `down`.
#[derive(Clone, Debug, Default)]
struct Arena {
    winner: Option<GestureKind>,
    rejected: [bool; 4],
}

impl Arena {
    /// Index of one member's slot in the fixed four-member table.
    const fn slot(kind: GestureKind) -> usize {
        match kind {
            GestureKind::Tap => 0,
            GestureKind::DoubleTap => 1,
            GestureKind::LongPress => 2,
            GestureKind::Drag => 3,
        }
    }

    /// Open a fresh arena for a new pointer sequence.
    fn open(&mut self) {
        *self = Arena::default();
    }

    /// Rule one member out; it can never win this sequence.
    fn reject(&mut self, kind: GestureKind) {
        self.rejected[Arena::slot(kind)] = true;
    }

    /// Accept one member: the first accept wins, every other member is
    /// rejected at once.
    fn accept(&mut self, kind: GestureKind) {
        if self.winner.is_none() {
            self.winner = Some(kind);
            for other in [
                GestureKind::Tap,
                GestureKind::DoubleTap,
                GestureKind::LongPress,
                GestureKind::Drag,
            ] {
                if other != kind {
                    self.reject(other);
                }
            }
        }
    }

    /// The member that won the current sequence, readable after it ends.
    fn winner(&self) -> Option<GestureKind> {
        self.winner
    }

    fn is_rejected(&self, kind: GestureKind) -> bool {
        self.rejected[Arena::slot(kind)]
    }
}

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
    arena: Arena,
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
        // Every recognizer joins a fresh arena for this pointer sequence.
        self.arena.open();
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
        // A decided arena lets only the drag winner keep emitting deltas; a
        // long press or tap winner already consumed the sequence.
        if let Some(winner) = self.arena.winner() {
            if winner == GestureKind::Drag {
                return Some(Gesture::Drag {
                    dx: x.saturating_sub(down.x),
                    dy: y.saturating_sub(down.y),
                });
            }
            return None;
        }
        let displaced = !within(x, y, down.x, down.y, DRAG_THRESHOLD);
        if displaced {
            // Past the slop the tap members can no longer win this sequence.
            self.arena.reject(GestureKind::Tap);
            self.arena.reject(GestureKind::DoubleTap);
        }
        // Same-event accept tie resolved by the fixed priority table.
        let accepted = [
            (t.saturating_sub(down.t) >= LONG_PRESS_MS).then_some(GestureKind::LongPress),
            displaced.then_some(GestureKind::Drag),
        ]
        .into_iter()
        .flatten()
        .max_by_key(|kind| arena_priority(*kind));
        match accepted {
            Some(GestureKind::LongPress) => {
                self.arena.accept(GestureKind::LongPress);
                Some(Gesture::LongPress)
            }
            Some(GestureKind::Drag) => {
                self.arena.accept(GestureKind::Drag);
                Some(Gesture::Drag {
                    dx: x.saturating_sub(down.x),
                    dy: y.saturating_sub(down.y),
                })
            }
            _ => None,
        }
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
        // A decided arena ends the sequence silently at the lift.
        if self.arena.winner().is_some() {
            self.down = None;
            return None;
        }
        let elapsed = t.saturating_sub(down.t);
        let tap_ok = within(x, y, down.x, down.y, DRAG_THRESHOLD) && elapsed <= TAP_TIMEOUT_MS;
        let double_ok = tap_ok
            && self.last_tap.is_some_and(|mark| {
                let gap = t.saturating_sub(mark.t);
                gap >= DOUBLE_TAP_MIN_MS
                    && gap <= DOUBLE_TAP_INTERVAL_MS
                    && within(x, y, mark.x, mark.y, DOUBLE_TAP_SLOP)
                    && mark.node == node_id
            });
        // Nobody accepted during the sequence: the lift itself decides among
        // the members whose own conditions hold, by the fixed priority table.
        // A bare lift never accepts a drag.
        let winner = [
            (elapsed >= LONG_PRESS_MS).then_some(GestureKind::LongPress),
            double_ok.then_some(GestureKind::DoubleTap),
            tap_ok.then_some(GestureKind::Tap),
        ]
        .into_iter()
        .flatten()
        .max_by_key(|kind| arena_priority(*kind));
        self.down = None;
        match winner {
            Some(GestureKind::LongPress) => {
                self.arena.accept(GestureKind::LongPress);
                Some(Gesture::LongPress)
            }
            Some(GestureKind::DoubleTap) => {
                self.last_tap = None;
                self.arena.accept(GestureKind::DoubleTap);
                Some(Gesture::DoubleTap)
            }
            Some(GestureKind::Tap) => {
                self.last_tap = Some(TapMark {
                    node: node_id,
                    x,
                    y,
                    t,
                });
                self.arena.accept(GestureKind::Tap);
                Some(Gesture::Tap)
            }
            _ => None,
        }
    }

    /// The arena winner of the current (or just-ended) pointer sequence, until
    /// the next `down` opens a fresh arena. `None` while undecided or when the
    /// sequence ended with no winner.
    pub fn arena_winner(&self) -> Option<GestureKind> {
        self.arena.winner()
    }

    /// Whether `kind` has been ruled out of the current arena by an accept or
    /// by crossing the drag slop.
    pub fn arena_rejected(&self, kind: GestureKind) -> bool {
        self.arena.is_rejected(kind)
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

    /// Gate 1: press on a button, move past the drag slop, release. The drag
    /// member accepts first, the tap members are rejected, and the lift of a
    /// decided sequence emits nothing.
    #[test]
    fn drag_accepts_past_slop_and_tap_never_fires() {
        let mut engine = engine();
        assert_eq!(engine.on_down(1, 20, 20, 0), None);
        // Inside the slop: no member accepts yet.
        assert_eq!(engine.on_move(1, 30, 20, 10), None);
        assert_eq!(engine.arena_winner(), None);
        // Past the slop: drag is the first accept.
        assert_eq!(
            engine.on_move(1, 60, 20, 20),
            Some(Gesture::Drag { dx: 40, dy: 0 })
        );
        assert_eq!(engine.arena_winner(), Some(GestureKind::Drag));
        assert!(engine.arena_rejected(GestureKind::Tap));
        assert!(engine.arena_rejected(GestureKind::LongPress));
        // The decided sequence ends silently at the lift: no tap.
        assert_eq!(engine.on_up(1, 60, 20, 40), None);
        assert_eq!(engine.on_up(1, 20, 20, 50), None);
    }

    /// Gate 2: a plain press-and-lift with no displacement lets the tap member
    /// win at the lift, and drag/long press are rejected.
    #[test]
    fn tap_wins_a_lift_with_no_displacement() {
        let mut engine = engine();
        assert_eq!(engine.on_down(1, 20, 20, 0), None);
        assert_eq!(engine.on_up(1, 21, 21, 20), Some(Gesture::Tap));
        assert_eq!(engine.arena_winner(), Some(GestureKind::Tap));
        assert!(engine.arena_rejected(GestureKind::Drag));
    }

    /// Gate 3: inside a scroll region a drag-capable node still resolves a
    /// click by threshold: a lift within the slop is a tap, a move past it is
    /// a drag.
    #[test]
    fn scroll_region_click_is_resolved_by_the_threshold() {
        // The scroll region: a node whose recognizers include drag.
        let mut engine = engine();
        // Click: small jitter, then lift, stays inside the slop.
        assert_eq!(engine.on_down(1, 40, 40, 0), None);
        assert_eq!(engine.on_move(1, 45, 43, 10), None);
        assert_eq!(engine.on_up(1, 45, 43, 30), Some(Gesture::Tap));
        // Drag: a move crosses the slop, the drag member wins for the sequence.
        assert_eq!(engine.on_down(1, 40, 40, 100), None);
        assert_eq!(
            engine.on_move(1, 40, 70, 110),
            Some(Gesture::Drag { dx: 0, dy: 30 })
        );
        assert_eq!(engine.arena_winner(), Some(GestureKind::Drag));
        assert_eq!(engine.on_up(1, 40, 70, 130), None);
    }

    /// Gate 4: with nobody accepting during the sequence the lift decides by
    /// the fixed priority — a held pointer resolves to long press, a second
    /// qualifying tap resolves to double tap over plain tap.
    #[test]
    fn lift_without_an_accept_is_resolved_by_priority() {
        let mut engine = engine();
        // Held past LONG_PRESS_MS with no moves: long press outranks tap.
        assert_eq!(engine.on_down(1, 10, 10, 0), None);
        assert_eq!(
            engine.on_up(1, 10, 10, LONG_PRESS_MS),
            Some(Gesture::LongPress)
        );
        assert_eq!(engine.arena_winner(), Some(GestureKind::LongPress));
        // A second qualifying tap: at that lift tap and doubleTap are both
        // eligible and doubleTap has the higher priority.
        let mut pair = GestureEngine::new();
        pair.add_node(1, rect(0, 0, 200, 200));
        assert_eq!(pair.on_down(1, 10, 10, 0), None);
        assert_eq!(pair.on_up(1, 10, 10, 10), Some(Gesture::Tap));
        assert_eq!(pair.on_down(1, 10, 10, 60), None);
        assert_eq!(pair.on_up(1, 10, 10, 80), Some(Gesture::DoubleTap));
        assert_eq!(pair.arena_winner(), Some(GestureKind::DoubleTap));
        assert!(pair.arena_rejected(GestureKind::Tap));
        // Nobody eligible: a lift past the tap timeout with no long press
        // (early, then time jumps past the window on a fresh sequence) leaves
        // the arena without a winner.
        let mut undecided = GestureEngine::new();
        undecided.add_node(1, rect(0, 0, 200, 200));
        assert_eq!(undecided.on_down(1, 10, 10, 0), None);
        assert_eq!(undecided.on_move(1, 14, 10, 10), None);
        assert_eq!(undecided.on_up(1, 14, 10, TAP_TIMEOUT_MS + 1), None);
        assert_eq!(undecided.arena_winner(), None);
    }

    /// A same-event tie on one move resolves by the fixed priority table:
    /// elapsed past LONG_PRESS_MS and displacement past the slop arrive
    /// together, long press (3) outranks drag (2), and drag stays inert.
    #[test]
    fn same_event_tie_follows_the_fixed_priority() {
        let mut engine = engine();
        assert_eq!(engine.on_down(1, 10, 10, 0), None);
        assert_eq!(
            engine.on_move(1, 10 + DRAG_THRESHOLD + 1, 10, LONG_PRESS_MS),
            Some(Gesture::LongPress)
        );
        assert_eq!(engine.arena_winner(), Some(GestureKind::LongPress));
        assert!(engine.arena_rejected(GestureKind::Drag));
        // The drag member lost: later moves report nothing.
        assert_eq!(
            engine.on_move(1, 10 + DRAG_THRESHOLD + 2, 10, LONG_PRESS_MS + 1),
            None
        );
        assert_eq!(engine.on_up(1, 10, 10, LONG_PRESS_MS + 2), None);
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
