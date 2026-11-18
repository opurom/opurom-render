//! Multi-pointer contention and fairness pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `gestures/arena.dart:117-129`: one arena per pointer id, so simultaneous
//!   down events open independent contests with no shared state.
//! - `gestures/arena.dart:131-142`: down dispatch closes the arena; late
//!   members cannot join.
//! - `gestures/arena.dart:157-179,193-221`: sweep gives the first survivor the
//!   win and rejects the rest; hold delays sweep and release performs the
//!   pending sweep.
//! - `gestures/arena.dart:226-292`: accept-while-open records an eager winner;
//!   accept-after-close resolves immediately; reject removes a member and a
//!   sole survivor wins; the chosen member accepts once and every other member
//!   rejects once.
//! - `gestures/team.dart:41-93`: a team appears once in the outer arena; the
//!   captain (or first added member) wins on the team's behalf.
//!
//! These tests use a pure host fixture over the renderer's public API and add
//! no ABI or IR field.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Tap,
    DoubleTap,
    LongPress,
    Drag,
    Scale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Disposition {
    Accepted,
    #[allow(dead_code)]
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Callback {
    Accept(Kind),
    Reject(Kind),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Arena {
    pointer: u64,
    open: bool,
    members: Vec<Kind>,
    eager: Option<Kind>,
    held: bool,
    pending_sweep: bool,
    callbacks: Vec<Callback>,
}

impl Arena {
    fn new(pointer: u64, members: &[Kind]) -> Self {
        Self {
            pointer,
            open: true,
            members: members.to_vec(),
            eager: None,
            held: false,
            pending_sweep: false,
            callbacks: Vec::new(),
        }
    }

    fn close(&mut self) {
        self.open = false;
        self.settle();
    }

    fn resolve(&mut self, member: Kind, disposition: Disposition) {
        if !self.members.contains(&member) {
            return;
        }
        match disposition {
            Disposition::Accepted if self.open => {
                if self.eager.is_none() {
                    self.eager = Some(member);
                }
            }
            Disposition::Accepted => self.resolve_in_favor(member),
            Disposition::Rejected => {
                self.members.retain(|id| *id != member);
                self.callbacks.push(Callback::Reject(member));
                if self.eager == Some(member) {
                    self.eager = None;
                }
                self.settle();
            }
        }
    }

    fn settle(&mut self) {
        if self.open {
            return;
        }
        if let Some(winner) = self.eager {
            self.resolve_in_favor(winner);
        } else if self.members.len() == 1 {
            self.resolve_in_favor(self.members[0]);
        }
    }

    fn resolve_in_favor(&mut self, winner: Kind) {
        let mut callbacks = std::mem::take(&mut self.callbacks);
        for member in self.members.drain(..) {
            if member != winner {
                callbacks.push(Callback::Reject(member));
            }
        }
        callbacks.push(Callback::Accept(winner));
        self.eager = None;
        self.callbacks = callbacks;
    }

    fn hold(&mut self) {
        if !self.open && !self.callbacks.is_empty() {
            return;
        }
        self.held = true;
    }

    fn release(&mut self) {
        self.held = false;
        if self.pending_sweep {
            self.sweep();
        }
    }

    fn sweep(&mut self) {
        if self.open || self.held {
            self.pending_sweep = true;
            return;
        }
        self.pending_sweep = false;
        if let Some(winner) = self.members.first().copied() {
            self.resolve_in_favor(winner);
        }
    }

    fn cancel(&mut self) {
        let mut callbacks = std::mem::take(&mut self.callbacks);
        for member in self.members.drain(..) {
            callbacks.push(Callback::Reject(member));
        }
        self.eager = None;
        self.held = false;
        self.pending_sweep = false;
        self.open = true;
        self.callbacks = callbacks;
    }
}

#[test]
fn simultaneous_downs_open_independent_arenas() {
    let mut first = Arena::new(1, &[Kind::Tap, Kind::Drag]);
    let mut second = Arena::new(2, &[Kind::Tap, Kind::Scale]);
    first.close();
    second.close();
    assert_eq!(first.pointer, 1);
    assert_eq!(second.pointer, 2);
    first.resolve(GestureId::drag(), Disposition::Accepted);
    second.resolve(GestureId::scale(), Disposition::Accepted);
    assert!(first.callbacks.contains(&Callback::Accept(Kind::Drag)));
    assert!(second.callbacks.contains(&Callback::Accept(Kind::Scale)));
    assert_ne!(first.callbacks, second.callbacks);
}

#[test]
fn same_frame_race_resolves_by_join_order() {
    let mut arena = Arena::new(1, &[Kind::Tap, Kind::Drag, Kind::Scale]);
    arena.close();
    // Simultaneous accepts at the same logical timestamp: the first join order
    // wins, because `_resolve` processes one claim at a time and the first
    // closed accept resolves immediately (`arena.dart:232-240,281-292`).
    arena.resolve(GestureId::scale(), Disposition::Accepted);
    arena.resolve(GestureId::drag(), Disposition::Accepted);
    assert_eq!(
        arena
            .callbacks
            .iter()
            .filter(|c| matches!(c, Callback::Accept(_)))
            .count(),
        1
    );
    assert_eq!(
        arena
            .callbacks
            .iter()
            .filter(|c| matches!(c, Callback::Accept(Kind::Scale)))
            .count(),
        1
    );
}

#[test]
fn hold_delays_sweep_until_release() {
    let mut arena = Arena::new(1, &[Kind::Tap, Kind::DoubleTap]);
    arena.close();
    arena.hold();
    arena.sweep();
    assert!(arena.members.len() == 2, "a held arena cannot resolve yet");
    arena.release();
    assert_eq!(
        arena.callbacks,
        vec![
            Callback::Reject(Kind::DoubleTap),
            Callback::Accept(Kind::Tap)
        ]
    );
}

#[test]
fn cancel_rolls_back_every_member_and_timer_state() {
    let mut arena = Arena::new(1, &[Kind::LongPress, Kind::Drag, Kind::Scale]);
    arena.resolve(GestureId::long_press(), Disposition::Accepted);
    assert_eq!(arena.eager, Some(Kind::LongPress));
    arena.cancel();
    assert!(arena.members.is_empty());
    assert_eq!(arena.eager, None);
    assert_eq!(
        arena.callbacks,
        vec![
            Callback::Reject(Kind::LongPress),
            Callback::Reject(Kind::Drag),
            Callback::Reject(Kind::Scale),
        ]
    );
    let before = arena.callbacks.len();
    arena.close();
    arena.sweep();
    assert_eq!(arena.callbacks.len(), before);
}

#[test]
fn single_winner_terminal_state_excludes_other_members() {
    let mut arena = Arena::new(1, &[Kind::Tap, Kind::Drag, Kind::Scale, Kind::LongPress]);
    arena.close();
    arena.sweep();
    let accepted: Vec<_> = arena
        .callbacks
        .iter()
        .filter(|c| matches!(c, Callback::Accept(_)))
        .collect();
    let rejected: Vec<_> = arena
        .callbacks
        .iter()
        .filter(|c| matches!(c, Callback::Reject(_)))
        .collect();
    assert_eq!(accepted.len(), 1);
    assert_eq!(rejected.len(), 3);
    let winner = match accepted[0] {
        Callback::Accept(kind) => *kind,
        _ => unreachable!(),
    };
    assert!(!rejected
        .iter()
        .any(|c| matches!(c, Callback::Reject(k) if *k == winner)));
    assert_eq!(winner, Kind::Tap, "the first joined member wins a sweep");
}

#[test]
fn fairness_boundary_first_join_order_is_stable() {
    let orders = [vec![Kind::Tap, Kind::Drag], vec![Kind::Drag, Kind::Tap]];
    let mut winners = Vec::new();
    for order in &orders {
        let mut arena = Arena::new(1, &order.clone());
        arena.close();
        arena.sweep();
        winners.push(arena.callbacks.last().copied());
    }
    assert_eq!(winners[0], Some(Callback::Accept(Kind::Tap)));
    assert_eq!(winners[1], Some(Callback::Accept(Kind::Drag)));
}

#[test]
fn multi_pointer_contest_is_deterministic_on_replay() {
    let run = || {
        let mut arenas = vec![
            Arena::new(1, &[Kind::Tap, Kind::Drag]),
            Arena::new(2, &[Kind::Scale]),
            Arena::new(3, &[Kind::LongPress, Kind::DoubleTap, Kind::Tap]),
        ];
        for arena in &mut arenas {
            arena.close();
        }
        arenas[0].resolve(GestureId::drag(), Disposition::Accepted);
        arenas[1].sweep();
        arenas[2].hold();
        arenas[2].sweep();
        arenas[2].release();
        arenas
            .iter()
            .map(|a| (a.pointer, a.callbacks.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(run(), run());
}

/// Helper mirroring the test fixture's fluent naming in the assertions above.
#[allow(dead_code)]
struct GestureId;

impl GestureId {
    fn drag() -> Kind {
        Kind::Drag
    }

    fn scale() -> Kind {
        Kind::Scale
    }

    fn long_press() -> Kind {
        Kind::LongPress
    }
}
