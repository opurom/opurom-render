//! Draggable and DragTarget data-flow parity pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors (blob `7668a776611d7d921af81945a20272427bcb4cc9`):
//! - `widgets/drag_target.dart:385-398` (`Draggable.createRecognizer`): a
//!   horizontal/vertical affinity selects the corresponding multidrag
//!   recognizer; no affinity starts immediately.
//! - `widgets/drag_target.dart:502-509`: `maxSimultaneousDrags` stops new
//!   avatars when the active count reaches the limit.
//! - `widgets/drag_target.dart:755,758-780` (`didEnter`): a target accepts
//!   when no `onWillAccept` is provided or when the callback returns true;
//!   otherwise the avatar is recorded as rejected.
//! - `widgets/drag_target.dart:782-808` (`didLeave`, `didDrop`): leaving
//!   removes the avatar from both sets; a drop only calls `onAccept` when the
//!   avatar was a candidate.
//!
//! These tests are a deterministic host projection and add no ABI or IR
//! field.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Affinity {
    Horizontal,
    Vertical,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragRecognizer {
    Horizontal,
    Vertical,
    Immediate,
}

/// `drag_target.dart:385-398`.
fn recognizer_for(affinity: Affinity) -> DragRecognizer {
    match affinity {
        Affinity::Horizontal => DragRecognizer::Horizontal,
        Affinity::Vertical => DragRecognizer::Vertical,
        Affinity::None => DragRecognizer::Immediate,
    }
}

/// `drag_target.dart:502-509`.
fn may_start(active: u32, max_simultaneous: Option<u32>) -> bool {
    match max_simultaneous {
        Some(max) => active < max,
        None => true,
    }
}

/// A host projection of `DragTarget<T>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DragTarget {
    will_accept: Option<bool>,
    candidate: bool,
}

impl DragTarget {
    fn new(will_accept: Option<bool>) -> Self {
        Self {
            will_accept,
            candidate: false,
        }
    }

    /// `drag_target.dart:758-780`.
    fn did_enter(&mut self, data_present: bool) -> bool {
        let accepted = match self.will_accept {
            None => true,
            Some(value) => value && data_present,
        };
        self.candidate = accepted;
        accepted
    }

    /// `drag_target.dart:782-792`.
    fn did_leave(&mut self) {
        self.candidate = false;
    }

    /// `drag_target.dart:794-808`: only a candidate receives the data.
    fn did_drop(&mut self, data_present: bool) -> bool {
        if self.candidate {
            self.candidate = false;
            data_present
        } else {
            false
        }
    }
}

#[test]
fn affinity_selects_the_recognizer() {
    // drag_target.dart:389-397.
    assert_eq!(
        recognizer_for(Affinity::Horizontal),
        DragRecognizer::Horizontal
    );
    assert_eq!(recognizer_for(Affinity::Vertical), DragRecognizer::Vertical);
    assert_eq!(recognizer_for(Affinity::None), DragRecognizer::Immediate);
}

#[test]
fn a_limit_stops_new_drag_attempts() {
    // drag_target.dart:502-509.
    assert!(may_start(0, Some(2)));
    assert!(may_start(1, Some(2)));
    assert!(!may_start(2, Some(2)));
    assert!(may_start(2, None));
}

#[test]
fn an_unset_will_accept_accepts_every_data() {
    // drag_target.dart:761-768.
    let mut target = DragTarget::new(None);
    assert!(target.did_enter(true));
    assert!(target.did_drop(true));
}

#[test]
fn a_rejecting_will_accept_records_the_avatar_as_rejected() {
    let mut target = DragTarget::new(Some(false));
    assert!(!target.did_enter(true));
    assert!(!target.did_drop(true));
}

#[test]
fn only_a_candidate_can_accept_the_drop() {
    let mut target = DragTarget::new(Some(true));
    assert!(target.did_enter(true));
    assert!(target.did_drop(true));
    // After the drop the candidate slot is consumed.
    assert!(!target.did_drop(true));
    // A leave clears the candidate state.
    target.did_enter(true);
    target.did_leave();
    assert!(!target.did_drop(true));
}

#[test]
fn drag_target_projection_is_deterministic() {
    let run = || {
        let mut target = DragTarget::new(Some(true));
        let entered = target.did_enter(true);
        let dropped = target.did_drop(true);
        (recognizer_for(Affinity::None), entered, dropped)
    };
    assert_eq!(run(), run());
}
