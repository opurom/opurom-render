//! Deterministic, headless gesture recognition.
//!
//! The engine is a pure integer-only state machine. Every timing and threshold
//! is an integer millisecond or pixel value and no wall-clock or host query is
//! used. The host injects [`PointerEvent`]s and ticks with the time it reads
//! from its own clock; the engine only ever reads the supplied `time_ms`.
//!
//! Flutter anchors (flutter 3.50, packages/flutter/lib/src/gestures):
//! - `constants.dart:29` long press 500ms, `:35` double tap 300ms,
//!   `:39` min gap 40ms, `:49` double tap slop 100px, `:65` touch slop 18px.
//! - `arena.dart:157` sweep: the first member wins, all others are rejected;
//!   `arena.dart:140` hold delays the sweep until `release` (`:211`).
//! - `tap.dart:348` a tap accepts on lift; there is no down-to-up timeout.
//! - `long_press.dart:641` deadline expiry accepts; `:661` lift before the
//!   deadline rejects; `:668` cancel resets; `:679` move past slop cancels.
//! - `multitap.dart:340-345` the second tap accepts and emits double tap;
//!   `:355` the 300ms timer resets the pending first tap.
//! - `hit_test.dart:30` hit testing returns a leaf-to-root path.
//! - `scale.dart:345` ScaleGestureRecognizer; `:436-437` initial/current span,
//!   `:455` span ratio, `:641` focal mean, `:744-748` slop acceptance,
//!   `:806` acceptGesture. Blob fe3cdf27a414de2f112c58ad5d633831f7f1536e.
//! - `constants.dart:65` kTouchSlop 18px, `:80` kScaleSlop == kTouchSlop.
//!   Blob ae23eac1178aff50cbb5b0f29566a0e45589dd99.
//!
//! The five live phases are down -> pending -> triggered -> cancelled -> up:
//! a down enters pending, a tick at the deadline triggers, leaving the hit
//! region cancels, re-entering restarts the timer, and a lift closes the
//! sequence. A cancelled long press can never fire.

use crate::layout::Rect;

/// The pointer signal carried by a [`PointerEvent`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerKind {
    Down,
    Move,
    Up,
    Cancel,
}

/// One host-supplied pointer signal. Local and global positions are kept
/// separate so a scrolled or transformed host can move the pointer without
/// the recognizer losing its node-local frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointerEvent {
    pub kind: PointerKind,
    pub pointer_id: u32,
    pub time_ms: i64,
    pub local_x: i64,
    pub local_y: i64,
    pub global_x: i64,
    pub global_y: i64,
}

impl PointerEvent {
    /// Full constructor; callers supply every field explicitly.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        kind: PointerKind,
        pointer_id: u32,
        time_ms: i64,
        local_x: i64,
        local_y: i64,
        global_x: i64,
        global_y: i64,
    ) -> PointerEvent {
        PointerEvent {
            kind,
            pointer_id,
            time_ms,
            local_x,
            local_y,
            global_x,
            global_y,
        }
    }

    /// Pointer 0 with local == global, for untransformed single-pointer hosts.
    pub fn at(kind: PointerKind, x: i64, y: i64, t: i64) -> PointerEvent {
        PointerEvent::new(kind, 0, t, x, y, x, y)
    }
}

/// How the node's `gesture` prop selects which recognized gesture it accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GestureKind {
    Tap,
    DoubleTap,
    LongPress,
    Drag,
    /// Two-finger pinch; joins the same arena as the other recognizers
    /// (`scale.dart:345`).
    Scale,
}

impl GestureKind {
    /// Strict parse of the IR prop value; unknown values return `None`.
    pub fn parse(s: &str) -> Option<GestureKind> {
        match s {
            "tap" => Some(GestureKind::Tap),
            "doubleTap" => Some(GestureKind::DoubleTap),
            "longPress" => Some(GestureKind::LongPress),
            "drag" => Some(GestureKind::Drag),
            "scale" => Some(GestureKind::Scale),
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
            GestureKind::Scale => "scale",
        }
    }

    /// A node with no explicit gesture prop competes with every recognizer.
    /// Scale is last so the sweep order of the four legacy kinds is unchanged.
    const fn all() -> [GestureKind; 5] {
        [
            GestureKind::Tap,
            GestureKind::DoubleTap,
            GestureKind::LongPress,
            GestureKind::Drag,
            GestureKind::Scale,
        ]
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
    /// A long press that was accepted and then moved. The four offsets follow
    /// `LongPressMoveUpdateDetails` (`long_press.dart:176-196`): `dx`/`dy` are
    /// global `offsetFromOrigin`, `ldx`/`ldy` local `localOffsetFromOrigin`.
    LongPressMoveUpdate {
        global_x: i64,
        global_y: i64,
        local_x: i64,
        local_y: i64,
        dx: i64,
        dy: i64,
        ldx: i64,
        ldy: i64,
    },
    /// A two-finger scale. `scale` is fixed point, 1024 == 1.0, computed as
    /// current span / initial span (`scale.dart:455,463`). `focal_x`/`focal_y`
    /// are the current midpoint of the two pointers (`scale.dart:641`).
    Scale {
        scale: i64,
        focal_x: i64,
        focal_y: i64,
    },
}

/// Time a pointer must stay down before a long press fires (`constants.dart:29`).
pub const LONG_PRESS_MS: i64 = 500;
/// Maximum time between the two taps of a double tap (`constants.dart:35`).
pub const DOUBLE_TAP_INTERVAL_MS: i64 = 300;
/// Minimum time between the two taps of a double tap (`constants.dart:39`).
pub const DOUBLE_TAP_MIN_MS: i64 = 40;
/// Maximum distance between the two taps of a double tap (`constants.dart:49`).
pub const DOUBLE_TAP_SLOP: i64 = 100;
/// Distance past which a move becomes a drag (`constants.dart:65`).
pub const DRAG_THRESHOLD: i64 = 18;
/// Scale fixed point: 1024 == 1.0, matching the IR/raster unit convention.
pub const SCALE_UNIT: i64 = 1024;
/// Span change that accepts a scale gesture (`constants.dart:80` kScaleSlop).
pub const SCALE_SLOP: i64 = 18;
/// Focal movement that also accepts a scale (`constants.dart:76` kPanSlop).
pub const PAN_SLOP: i64 = 36;
/// Minimum interactive target in canvas px. Flutter's `kMinInteractiveDimension`
/// (`material/constants.dart:27`) resolved to the IR design domain, not dp.
pub const MIN_HIT_SIZE: i64 = 48;

/// The five live states of one pointer sequence. `Idle` is the resting state
/// between sequences and is not part of the loop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Idle,
    Down,
    Pending,
    Triggered,
    Cancelled,
    Up,
}

