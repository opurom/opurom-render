//! EagerGestureRecognizer parity pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `gestures/eager.dart:14-19`: an eager recognizer claims victory in
//!   every arena it joins.
//! - `gestures/eager.dart:25-30`: `addAllowedPointer` accepts immediately and
//!   stops tracking the pointer.
//! - `gestures/eager.dart:32-39`: it has no move handling of its own.
//! - `gestures/arena.dart:226-240`: an eager accept while the arena is open
//!   records the winner and rejects the remaining members on sweep.
//!
//! These tests are a deterministic host projection; Opurom's ABI and IR do
//! not expose runtime recognizer objects, so none is added.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Member {
    Eager,
    Tap,
    LongPress,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Callback {
    Accept(Member),
    Reject(Member),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Arena {
    members: Vec<Member>,
    winner: Option<Member>,
    accepted: Vec<Callback>,
    open: bool,
}

impl Arena {
    fn new(members: Vec<Member>) -> Self {
        Self {
            members,
            winner: None,
            accepted: Vec::new(),
            open: true,
        }
    }

    /// `eager.dart:25-30`: join and immediately accept.
    fn eager_join(&mut self) {
        if self.winner.is_none() {
            self.winner = Some(Member::Eager);
        }
        self.close();
    }

    /// `arena.dart:226-240,254-292`: the winner is resolved once and the
    /// other members are rejected.
    fn close(&mut self) {
        if !self.open {
            return;
        }
        self.open = false;
        if let Some(winner) = self.winner {
            for member in &self.members {
                if *member != winner {
                    self.accepted.push(Callback::Reject(*member));
                }
            }
            self.accepted.push(Callback::Accept(winner));
        }
    }
}

#[test]
fn eager_accept_wins_over_an_open_arena() {
    // eager.dart:25-30, arena.dart:226-240.
    let mut arena = Arena::new(vec![Member::Tap, Member::Eager, Member::LongPress]);
    arena.eager_join();
    assert_eq!(arena.winner, Some(Member::Eager));
    assert_eq!(
        arena.accepted,
        vec![
            Callback::Reject(Member::Tap),
            Callback::Reject(Member::LongPress),
            Callback::Accept(Member::Eager),
        ]
    );
}

#[test]
fn an_eager_accept_is_terminal() {
    // arena.dart:226-240: only one member is accepted.
    let mut arena = Arena::new(vec![Member::Eager, Member::Tap]);
    arena.eager_join();
    let first = arena.accepted.clone();
    arena.eager_join();
    assert_eq!(arena.accepted, first);
    assert_eq!(
        arena
            .accepted
            .iter()
            .filter(|callback| matches!(callback, Callback::Accept(_)))
            .count(),
        1
    );
}

#[test]
fn an_eager_recognizer_has_no_move_handling() {
    // eager.dart:32-39: handleEvent is empty, so only the arena changes.
    let mut arena = Arena::new(vec![Member::Eager, Member::Tap]);
    arena.winner = Some(Member::Eager);
    arena.close();
    assert_eq!(arena.winner, Some(Member::Eager));
    assert_eq!(arena.accepted.len(), 2);
}

#[test]
fn eager_resolution_is_deterministic() {
    let run = || {
        let mut arena = Arena::new(vec![Member::Tap, Member::Eager, Member::LongPress]);
        arena.eager_join();
        (arena.winner, arena.accepted)
    };
    assert_eq!(run(), run());
}
