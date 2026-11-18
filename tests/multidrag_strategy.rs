//! Multi-touch drag strategy parity pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors (blob `f3f88766dfda22c6138eece1df4fdd1d6f3a3aeb` for
//! `recognizer.dart`, `6acbf98b5158d8b4d8bb200d6133ce26fda253be` for
//! `monodrag.dart`):
//! - `gestures/recognizer.dart:58-109`: `MultitouchDragStrategy` has
//!   `latestPointer`, `averageBoundaryPointers` and `sumAllPointers`; the
//!   boundary strategy reports the maximum positive and minimum negative
//!   delta, and pan averages all pointers.
//! - `gestures/monodrag.dart:450-460`: `latestPointer` tracks only the
//!   active pointer; the other two track every move.
//! - `gestures/monodrag.dart:729-745`: the last accepted pointer becomes
//!   active; when it lifts the first remaining accepted one continues.
//!
//! These tests are a deterministic host projection. Opurom's ABI and IR do
//! not carry a strategy field, so none is added here.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MultitouchDragStrategy {
    LatestPointer,
    AverageBoundaryPointers,
    SumAllPointers,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Move {
    pointer: u32,
    delta: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Drag {
    active: Option<u32>,
    accepted: [bool; 8],
}

impl Drag {
    fn new() -> Self {
        Self {
            active: None,
            accepted: [false; 8],
        }
    }

    fn accept(&mut self, pointer: u32) {
        let index = pointer as usize;
        if index < self.accepted.len() {
            self.accepted[index] = true;
            self.active = Some(pointer);
        }
    }

    fn give_up(&mut self, pointer: u32) {
        let index = pointer as usize;
        if index < self.accepted.len() {
            self.accepted[index] = false;
        }
        if self.active == Some(pointer) {
            // monodrag.dart:729-735: fall back to the first remaining
            // accepted pointer in ascending id order.
            self.active = (0..self.accepted.len())
                .find(|index| self.accepted[*index])
                .map(|index| index as u32);
        }
    }

    fn should_track(&self, strategy: MultitouchDragStrategy, pointer: u32) -> bool {
        match strategy {
            MultitouchDragStrategy::LatestPointer => {
                self.active.is_none() || self.active == Some(pointer)
            }
            MultitouchDragStrategy::AverageBoundaryPointers
            | MultitouchDragStrategy::SumAllPointers => true,
        }
    }

    /// The reported delta for one frame of moves.
    fn delta(&self, strategy: MultitouchDragStrategy, moves: &[Move]) -> i64 {
        let tracked: Vec<i64> = moves
            .iter()
            .filter(|sample| self.should_track(strategy, sample.pointer))
            .map(|sample| sample.delta)
            .collect();
        match strategy {
            MultitouchDragStrategy::LatestPointer => tracked.last().copied().unwrap_or(0),
            MultitouchDragStrategy::SumAllPointers => tracked.iter().sum(),
            MultitouchDragStrategy::AverageBoundaryPointers => {
                if tracked.is_empty() {
                    return 0;
                }
                // recognizer.dart:76-91: the maximum positive and minimum
                // negative deltas; a pan averages all active pointers.
                let positive = tracked.iter().copied().max().unwrap_or(0).max(0);
                let negative = tracked.iter().copied().min().unwrap_or(0).min(0);
                positive + negative
            }
        }
    }
}

#[test]
fn latest_pointer_tracks_only_the_active_pointer() {
    // monodrag.dart:450-460, recognizer.dart:64-71.
    let mut drag = Drag::new();
    drag.accept(1);
    assert_eq!(drag.active, Some(1));
    assert!(drag.should_track(MultitouchDragStrategy::LatestPointer, 1));
    assert!(!drag.should_track(MultitouchDragStrategy::LatestPointer, 2));
}

#[test]
fn lifting_the_active_pointer_falls_back_to_the_first_accepted() {
    // monodrag.dart:729-735.
    let mut drag = Drag::new();
    drag.accept(3);
    drag.accept(5);
    drag.accept(7);
    assert_eq!(drag.active, Some(7));
    drag.give_up(7);
    assert_eq!(drag.active, Some(3));
    drag.give_up(3);
    assert_eq!(drag.active, Some(5));
}

#[test]
fn boundary_strategy_combines_both_directions() {
    // recognizer.dart:81-83: [10, 20, 33, -1, -12] reports 33 + (-12) = 21.
    let drag = Drag::new();
    let moves = [
        Move {
            pointer: 1,
            delta: 10,
        },
        Move {
            pointer: 2,
            delta: 20,
        },
        Move {
            pointer: 3,
            delta: 33,
        },
        Move {
            pointer: 4,
            delta: -1,
        },
        Move {
            pointer: 5,
            delta: -12,
        },
    ];
    assert_eq!(
        drag.delta(MultitouchDragStrategy::AverageBoundaryPointers, &moves),
        21
    );
}

#[test]
fn sum_strategy_adds_every_active_pointer() {
    // recognizer.dart:101-107: [10, 20, 33] reports 63.
    let drag = Drag::new();
    let moves = [
        Move {
            pointer: 1,
            delta: 10,
        },
        Move {
            pointer: 2,
            delta: 20,
        },
        Move {
            pointer: 3,
            delta: 33,
        },
    ];
    assert_eq!(
        drag.delta(MultitouchDragStrategy::SumAllPointers, &moves),
        63
    );
}

#[test]
fn single_pointer_is_identical_for_every_strategy() {
    // recognizer.dart:58-62: there is no difference with one finger.
    let mut drag = Drag::new();
    drag.accept(1);
    let moves = [Move {
        pointer: 1,
        delta: 25,
    }];
    for strategy in [
        MultitouchDragStrategy::LatestPointer,
        MultitouchDragStrategy::AverageBoundaryPointers,
        MultitouchDragStrategy::SumAllPointers,
    ] {
        assert_eq!(drag.delta(strategy, &moves), 25);
    }
}

#[test]
fn strategy_projection_is_deterministic() {
    let run = || {
        let mut drag = Drag::new();
        drag.accept(2);
        drag.accept(9);
        let moves = [
            Move {
                pointer: 2,
                delta: 10,
            },
            Move {
                pointer: 9,
                delta: -4,
            },
        ];
        let first = drag.delta(MultitouchDragStrategy::LatestPointer, &moves);
        let second = drag.delta(MultitouchDragStrategy::AverageBoundaryPointers, &moves);
        let third = drag.delta(MultitouchDragStrategy::SumAllPointers, &moves);
        drag.give_up(9);
        (first, second, third, drag.active)
    };
    assert_eq!(run(), run());
}