/// One arena member: a recognizer on one node of the hit path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Member {
    node: u32,
    kind: GestureKind,
}

/// The pointer arena. Members join in hit-path order (leaf to root), as in
/// `GestureArenaManager.add` (`arena.dart:71`). The first explicit accept wins;
/// otherwise the sweep at lift or timer release accepts the first surviving
/// member (`arena.dart:157-171`).
#[derive(Clone, Debug, Default)]
struct Arena {
    members: Vec<Member>,
    rejected: Vec<bool>,
    winner: Option<Member>,
    held: bool,
    pending_sweep: bool,
}

impl Arena {
    fn open(&mut self, members: Vec<Member>) {
        *self = Arena {
            rejected: vec![false; members.len()],
            members,
            ..Arena::default()
        };
    }

    fn is_rejected(&self, kind: GestureKind) -> bool {
        self.members
            .iter()
            .zip(&self.rejected)
            .any(|(m, r)| *r && m.kind == kind)
    }

    fn reject(&mut self, node: u32, kind: GestureKind) {
        for (m, r) in self.members.iter().zip(&mut self.rejected) {
            if m.node == node && m.kind == kind {
                *r = true;
            }
        }
    }

    fn unreject(&mut self, node: u32, kind: GestureKind) {
        for (m, r) in self.members.iter().zip(&mut self.rejected) {
            if m.node == node && m.kind == kind {
                *r = false;
            }
        }
    }

    fn has(&self, node: u32, kind: GestureKind) -> bool {
        self.members
            .iter()
            .any(|m| m.node == node && m.kind == kind)
    }

    /// First explicit accept wins and rejects every other member.
    fn accept(&mut self, node: u32, kind: GestureKind) -> bool {
        if self.winner.is_some() {
            return false;
        }
        let Some(member) = self
            .members
            .iter()
            .find(|m| m.node == node && m.kind == kind)
            .copied()
        else {
            return false;
        };
        self.winner = Some(member);
        for (i, m) in self.members.iter().enumerate() {
            if m != &member {
                self.rejected[i] = true;
            }
        }
        true
    }

    fn winner(&self) -> Option<Member> {
        self.winner
    }

    /// Delay the sweep while a double-tap window is open (`arena.dart:140`).
    fn hold(&mut self) {
        self.held = true;
    }

    fn release(&mut self) -> Option<Member> {
        if !self.held {
            return None;
        }
        self.held = false;
        self.pending_sweep = false;
        self.sweep()
    }

    /// First surviving member wins (`arena.dart:157-171`).
    fn sweep(&mut self) -> Option<Member> {
        if self.winner.is_some() {
            return self.winner;
        }
        if self.held {
            self.pending_sweep = true;
            return None;
        }
        let first = (0..self.members.len()).find(|i| !self.rejected[*i])?;
        let member = self.members[first];
        self.accept(member.node, member.kind);
        Some(member)
    }
}

/// The active pointer's down sample.
#[derive(Clone, Copy, Debug)]
struct Down {
    node: u32,
    pointer_id: u32,
    local_x: i64,
    local_y: i64,
    global_x: i64,
    global_y: i64,
    t: i64,
    /// Restarted when the pointer leaves and re-enters the hit region.
    origin_t: i64,
}

/// Two-finger span tracker for the active sequence. All coordinates are the
/// host-supplied global frame, matching the drag delta's frame.
#[derive(Clone, Copy, Debug)]
struct ScaleTrack {
    node: u32,
    first_id: u32,
    second_id: u32,
    init_x1: i64,
    init_y1: i64,
    init_x2: i64,
    init_y2: i64,
    cur_x1: i64,
    cur_y1: i64,
    cur_x2: i64,
    cur_y2: i64,
}

impl ScaleTrack {
    fn new(node: u32, first: &Down, second: &PointerEvent) -> ScaleTrack {
        ScaleTrack {
            node,
            first_id: first.pointer_id,
            second_id: second.pointer_id,
            init_x1: first.global_x,
            init_y1: first.global_y,
            init_x2: second.global_x,
            init_y2: second.global_y,
            cur_x1: first.global_x,
            cur_y1: first.global_y,
            cur_x2: second.global_x,
            cur_y2: second.global_y,
        }
    }

    fn has(&self, pointer_id: u32) -> bool {
        pointer_id == self.first_id || pointer_id == self.second_id
    }

    fn update(&mut self, event: &PointerEvent) -> bool {
        if event.pointer_id == self.first_id {
            self.cur_x1 = event.global_x;
            self.cur_y1 = event.global_y;
            true
        } else if event.pointer_id == self.second_id {
            self.cur_x2 = event.global_x;
            self.cur_y2 = event.global_y;
            true
        } else {
            false
        }
    }

