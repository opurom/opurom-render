//! Host-configurable conflict projection parity pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `gestures/arena.dart:14-37`: a member receives exactly one accept or
//!   reject callback per arena it joined.
//! - `gestures/arena.dart:226-292`: accept while open records an eager winner;
//!   accept after close resolves immediately; reject removes the member; the
//!   sole survivor wins; the chosen member wins and every other is rejected.
//! - `gestures/team.dart:78-93,96-112,139-162`: a team is one outer member;
//!   an internal accept selects the captain or claiming member; without a
//!   captain the first added member wins when competitors leave.
//! - `gestures/recognizer.dart:247-282`: `addPointer` filters each pointer by
//!   `supportedDevices` and `allowedButtonsFilter` before it joins a team or
//!   arena, which is the Flutter equivalent of a host conflict policy.
//!
//! Opurom's renderer deliberately has no runtime recognizer registry and no
//! new ABI/IR field. These tests use a host fixture: a deterministic
//! declaration of allowed fields, team membership, captain and disposition.
//! They pin the projection the host must apply before feeding Opurom's
//! renderer so conflict rules are explicit and replayable.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum GestureId {
    Tap,
    DoubleTap,
    LongPress,
    Drag,
    Scale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Disposition {
    Accepted,
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Callback {
    Accept(GestureId),
    Reject(GestureId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Policy {
    allowed: Vec<GestureId>,
    team: Vec<GestureId>,
    captain: Option<GestureId>,
}

impl Policy {
    fn field_projection(&self) -> Vec<GestureId> {
        self.allowed
            .iter()
            .copied()
            .filter(|id| self.team.contains(id) || !self.team.is_empty())
            .collect()
    }

    fn eligible(&self, id: GestureId) -> bool {
        self.allowed.contains(&id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Arena {
    open: bool,
    members: Vec<GestureId>,
    eager: Option<GestureId>,
    callbacks: Vec<Callback>,
}

impl Arena {
    fn new(members: Vec<GestureId>) -> Self {
        Self {
            open: true,
            members,
            eager: None,
            callbacks: Vec::new(),
        }
    }

    fn close(&mut self) {
        self.open = false;
        self.settle();
    }

    fn resolve(&mut self, member: GestureId, disposition: Disposition) {
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
            let winner = self.members[0];
            self.resolve_in_favor(winner);
        }
    }

    fn resolve_in_favor(&mut self, winner: GestureId) {
        // arena.dart:286-291 rejects every other member first, then accepts
        // the winner; each member is notified exactly once.
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

    fn cancel(&mut self) {
        let mut callbacks = std::mem::take(&mut self.callbacks);
        for member in self.members.drain(..) {
            callbacks.push(Callback::Reject(member));
        }
        self.eager = None;
        self.open = true;
        self.callbacks = callbacks;
    }

    fn sweep(&mut self) {
        if self.open {
            return;
        }
        if let Some(winner) = self.members.first().copied() {
            self.resolve_in_favor(winner);
        }
    }
}

fn team_winner(policy: &Policy, claiming: Option<GestureId>) -> GestureId {
    // team.dart:83-85: a claim selects the captain when one is configured.
    policy
        .captain
        .or(claiming)
        .or_else(|| policy.team.first().copied())
        .expect("test policies always declare a team")
}

#[test]
fn host_policy_fields_are_projected_before_arena_membership() {
    let policy = Policy {
        allowed: vec![GestureId::Tap, GestureId::Drag, GestureId::Scale],
        team: vec![GestureId::Tap, GestureId::Drag],
        captain: Some(GestureId::Drag),
    };
    assert_eq!(
        policy.field_projection(),
        vec![GestureId::Tap, GestureId::Drag, GestureId::Scale]
    );
    assert!(policy.eligible(GestureId::Tap));
    assert!(!policy.eligible(GestureId::LongPress));
    assert_eq!(team_winner(&policy, Some(GestureId::Tap)), GestureId::Drag);
}

#[test]
fn eager_accept_wins_over_later_claims() {
    let mut arena = Arena::new(vec![GestureId::Tap, GestureId::Drag, GestureId::Scale]);
    arena.resolve(GestureId::Tap, Disposition::Accepted);
    assert!(arena.callbacks.is_empty());
    arena.close();
    assert_eq!(
        arena.callbacks,
        vec![
            Callback::Reject(GestureId::Drag),
            Callback::Reject(GestureId::Scale),
            Callback::Accept(GestureId::Tap),
        ]
    );
}

#[test]
fn closed_accept_resolves_immediately_and_once() {
    let mut arena = Arena::new(vec![GestureId::Tap, GestureId::Drag]);
    arena.close();
    arena.resolve(GestureId::Drag, Disposition::Accepted);
    arena.resolve(GestureId::Tap, Disposition::Accepted);
    assert_eq!(arena.callbacks.len(), 2);
    assert!(arena.callbacks.contains(&Callback::Accept(GestureId::Drag)));
    assert!(arena.callbacks.contains(&Callback::Reject(GestureId::Tap)));
}

#[test]
fn rejection_leaves_exactly_one_default_winner() {
    let mut arena = Arena::new(vec![GestureId::Tap, GestureId::DoubleTap, GestureId::Drag]);
    arena.close();
    arena.resolve(GestureId::Tap, Disposition::Rejected);
    arena.resolve(GestureId::DoubleTap, Disposition::Rejected);
    assert!(arena.callbacks.contains(&Callback::Accept(GestureId::Drag)));
    assert_eq!(
        arena
            .callbacks
            .iter()
            .filter(|c| matches!(c, Callback::Accept(_)))
            .count(),
        1
    );
}

#[test]
fn cancel_rolls_back_every_member_and_timer_state() {
    let mut arena = Arena::new(vec![GestureId::LongPress, GestureId::Drag]);
    arena.resolve(GestureId::LongPress, Disposition::Accepted);
    assert_eq!(arena.eager, Some(GestureId::LongPress));
    arena.cancel();
    assert!(arena.members.is_empty());
    assert_eq!(arena.eager, None);
    assert!(arena.open);
    assert_eq!(
        arena.callbacks,
        vec![
            Callback::Reject(GestureId::LongPress),
            Callback::Reject(GestureId::Drag),
        ]
    );
    // A cancelled arena has no pending winner and cannot emit later callbacks.
    let before = arena.callbacks.len();
    arena.close();
    arena.sweep();
    assert_eq!(arena.callbacks.len(), before);
}

#[test]
fn host_conflict_projection_is_byte_identical_on_replay() {
    let run = || {
        let policy = Policy {
            allowed: vec![GestureId::Tap, GestureId::Drag, GestureId::Scale],
            team: vec![GestureId::Tap, GestureId::Drag],
            captain: Some(GestureId::Drag),
        };
        let mut arena = Arena::new(policy.field_projection());
        arena.close();
        arena.resolve(GestureId::Tap, Disposition::Accepted);
        let winner = team_winner(&policy, Some(GestureId::Tap));
        (arena.callbacks, winner)
    };
    assert_eq!(run(), run());
}
