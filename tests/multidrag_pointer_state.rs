//! MultiDragPointerState parity pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors (blob `5dcd04bd32c848052a3e7058578796509e606945`):
//! - `gestures/multidrag.dart:34-43`: one state object per pointer id.
//! - `gestures/multidrag.dart:89-116`: a move before acceptance accumulates
//!   into `pendingDelta`; after acceptance every delta goes to the client.
//! - `gestures/multidrag.dart:139-153`: acceptance replays the accumulated
//!   pending delta as the first client update.
//! - `gestures/multidrag.dart:155-168`: lifting a pointer ends the client
//!   with `DragEndDetails`; lifting before acceptance drops the pending
//!   delta without a callback.
//! - `gestures/multidrag.dart:502-547`: the delayed recognizer accepts after
//!   its deadline while the pointer stays inside hit slop, and rejects when
//!   pending travel exceeds that slop.
//!
//! These tests are a deterministic host projection and add no ABI or IR
//! field.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Resolution {
    Pending,
    Accepted,
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PointerDrag {
    pointer: u32,
    resolution: Resolution,
    pending_x: i64,
    pending_y: i64,
    client_x: i64,
    client_y: i64,
    ended: bool,
}

impl PointerDrag {
    fn new(pointer: u32) -> Self {
        Self {
            pointer,
            resolution: Resolution::Pending,
            pending_x: 0,
            pending_y: 0,
            client_x: 0,
            client_y: 0,
            ended: false,
        }
    }

    /// `multidrag.dart:89-116`.
    fn on_move(&mut self, dx: i64, dy: i64) {
        match self.resolution {
            Resolution::Accepted => {
                self.client_x += dx;
                self.client_y += dy;
            }
            Resolution::Pending => {
                self.pending_x += dx;
                self.pending_y += dy;
            }
            Resolution::Rejected => {}
        }
    }

    /// `multidrag.dart:139-153`: replay the pending delta once on acceptance.
    fn accept(&mut self) {
        if self.resolution != Resolution::Pending {
            return;
        }
        self.resolution = Resolution::Accepted;
        self.client_x += self.pending_x;
        self.client_y += self.pending_y;
        self.pending_x = 0;
        self.pending_y = 0;
    }

    fn reject(&mut self) {
        if self.resolution == Resolution::Pending {
            self.resolution = Resolution::Rejected;
            self.pending_x = 0;
            self.pending_y = 0;
        }
    }

    /// `multidrag.dart:155-168`.
    fn up(&mut self) {
        match self.resolution {
            Resolution::Accepted => self.ended = true,
            Resolution::Pending | Resolution::Rejected => {
                self.pending_x = 0;
                self.pending_y = 0;
            }
        }
    }

    /// `multidrag.dart:502-547`: delayed acceptance by deadline, or rejection
    /// when pending travel exceeds hit slop (18 px at constants.dart:65).
    fn delayed_deadline(&mut self) {
        if self.resolution == Resolution::Pending {
            self.accept();
        }
    }

    fn delayed_move(&mut self, dx: i64, dy: i64, inside_slop: bool) {
        self.on_move(dx, dy);
        if self.resolution == Resolution::Pending && !inside_slop {
            self.reject();
        }
    }
}

#[test]
fn each_pointer_owns_its_own_state() {
    let mut first = PointerDrag::new(1);
    let mut second = PointerDrag::new(2);
    first.on_move(10, 0);
    second.on_move(0, 20);
    assert_eq!((first.pending_x, first.pending_y), (10, 0));
    assert_eq!((second.pending_x, second.pending_y), (0, 20));
}

#[test]
fn acceptance_replays_the_pending_delta_once() {
    let mut drag = PointerDrag::new(1);
    drag.on_move(5, 7);
    drag.accept();
    assert_eq!((drag.client_x, drag.client_y), (5, 7));
    assert_eq!((drag.pending_x, drag.pending_y), (0, 0));
    drag.accept();
    assert_eq!((drag.client_x, drag.client_y), (5, 7));
}

#[test]
fn moves_after_acceptance_go_to_the_client() {
    let mut drag = PointerDrag::new(1);
    drag.accept();
    drag.on_move(8, 9);
    assert_eq!((drag.client_x, drag.client_y), (8, 9));
    assert_eq!((drag.pending_x, drag.pending_y), (0, 0));
}

#[test]
fn lifting_after_acceptance_ends_the_client() {
    let mut drag = PointerDrag::new(1);
    drag.accept();
    drag.on_move(1, 2);
    drag.up();
    assert!(drag.ended);
}

#[test]
fn lifting_before_acceptance_drops_the_pending_delta() {
    let mut drag = PointerDrag::new(1);
    drag.on_move(10, 10);
    drag.up();
    assert!(!drag.ended);
    assert_eq!((drag.pending_x, drag.pending_y), (0, 0));
}

#[test]
fn a_delayed_pointer_must_stay_within_hit_slop() {
    let mut drag = PointerDrag::new(1);
    drag.delayed_move(5, 5, true);
    drag.delayed_deadline();
    assert_eq!(drag.resolution, Resolution::Accepted);
    let mut rejected = PointerDrag::new(2);
    rejected.delayed_move(30, 0, false);
    assert_eq!(rejected.resolution, Resolution::Rejected);
    rejected.delayed_deadline();
    assert_eq!(rejected.resolution, Resolution::Rejected);
}

#[test]
fn multidrag_projection_is_deterministic() {
    let run = || {
        let mut drag = PointerDrag::new(1);
        drag.on_move(10, 20);
        drag.accept();
        drag.on_move(1, 2);
        drag.up();
        (drag.resolution, drag.client_x, drag.client_y, drag.ended)
    };
    assert_eq!(run(), run());
}