    fn distance_sq(&self, current: bool) -> i128 {
        let (x1, y1, x2, y2) = if current {
            (self.cur_x1, self.cur_y1, self.cur_x2, self.cur_y2)
        } else {
            (self.init_x1, self.init_y1, self.init_x2, self.init_y2)
        };
        distance_sq(x1, y1, x2, y2)
    }

    /// Flutter's span is the mean deviation from the focal point; for two
    /// pointers that is half their separation (`scale.dart:641`, `_update`).
    fn span(&self, current: bool) -> i64 {
        (isqrt(self.distance_sq(current)) / 2) as i64
    }

    fn focal(&self) -> (i64, i64) {
        (
            self.cur_x1.saturating_add(self.cur_x2) / 2,
            self.cur_y1.saturating_add(self.cur_y2) / 2,
        )
    }

    /// `currentSpan / initialSpan` in 1024 fixed point (`scale.dart:455,463`).
    fn scale(&self) -> i64 {
        let initial = self.span(false);
        if initial <= 0 {
            return SCALE_UNIT;
        }
        (self.span(true) as i128 * SCALE_UNIT as i128 / initial as i128) as i64
    }

    /// `scale.dart:744-748`: span slop, focal slop, or a span ratio beyond
    /// 1.05 accepts the recognizer. The ratio is expressed in fixed point so
    /// the engine stays integer-only.
    fn exceeded(&self) -> bool {
        (self.span(true) - self.span(false)).abs() > SCALE_SLOP
            || {
                let (fx, fy) = self.focal();
                let (ix, iy) = (
                    self.init_x1.saturating_add(self.init_x2) / 2,
                    self.init_y1.saturating_add(self.init_y2) / 2,
                );
                distance_sq(fx, fy, ix, iy) > (PAN_SLOP as i128) * (PAN_SLOP as i128)
            }
            || self.scale_ratio_exceeded()
    }

    /// Flutter accepts when `max(current/initial, initial/current) > 1.05`
    /// (`scale.dart:748`). Compare with integers: `20 * span > 21 * span`
    /// is the fixed-point form of `current / initial > 21 / 20`.
    fn scale_ratio_exceeded(&self) -> bool {
        let initial = self.span(false);
        let current = self.span(true);
        initial > 0
            && current > 0
            && (current as i128 * 20 > initial as i128 * 21
                || initial as i128 * 20 > current as i128 * 21)
    }
}

