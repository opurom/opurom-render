//! Gesture-arena team conflict parity pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `gestures/team.dart:17-27,29-52,54-93`: a team appears in the outer
//!   arena as one combining member; when the outer arena accepts the team,
//!   the team's captain wins, otherwise the first member that claimed victory
//!   (or the first added member) wins, and every other member is rejected.
//! - `gestures/team.dart:96-112,139-162`: without a captain the first added
//!   member wins once other competitors leave; with a captain the captain
//!   wins as soon as any team member claims victory or competitors leave.
//! - `gestures/arena.dart:65-69,232-240,254-292`: an accept while the arena
//!   is open records an eager winner; a closed accept resolves immediately;
//!   exactly one accept or reject callback is delivered per member.
//! - `gestures/arena.dart:144-179`: sweep gives the win to the first
//!   surviving entry and rejects the rest.
//!
//! Opurom's renderer does not expose runtime recognizer objects or a team
//! builder, and this suite deliberately does not add an ABI or IR field. It
//! pins a pure host-side projection of the Flutter rules so any host-side
//! coordinator can be verified byte-for-byte without a widget tree.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Tap,
    Drag,
    Scale,
    LongPress,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Disposition {
    Accepted,
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Callback {
    Accept(Kind),
    Reject(Kind),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Team {
    captain: Option<Kind>,
    members: Vec<Kind>,
    pending_winner: Option<Kind>,
    resolved: bool,
}

impl Team {
    fn new(captain: Option<Kind>, members: &[Kind]) -> Self {
        Self {
            captain,
            members: members.to_vec(),
            pending_winner: None,
            resolved: false,
        }
    }

    /// `team.dart:78-93`: an internal accept selects the captain or that
    /// member; a reject removes the member and resolves the outer entry only
    /// when the team is empty.
    fn resolve_member(&mut self, member: Kind, disposition: Disposition) -> Option<Disposition> {
        if self.resolved {
            return None;
        }
        match disposition {
            Disposition::Accepted => {
                self.pending_winner = Some(self.captain.unwrap_or(member));
                Some(Disposition::Accepted)
            }
            Disposition::Rejected => {
                self.members.retain(|m| *m != member);
                if self.members.is_empty() {
                    Some(Disposition::Rejected)
                } else {
                    None
                }
            }
        }
    }

    /// `team.dart:41-52`: the outer arena accepted the team as one entry.
    fn accept_outer(&mut self, callbacks: &mut Vec<Callback>) {
        if self.resolved {
            return;
        }
        self.resolved = true;
        let winner = self
            .pending_winner
            .or(self.captain)
            .or_else(|| self.members.first().copied());
        if let Some(winner) = winner {
            for member in &self.members {
                if *member != winner {
                    callbacks.push(Callback::Reject(*member));
                }
            }
            callbacks.push(Callback::Accept(winner));
        }
    }

    fn reject_outer(&mut self, callbacks: &mut Vec<Callback>) {
        if self.resolved {
            return;
        }
        self.resolved = true;
        for member in &self.members {
            callbacks.push(Callback::Reject(*member));
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Entry {
    Team(Team),
    Solo(Kind),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OuterArena {
    open: bool,
    entries: Vec<Entry>,
    eager_team: bool,
    callbacks: Vec<Callback>,
}

impl OuterArena {
    fn new(entries: Vec<Entry>) -> Self {
        Self {
            open: true,
            entries,
            eager_team: false,
            callbacks: Vec::new(),
        }
    }

    fn close(&mut self) {
        self.open = false;
        if self.eager_team {
            self.resolve_team_in_favor();
        }
    }

    fn resolve_team_member(&mut self, member: Kind, disposition: Disposition) {
        let Some(Entry::Team(team)) = self
            .entries
            .iter_mut()
            .find(|entry| matches!(entry, Entry::Team(_)))
        else {
            return;
        };
        if let Some(result) = team.resolve_member(member, disposition) {
            match result {
                Disposition::Accepted if self.open => self.eager_team = true,
                Disposition::Accepted => self.resolve_team_in_favor(),
                Disposition::Rejected if self.open => {}
                Disposition::Rejected => {
                    if team.members.is_empty() {
                        self.entries.retain(|e| !matches!(e, Entry::Team(_)));
                    }
                }
            }
        }
    }

    fn resolve_solo(&mut self, member: Kind, disposition: Disposition) {
        if self.open {
            if disposition == Disposition::Rejected {
                self.entries
                    .retain(|e| !matches!(e, Entry::Solo(k) if *k == member));
            }
            return;
        }
        match disposition {
            Disposition::Accepted => {
                self.callbacks.push(Callback::Accept(member));
                for entry in &mut self.entries {
                    match entry {
                        Entry::Solo(k) if *k != member => {
                            let callback = Callback::Reject(*k);
                            self.callbacks.push(callback);
                        }
                        Entry::Team(team) => {
                            let mut callbacks = std::mem::take(&mut self.callbacks);
                            team.reject_outer(&mut callbacks);
                            self.callbacks = callbacks;
                        }
                        _ => {}
                    }
                }
                self.entries.clear();
            }
            Disposition::Rejected => {
                self.entries
                    .retain(|e| !matches!(e, Entry::Solo(k) if *k == member));
            }
        }
    }

    fn resolve_team_in_favor(&mut self) {
        let Some(index) = self
            .entries
            .iter()
            .position(|e| matches!(e, Entry::Team(_)))
        else {
            return;
        };
        let mut entries = std::mem::take(&mut self.entries);
        let mut callbacks = std::mem::take(&mut self.callbacks);
        for (i, entry) in entries.iter_mut().enumerate() {
            match entry {
                Entry::Team(team) if i == index => team.accept_outer(&mut callbacks),
                Entry::Team(team) => team.reject_outer(&mut callbacks),
                Entry::Solo(kind) => callbacks.push(Callback::Reject(*kind)),
            }
        }
        self.entries = entries;
        self.callbacks = callbacks;
    }

    fn sweep(&mut self) {
        if self.open {
            return;
        }
        let Some(index) = self.entries.iter().position(|_| true) else {
            return;
        };
        let mut entries = std::mem::take(&mut self.entries);
        let mut callbacks = std::mem::take(&mut self.callbacks);
        for (i, entry) in entries.iter_mut().enumerate() {
            match entry {
                Entry::Team(team) if i == index => team.accept_outer(&mut callbacks),
                Entry::Team(team) => team.reject_outer(&mut callbacks),
                Entry::Solo(kind) if i == index => callbacks.push(Callback::Accept(*kind)),
                Entry::Solo(kind) => callbacks.push(Callback::Reject(*kind)),
            }
        }
        self.entries = entries;
        self.callbacks = callbacks;
    }
}

#[test]
fn team_without_captain_uses_first_added_member_on_sweep() {
    let mut arena = OuterArena::new(vec![
        Entry::Team(Team::new(None, &[Kind::Tap, Kind::Drag])),
        Entry::Solo(Kind::Scale),
    ]);
    arena.close();
    arena.resolve_solo(Kind::Scale, Disposition::Rejected);
    arena.sweep();
    assert_eq!(
        arena.callbacks,
        vec![Callback::Reject(Kind::Drag), Callback::Accept(Kind::Tap),]
    );
}

#[test]
fn captain_wins_when_any_team_member_accepts() {
    let mut arena = OuterArena::new(vec![Entry::Team(Team::new(
        Some(Kind::Drag),
        &[Kind::Tap, Kind::Drag, Kind::Scale],
    ))]);
    arena.close();
    arena.resolve_team_member(Kind::Tap, Disposition::Accepted);
    arena.sweep();
    assert!(arena.callbacks.contains(&Callback::Accept(Kind::Drag)));
    assert!(arena.callbacks.contains(&Callback::Reject(Kind::Tap)));
    assert!(arena.callbacks.contains(&Callback::Reject(Kind::Scale)));
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
fn solo_recognizer_can_beat_the_whole_team() {
    let mut arena = OuterArena::new(vec![
        Entry::Team(Team::new(None, &[Kind::Drag, Kind::Scale])),
        Entry::Solo(Kind::Tap),
    ]);
    arena.close();
    arena.resolve_solo(Kind::Tap, Disposition::Accepted);
    assert_eq!(
        arena.callbacks,
        vec![
            Callback::Accept(Kind::Tap),
            Callback::Reject(Kind::Drag),
            Callback::Reject(Kind::Scale),
        ]
    );
}

#[test]
fn every_member_receives_exactly_one_callback() {
    let mut arena = OuterArena::new(vec![Entry::Team(Team::new(
        None,
        &[Kind::Tap, Kind::Drag, Kind::Scale, Kind::LongPress],
    ))]);
    arena.close();
    arena.sweep();
    let mut seen = std::collections::BTreeSet::new();
    for callback in &arena.callbacks {
        let kind = match callback {
            Callback::Accept(k) | Callback::Reject(k) => *k,
        };
        assert!(seen.insert(kind), "duplicate callback for {kind:?}");
    }
    assert_eq!(seen.len(), 4);
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
fn conflict_projection_is_deterministic() {
    let run = || {
        let mut arena = OuterArena::new(vec![
            Entry::Team(Team::new(Some(Kind::Drag), &[Kind::Tap, Kind::Drag])),
            Entry::Solo(Kind::Scale),
        ]);
        arena.resolve_team_member(Kind::Tap, Disposition::Accepted);
        arena.close();
        arena.sweep();
        arena.callbacks.clone()
    };
    assert_eq!(run(), run());
}