/// Floor integer square root, no float; `mid <= v / mid` cannot overflow.
fn isqrt(v: i128) -> i128 {
    if v <= 0 {
        return 0;
    }
    let (mut lo, mut hi) = (1i128, v);
    while lo < hi {
        let mid = lo + (hi - lo + 1) / 2;
        if mid <= v / mid {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo
}

/// The most recent completed tap that may start a double tap.
#[derive(Clone, Copy, Debug)]
struct TapMark {
    node: u32,
    x: i64,
    y: i64,
    t: i64,
}

#[derive(Clone, Debug)]
struct NodeEntry {
    id: u32,
    hit_rect: Rect,
    parent: Option<u32>,
    gesture: Option<GestureKind>,
}

/// Pure, deterministic gesture recognizer.
#[derive(Clone, Debug, Default)]
pub struct GestureEngine {
    nodes: Vec<NodeEntry>,
    down: Option<Down>,
    /// Hit path captured at down, keyed by pointer id. Flutter reuses the
    /// down-time path for every move/up/cancel of that pointer instead of
    /// re-running hit testing (`gestures/binding.dart:407-448`).
    hit_paths: Vec<(u32, Vec<u32>)>,
    scale: Option<ScaleTrack>,
    /// Opt-in long-press drag updates; off keeps the legacy post-trigger moves
    /// silent, so existing arena fingerprints stay byte-identical.
    long_press_move_update: bool,
    phase: Phase,
    last_tap: Option<TapMark>,
    arena: Arena,
}

impl GestureEngine {
    pub fn new() -> GestureEngine {
        GestureEngine::default()
    }

    /// Opt in to post-long-press move updates. The default engine stays silent
    /// after `LongPress` so existing consumers see no new gesture stream.
    pub fn enable_long_press_move_update(&mut self) {
        self.long_press_move_update = true;
    }

    /// Registers a node in draw order (the order returned by `layout`).
    /// A node without a gesture prop competes with all four recognizers.
    pub fn add_node(&mut self, id: u32, rect: Rect) {
        self.add_node_with(id, rect, None, None);
    }

    /// Registers a node with its parent id and optional IR gesture prop.
    /// Re-registering an id is a no-op so a stale rect cannot duplicate an entry.
    pub fn add_node_with(
        &mut self,
        id: u32,
        rect: Rect,
        parent: Option<u32>,
        gesture: Option<GestureKind>,
    ) {
        if self.nodes.iter().any(|n| n.id == id) {
            return;
        }
        // Drag-only surfaces are scroll areas, not tap targets; keep their rect.
        let hit_rect = tap_hit_rect(rect, gesture == Some(GestureKind::Drag));
        self.nodes.push(NodeEntry {
            id,
            hit_rect,
            parent,
            gesture,
        });
    }

    /// Top-most registered node containing `(x, y)` (legacy single-hit API).
    pub fn hit_test(&self, x: i64, y: i64) -> Option<u32> {
        self.hit_test_path(x, y).first().copied()
    }

    /// Leaf-to-root path of registered nodes containing `(x, y)`
    /// (`hit_test.dart:30`). The leaf is first; event dispatch consumes on the
    /// first node that produces a gesture.
    pub fn hit_test_path(&self, x: i64, y: i64) -> Vec<u32> {
        let Some(leaf) = self
            .nodes
            .iter()
            .rev()
            .find(|n| hit(&n.hit_rect, x, y))
            .map(|n| n.id)
        else {
            return Vec::new();
        };
        let mut path = vec![leaf];
        let mut cursor = Some(leaf);
        while let Some(id) = cursor {
            let parent = self
                .nodes
                .iter()
                .find(|n| n.id == id)
                .and_then(|n| n.parent);
            let Some(pid) = parent else { break };
            let Some(parent_node) = self.nodes.iter().find(|n| n.id == pid) else {
                break;
            };
            if hit(&parent_node.hit_rect, x, y) {
                path.push(pid);
                cursor = Some(pid);
            } else {
                break;
            }
        }
        path
    }

    /// Dispatches one event along the hit path; the first node that yields a
    /// gesture consumes the event and stops propagation.
    pub fn dispatch(&mut self, event: &PointerEvent) -> Option<(u32, Gesture)> {
        if event.kind == PointerKind::Cancel {
            // Flutter's cancelPointer is keyed by pointer id only; a cancelled
            // pointer may no longer hit any node (`binding.dart:336-346`).
            let node = self
                .down
                .map(|d| d.node)
                .or_else(|| self.scale.map(|t| t.node))?;
            self.forget_path(event.pointer_id);
            return self.on_cancel(node, event).map(|g| (node, g));
        }
        let path = match event.kind {
            PointerKind::Down => {
                let path = self.hit_test_path(event.global_x, event.global_y);
                self.remember_path(event.pointer_id, &path);
                path
            }
            // Down-time hit tests are cached per pointer (`binding.dart:432-438`).
            PointerKind::Move | PointerKind::Up => self.path_for(event.pointer_id),
            PointerKind::Cancel => Vec::new(),
        };
        let Some(node) = path.first().copied() else {
            if event.kind == PointerKind::Up {
                self.forget_path(event.pointer_id);
            }
            return None;
        };
        let gesture = match event.kind {
            PointerKind::Down => self.on_down(node, event),
            PointerKind::Move => self.on_move(node, event),
            PointerKind::Up => self.on_up(node, event),
            PointerKind::Cancel => self.on_cancel(node, event),
        }?;
        if event.kind == PointerKind::Up {
            self.forget_path(event.pointer_id);
        }
        Some((node, gesture))
    }

    /// Stores a leaf-to-root hit path for the lifetime of one pointer.
    fn remember_path(&mut self, pointer: u32, path: &[u32]) {
        self.forget_path(pointer);
        self.hit_paths.push((pointer, path.to_vec()));
    }

    fn path_for(&self, pointer: u32) -> Vec<u32> {
        self.hit_paths
            .iter()
            .find(|(id, _)| *id == pointer)
            .map(|(_, path)| path.clone())
            .unwrap_or_default()
    }

    fn forget_path(&mut self, pointer: u32) {
        self.hit_paths.retain(|(id, _)| *id != pointer);
    }

    /// The next host tick the engine needs: the long-press deadline or the
    /// double-tap window end, whichever comes first. `None` when idle.
    pub fn next_deadline_ms(&self) -> Option<i64> {
        let mut deadline = None;
        if let Some(down) = self.down {
            if self.phase == Phase::Pending
                && self.arena.has(down.node, GestureKind::LongPress)
                && !self.arena.is_rejected(GestureKind::LongPress)
            {
                deadline = Some(down.origin_t.saturating_add(LONG_PRESS_MS));
            }
        }
        if let Some(mark) = self.last_tap {
            let tap_deadline = mark.t.saturating_add(DOUBLE_TAP_INTERVAL_MS);
            deadline = Some(deadline.map_or(tap_deadline, |d| d.min(tap_deadline)));
        }
        deadline
    }

    /// Starts a sequence. The pointer id and both coordinate frames are kept
    /// verbatim; only events from this pointer can continue the sequence.
    pub fn on_down(&mut self, node_id: u32, event: &PointerEvent) -> Option<Gesture> {
        if event.kind != PointerKind::Down {
            return None;
        }
        if let Some(first) = self.down {
            // A second finger on the same node opens a two-pointer scale
            // track; Flutter records every down in the pointer queue
            // (`scale.dart:586-592`) while the arena stays open.
            if first.pointer_id != event.pointer_id
                && first.node == node_id
                && self.scale.is_none()
                && self.arena.winner().is_none()
                && self.arena.has(node_id, GestureKind::Scale)
            {
                self.scale = Some(ScaleTrack::new(node_id, &first, event));
            }
            return None;
        }
        let entry = self.nodes.iter().find(|n| n.id == node_id)?;
        if !hit(&entry.hit_rect, event.local_x, event.local_y) {
            return None;
        }
        if let Some(mark) = self.last_tap {
            let gap = event.time_ms.saturating_sub(mark.t);
            if mark.node != node_id
                || gap > DOUBLE_TAP_INTERVAL_MS
                || gap < DOUBLE_TAP_MIN_MS
                || !within(
                    event.local_x,
                    event.local_y,
                    mark.x,
                    mark.y,
                    DOUBLE_TAP_SLOP,
                )
            {
                self.last_tap = None;
            }
        }
        let path = self.hit_test_path(event.global_x, event.global_y);
        let mut members = Vec::new();
        for id in path {
            let node = self.nodes.iter().find(|n| n.id == id)?;
            match node.gesture {
                Some(kind) => members.push(Member { node: id, kind }),
                None => {
                    for kind in GestureKind::all() {
                        members.push(Member { node: id, kind });
                    }
                }
            }
        }
        self.arena.open(members);
        self.down = Some(Down {
            node: node_id,
            pointer_id: event.pointer_id,
            local_x: event.local_x,
            local_y: event.local_y,
            global_x: event.global_x,
            global_y: event.global_y,
            t: event.time_ms,
            origin_t: event.time_ms,
        });
        self.phase = Phase::Down;
        self.phase = Phase::Pending;
        None
    }

    /// Moves the active pointer. Leaving the node rect cancels the long-press
    /// timer; re-entering restarts it at the re-entry time. Crossing the drag
    /// slop accepts drag and rejects the other members.
    pub fn on_move(&mut self, node_id: u32, event: &PointerEvent) -> Option<Gesture> {
        if event.kind != PointerKind::Move {
            return None;
        }
        if let Some(mut track) = self.scale {
            if track.update(event) {
                self.scale = Some(track);
                if self
                    .arena
                    .winner()
                    .is_some_and(|w| w.kind == GestureKind::Scale)
                    || (self.arena.winner().is_none()
                        && track.exceeded()
                        && self.arena.accept(track.node, GestureKind::Scale))
                {
                    let (focal_x, focal_y) = track.focal();
                    return Some(Gesture::Scale {
                        scale: track.scale(),
                        focal_x,
                        focal_y,
                    });
                }
                // Two pointers in one sequence form a scale candidate, not a
                // drag; leave the drag member for the arena sweep on lift.
                return None;
            }
        }
        let Some(down) = self.down else { return None };
        if down.node != node_id || down.pointer_id != event.pointer_id {
            return None;
        }
        if self.long_press_move_update
            && self.phase == Phase::Triggered
            && self
                .arena
                .winner()
                .is_some_and(|w| w.kind == GestureKind::LongPress)
        {
            // long_press.dart:783-800: every accepted move reports the current
            // position and its offset from the down origin, in both frames.
            return Some(Gesture::LongPressMoveUpdate {
                global_x: event.global_x,
                global_y: event.global_y,
                local_x: event.local_x,
                local_y: event.local_y,
                dx: event.global_x.saturating_sub(down.global_x),
                dy: event.global_y.saturating_sub(down.global_y),
                ldx: event.local_x.saturating_sub(down.local_x),
                ldy: event.local_y.saturating_sub(down.local_y),
            });
        }
        if let Some(winner) = self.arena.winner() {
            if winner.kind == GestureKind::Drag {
                return Some(Gesture::Drag {
                    dx: event.global_x.saturating_sub(down.global_x),
                    dy: event.global_y.saturating_sub(down.global_y),
                });
            }
            return None;
        }
        let rect = self.nodes.iter().find(|n| n.id == node_id)?.hit_rect;
        let inside = hit(&rect, event.local_x, event.local_y);
        if inside {
            if self.phase == Phase::Cancelled {
                self.phase = Phase::Pending;
                if let Some(d) = self.down.as_mut() {
                    d.origin_t = event.time_ms;
                }
                for kind in GestureKind::all() {
                    self.arena.unreject(node_id, kind);
                }
            }
        } else {
            self.phase = Phase::Cancelled;
            for kind in GestureKind::all() {
                self.arena.reject(node_id, kind);
            }
            return None;
        }
        let displaced = !within(
            event.local_x,
            event.local_y,
            down.local_x,
            down.local_y,
            DRAG_THRESHOLD,
        );
        if displaced {
            self.arena.reject(node_id, GestureKind::Tap);
            self.arena.reject(node_id, GestureKind::DoubleTap);
            if self.arena.accept(node_id, GestureKind::Drag) {
                return Some(Gesture::Drag {
                    dx: event.global_x.saturating_sub(down.global_x),
                    dy: event.global_y.saturating_sub(down.global_y),
                });
            }
        }
        None
    }

    /// Lifts the active pointer. A first tap holds the arena for the
    /// double-tap window; a second qualifying tap accepts double tap;
    /// otherwise the sweep accepts the first surviving member.
    pub fn on_up(&mut self, node_id: u32, event: &PointerEvent) -> Option<Gesture> {
        if event.kind != PointerKind::Up {
            return None;
        }
        if self.scale.is_some_and(|track| track.has(event.pointer_id)) {
            // Lifting either finger ends the two-pointer phase
            // (`scale.dart:600-604`); the remaining pointer keeps its own
            // single-pointer sequence.
            self.scale = None;
            if self.down.is_some_and(|d| d.pointer_id != event.pointer_id) {
                return None;
            }
        }
        let Some(down) = self.down else { return None };
        if down.node != node_id || down.pointer_id != event.pointer_id {
            return None;
        }
        if self.arena.winner().is_some() {
            self.down = None;
            self.phase = Phase::Up;
            return None;
        }
        let tap_ok = self.phase != Phase::Cancelled
            && within(
                event.local_x,
                event.local_y,
                down.local_x,
                down.local_y,
                DRAG_THRESHOLD,
            );
        let double_ok = tap_ok
            && self.last_tap.is_some_and(|mark| {
                let gap = down.t.saturating_sub(mark.t);
                gap >= DOUBLE_TAP_MIN_MS
                    && gap <= DOUBLE_TAP_INTERVAL_MS
                    && mark.node == node_id
                    && within(
                        event.local_x,
                        event.local_y,
                        mark.x,
                        mark.y,
                        DOUBLE_TAP_SLOP,
                    )
            });
        self.down = None;
        self.phase = Phase::Up;
        if double_ok {
            self.arena.accept(node_id, GestureKind::DoubleTap);
            self.last_tap = None;
            let _ = self.arena.release();
            return Some(Gesture::DoubleTap);
        }
        if tap_ok
            && self.last_tap.is_none()
            && self.arena.has(node_id, GestureKind::DoubleTap)
            && !self.arena.is_rejected(GestureKind::DoubleTap)
        {
            // Wait for the 300ms window; a tick awards the tap by sweep.
            self.last_tap = Some(TapMark {
                node: node_id,
                x: event.local_x,
                y: event.local_y,
                t: down.t,
            });
            self.arena.hold();
            return None;
        }
        self.arena.sweep().and_then(|m| gesture_of(m.kind))
    }

    /// Cancels the sequence without emitting a gesture (`long_press.dart:668`).
    pub fn on_cancel(&mut self, _node_id: u32, event: &PointerEvent) -> Option<Gesture> {
        if event.kind != PointerKind::Cancel {
            return None;
        }
        if let Some(track) = self.scale {
            if track.has(event.pointer_id) {
                let primary = self.down.is_some_and(|d| d.pointer_id == event.pointer_id);
                self.scale = None;
                if !primary {
                    return None;
                }
            }
        }
        let Some(down) = self.down else { return None };
        if down.pointer_id != event.pointer_id {
            return None;
        }
        let node = down.node;
        for kind in GestureKind::all() {
            self.arena.reject(node, kind);
        }
        self.down = None;
        self.phase = Phase::Cancelled;
        None
    }

    /// Routes an event by its `kind`, using the caller's node id.
    pub fn on_event(&mut self, node_id: u32, event: &PointerEvent) -> Option<Gesture> {
        match event.kind {
            PointerKind::Down => self.on_down(node_id, event),
            PointerKind::Move => self.on_move(node_id, event),
            PointerKind::Up => self.on_up(node_id, event),
            PointerKind::Cancel => self.on_cancel(node_id, event),
        }
    }

    /// Host-injected timer. At the long-press deadline the recognizer accepts
    /// (`long_press.dart:641`); at the double-tap deadline the held arena is
    /// released and the sweep awards the plain tap (`multitap.dart:355`).
    pub fn on_tick(&mut self, t: i64) -> Option<Gesture> {
        if let Some(down) = self.down {
            if self.phase == Phase::Pending
                && t.saturating_sub(down.origin_t) >= LONG_PRESS_MS
                && self.arena.has(down.node, GestureKind::LongPress)
                && !self.arena.is_rejected(GestureKind::LongPress)
            {
                let node = down.node;
                if self.arena.accept(node, GestureKind::LongPress) {
                    self.phase = Phase::Triggered;
                    return Some(Gesture::LongPress);
                }
            }
            return None;
        }
        if let Some(mark) = self.last_tap {
            if t.saturating_sub(mark.t) >= DOUBLE_TAP_INTERVAL_MS {
                self.last_tap = None;
                self.phase = Phase::Up;
                return self.arena.release().and_then(|m| gesture_of(m.kind));
            }
        }
        None
    }

    /// Current gesture phase.
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// Winner of the current (or just-ended) sequence.
    pub fn arena_winner(&self) -> Option<GestureKind> {
        self.arena.winner().map(|m| m.kind)
    }

    /// Whether a recognizer was ruled out in the current arena.
    pub fn arena_rejected(&self, kind: GestureKind) -> bool {
        self.arena.is_rejected(kind)
    }
}

fn gesture_of(kind: GestureKind) -> Option<Gesture> {
    match kind {
        GestureKind::Tap => Some(Gesture::Tap),
        GestureKind::DoubleTap => Some(Gesture::DoubleTap),
        GestureKind::LongPress => Some(Gesture::LongPress),
        GestureKind::Drag | GestureKind::Scale => None,
    }
}

/// Point-in-rect hit rectangle for a laid-out node. Drag-only surfaces keep
/// their visual rect; every other node gets a centred target of at least
/// [`MIN_HIT_SIZE`] per canvas-px axis. Idempotent for rects already >= minimum;
/// painting and ripple never consume the returned rect.
pub fn tap_hit_rect(rect: Rect, is_drag_only: bool) -> Rect {
    if is_drag_only {
        return rect;
    }
    let w = if rect.w > 0 {
        rect.w.max(MIN_HIT_SIZE)
    } else {
        rect.w
    };
    let h = if rect.h > 0 {
        rect.h.max(MIN_HIT_SIZE)
    } else {
        rect.h
    };
    Rect {
        x: rect.x.saturating_add((rect.w - w) / 2),
        y: rect.y.saturating_add((rect.h - h) / 2),
        w,
        h,
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

    fn engine() -> GestureEngine {
        let mut engine = GestureEngine::new();
        engine.add_node(1, rect(0, 0, 200, 200));
        engine
    }

    fn down(e: &mut GestureEngine, node: u32, x: i64, y: i64, t: i64) {
        assert_eq!(
            e.on_down(node, &PointerEvent::at(PointerKind::Down, x, y, t)),
            None
        );
    }

    fn move_to(e: &mut GestureEngine, node: u32, x: i64, y: i64, t: i64) -> Option<Gesture> {
        e.on_move(node, &PointerEvent::at(PointerKind::Move, x, y, t))
    }

    fn up(e: &mut GestureEngine, node: u32, x: i64, y: i64, t: i64) -> Option<Gesture> {
        e.on_up(node, &PointerEvent::at(PointerKind::Up, x, y, t))
    }

    #[test]
    fn tap_has_no_down_to_up_timeout() {
        let mut e = engine();
        down(&mut e, 1, 10, 10, 0);
        // 5000ms lift is still a tap: Flutter has no TAP_TIMEOUT_MS.
        assert_eq!(up(&mut e, 1, 10, 10, 5000), None);
        assert_eq!(e.on_tick(DOUBLE_TAP_INTERVAL_MS), Some(Gesture::Tap));
    }

    #[test]
    fn drag_reports_delta_from_the_down_point() {
        let mut e = engine();
        down(&mut e, 1, 100, 100, 0);
        assert_eq!(move_to(&mut e, 1, 110, 100, 10), None);
        assert_eq!(
            move_to(&mut e, 1, 140, 90, 20),
            Some(Gesture::Drag { dx: 40, dy: -10 })
        );
        assert_eq!(
            move_to(&mut e, 1, 150, 120, 30),
            Some(Gesture::Drag { dx: 50, dy: 20 })
        );
        assert_eq!(up(&mut e, 1, 150, 120, 40), None);
    }

    #[test]
    fn drag_delta_uses_the_global_frame() {
        let mut e = engine();
        let down = PointerEvent::new(PointerKind::Down, 0, 0, 30, 30, 50, 50);
        assert_eq!(e.on_down(1, &down), None);
        let move_event = PointerEvent::new(PointerKind::Move, 0, 10, 60, 35, 95, 60);
        assert_eq!(
            e.on_move(1, &move_event),
            Some(Gesture::Drag { dx: 45, dy: 10 })
        );
    }

    #[test]
    fn unknown_node_and_orphan_events_are_ignored() {
        let mut e = engine();
        assert_eq!(
            e.on_down(99, &PointerEvent::at(PointerKind::Down, 10, 10, 0)),
            None
        );
        assert_eq!(move_to(&mut e, 1, 50, 50, 10), None);
        assert_eq!(up(&mut e, 1, 50, 50, 20), None);
        down(&mut e, 1, 50, 50, 30);
        assert_eq!(
            e.on_up(2, &PointerEvent::at(PointerKind::Up, 50, 50, 40)),
            None
        );
        assert_eq!(up(&mut e, 1, 50, 50, 50), None);
        assert_eq!(e.on_tick(30 + DOUBLE_TAP_INTERVAL_MS), Some(Gesture::Tap));
    }

    #[test]
    fn extreme_coordinates_and_times_never_panic() {
        let mut e = engine();
        down(&mut e, 1, 50, 50, 0);
        assert_eq!(move_to(&mut e, 1, i64::MAX, i64::MIN, 10), None);
        let mut e = engine();
        down(&mut e, 1, 50, 50, i64::MAX);
        assert_eq!(up(&mut e, 1, 50, 50, i64::MIN), None);
        assert_eq!(e.next_deadline_ms(), Some(i64::MAX));
    }

    // ---- four requested acceptance scenarios ----

    #[test]
    fn long_press_fires_on_tick_and_cancel_restarts_the_timer() {
        let mut e = engine();
        down(&mut e, 1, 100, 100, 0);
        assert_eq!(e.phase(), Phase::Pending);
        assert_eq!(e.next_deadline_ms(), Some(LONG_PRESS_MS));
        assert_eq!(e.on_tick(LONG_PRESS_MS - 1), None);
        assert_eq!(e.on_tick(LONG_PRESS_MS), Some(Gesture::LongPress));
        assert_eq!(e.phase(), Phase::Triggered);
        assert_eq!(e.arena_winner(), Some(GestureKind::LongPress));
        assert_eq!(up(&mut e, 1, 100, 100, LONG_PRESS_MS + 1), None);

        // Moving outside the node cancels; re-entering restarts at re-entry.
        let mut e = engine();
        down(&mut e, 1, 100, 100, 0);
        assert_eq!(move_to(&mut e, 1, 300, 100, 100), None);
        assert_eq!(e.phase(), Phase::Cancelled);
        assert_eq!(e.on_tick(LONG_PRESS_MS), None);
        assert_eq!(move_to(&mut e, 1, 100, 100, 700), None);
        assert_eq!(e.phase(), Phase::Pending);
        assert_eq!(e.next_deadline_ms(), Some(700 + LONG_PRESS_MS));
        assert_eq!(e.on_tick(700 + LONG_PRESS_MS - 1), None);
        assert_eq!(e.on_tick(700 + LONG_PRESS_MS), Some(Gesture::LongPress));
    }

    #[test]
    fn double_tap_window_emits_double_or_downgrades_to_tap() {
        let mut e = engine();
        down(&mut e, 1, 20, 20, 0);
        assert_eq!(up(&mut e, 1, 20, 20, 10), None);
        assert_eq!(e.next_deadline_ms(), Some(DOUBLE_TAP_INTERVAL_MS));
        down(&mut e, 1, 21, 21, 60);
        assert_eq!(up(&mut e, 1, 21, 21, 80), Some(Gesture::DoubleTap));
        assert_eq!(e.arena_winner(), Some(GestureKind::DoubleTap));

        // No second tap: the tick releases the held arena and awards Tap.
        let mut e = engine();
        down(&mut e, 1, 20, 20, 0);
        assert_eq!(up(&mut e, 1, 20, 20, 10), None);
        assert_eq!(e.on_tick(DOUBLE_TAP_INTERVAL_MS - 1), None);
        assert_eq!(e.on_tick(DOUBLE_TAP_INTERVAL_MS), Some(Gesture::Tap));
        assert_eq!(e.arena_winner(), Some(GestureKind::Tap));

        // Second tap past the window starts a fresh series.
        let mut e = engine();
        down(&mut e, 1, 20, 20, 0);
        assert_eq!(up(&mut e, 1, 20, 20, 10), None);
        assert_eq!(e.on_tick(DOUBLE_TAP_INTERVAL_MS), Some(Gesture::Tap));
        down(&mut e, 1, 20, 20, 400);
        assert_eq!(up(&mut e, 1, 20, 20, 410), None);
        assert_eq!(e.on_tick(710), Some(Gesture::Tap));
    }

    #[test]
    fn conflicts_resolve_by_first_accept_then_sweep() {
        // Scroll + click: drag accepts past slop, tap never fires.
        let mut e = engine();
        down(&mut e, 1, 40, 40, 0);
        assert_eq!(move_to(&mut e, 1, 45, 43, 10), None);
        assert_eq!(up(&mut e, 1, 45, 43, 30), None);
        assert_eq!(e.on_tick(DOUBLE_TAP_INTERVAL_MS), Some(Gesture::Tap));
        let mut e = engine();
        down(&mut e, 1, 40, 40, 0);
        assert_eq!(
            move_to(&mut e, 1, 40, 70, 10),
            Some(Gesture::Drag { dx: 0, dy: 30 })
        );
        assert!(e.arena_rejected(GestureKind::Tap));
        assert_eq!(up(&mut e, 1, 40, 70, 20), None);

        // Long press + drag: the tick accepts first, later drag moves are inert.
        let mut e = engine();
        down(&mut e, 1, 40, 40, 0);
        assert_eq!(e.on_tick(LONG_PRESS_MS), Some(Gesture::LongPress));
        assert!(e.arena_rejected(GestureKind::Drag));
        assert_eq!(move_to(&mut e, 1, 90, 40, LONG_PRESS_MS + 1), None);

        // Single + double: the second tap accepts before the sweep.
        let mut e = engine();
        down(&mut e, 1, 20, 20, 0);
        assert_eq!(up(&mut e, 1, 20, 20, 10), None);
        down(&mut e, 1, 22, 20, 60);
        assert_eq!(up(&mut e, 1, 22, 20, 90), Some(Gesture::DoubleTap));
        assert!(e.arena_rejected(GestureKind::Tap));
    }

    #[test]
    fn hit_path_bubbles_leaf_to_root_and_consumption_stops() {
        let mut e = GestureEngine::new();
        e.add_node_with(1, rect(0, 0, 200, 200), None, Some(GestureKind::Drag));
        e.add_node_with(2, rect(20, 20, 60, 60), Some(1), Some(GestureKind::Tap));
        assert_eq!(e.hit_test(30, 30), Some(2));
        assert_eq!(e.hit_test_path(30, 30), vec![2, 1]);
        let event = PointerEvent::at(PointerKind::Down, 30, 30, 0);
        assert_eq!(e.dispatch(&event), None);
        // The leaf tap wins the sweep; the parent drag member is rejected.
        let event = PointerEvent::at(PointerKind::Up, 30, 30, 10);
        assert_eq!(e.dispatch(&event), Some((2, Gesture::Tap)));
        assert_eq!(e.arena_winner(), Some(GestureKind::Tap));
        assert!(e.arena_rejected(GestureKind::Drag));
        // A move far away finds no leaf and does not reach the parent.
        assert_eq!(
            e.dispatch(&PointerEvent::at(PointerKind::Move, 190, 190, 20)),
            None
        );
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
    fn hit_helper_handles_degenerate_and_extreme_rects() {
        assert!(hit(&rect(10, 10, 20, 20), 10, 10));
        assert!(hit(&rect(10, 10, 20, 20), 29, 29));
        assert!(!hit(&rect(10, 10, 20, 20), 30, 10));
        assert!(!hit(&rect(10, 10, 0, 0), 10, 10));
        assert!(!hit(
            &rect(i64::MIN, i64::MIN, i64::MAX, i64::MAX),
            i64::MAX,
            i64::MAX
        ));
    }

    #[test]
    fn minimum_hit_rect_expands_small_tap_targets_only() {
        let visual = rect(528, 1188, 24, 24);
        assert_eq!(tap_hit_rect(visual, false), rect(516, 1176, 48, 48));
        assert_eq!(tap_hit_rect(visual, true), visual);
        assert_eq!(
            tap_hit_rect(rect(0, 0, 200, 48), false),
            rect(0, 0, 200, 48)
        );
        let mut e = GestureEngine::new();
        e.add_node_with(7, visual, None, Some(GestureKind::Tap));
        // 20 px past the visual edge is inside the 48 px target, not the box.
        assert!(!hit(&visual, 560, 1200));
        assert_eq!(e.hit_test(540, 1200), Some(7));
        assert_eq!(e.hit_test(560, 1200), Some(7));
        assert_eq!(e.hit_test(565, 1200), None);
        assert_eq!(
            e.dispatch(&PointerEvent::at(PointerKind::Down, 560, 1200, 0)),
            None
        );
        assert_eq!(
            e.dispatch(&PointerEvent::at(PointerKind::Up, 560, 1200, 10)),
            Some((7, Gesture::Tap))
        );

        // Already >= 48 on both axes: identity, no new hits outside the rect.
        let mut big = GestureEngine::new();
        big.add_node_with(8, rect(0, 0, 200, 48), None, Some(GestureKind::Tap));
        assert_eq!(big.hit_test(199, 47), Some(8));
        assert_eq!(big.hit_test(210, 24), None);

        // A drag-only surface is a scroll area, not a tap target.
        let mut drag = GestureEngine::new();
        drag.add_node_with(9, visual, None, Some(GestureKind::Drag));
        assert_eq!(drag.hit_test(560, 1200), None);
    }

    #[test]
    fn expanded_hit_rects_keep_topmost_draw_order() {
        let mut e = GestureEngine::new();
        e.add_node_with(1, rect(10, 10, 24, 24), None, Some(GestureKind::Tap));
        e.add_node_with(2, rect(20, 20, 24, 24), Some(1), Some(GestureKind::Tap));
        // Both expanded targets cover (40,30); the later node is drawn on top.
        assert_eq!(e.hit_test(40, 30), Some(2));
        assert_eq!(e.hit_test_path(40, 30), vec![2, 1]);
        // Outside the top node's expansion the lower node still wins.
        assert_eq!(e.hit_test_path(0, 20), vec![1]);
    }
}
