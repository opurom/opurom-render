//! Deterministic navigation stack with page transitions.
//!
//! A `Navigator` owns an ordered stack of page keys. `push`, `pop` and
//! `replace` mutate the stack and start a transition from the previous top
//! page to the new top page. Like every other clock in the renderer, the
//! transition only advances when the caller ticks it, so the same operation
//! sequence plus the same tick sequence always yields identical composed
//! frames — no wall clock, no floats, no randomness.
//!
//! The navigator is a pure state machine: it never owns pixels. The caller
//! renders each page to a `Frame` and passes the outgoing and incoming frames
//! to `compose` at the current transition progress.

use crate::anim::{Transition, TransitionKind, ONE};
use crate::Frame;

/// Maximum number of pages kept on the stack. Pushing past this is rejected so
/// memory stays bounded regardless of caller behaviour.
pub const MAX_STACK_DEPTH: usize = 32;

/// Why a navigation request was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NavError {
    /// `pop` was requested on a stack that cannot go lower than one page.
    PopUnderflow,
    /// `push` was requested past [`MAX_STACK_DEPTH`].
    StackOverflow,
    /// `replace` was requested on an empty stack.
    EmptyReplace,
    /// A page transition frame pair had mismatched sizes.
    FrameMismatch,
}

impl std::fmt::Display for NavError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            NavError::PopUnderflow => "cannot pop the root page",
            NavError::StackOverflow => "navigation stack is full",
            NavError::EmptyReplace => "cannot replace with an empty stack",
            NavError::FrameMismatch => "transition frames must share dimensions",
        })
    }
}

impl std::error::Error for NavError {}

/// The result of one navigation step for the caller that drives the transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Navigation {
    /// The transition kind to run between the old and new top pages.
    pub kind: TransitionKind,
    /// Total transition duration in milliseconds ([`crate::anim::TRANSITION_MS`]).
    pub duration_ms: i64,
}

/// Ordered page stack plus the in-flight transition, if any.
#[derive(Clone, Debug)]
pub struct Navigator {
    stack: Vec<String>,
    transition: Option<Transition>,
    default_kind: TransitionKind,
}

impl Default for Navigator {
    fn default() -> Self {
        Navigator::new(TransitionKind::Slide)
    }
}

impl Navigator {
    /// A navigator rooted at no page, using `default_kind` for transitions.
    pub fn new(default_kind: TransitionKind) -> Navigator {
        Navigator {
            stack: Vec::new(),
            transition: None,
            default_kind,
        }
    }

    /// A navigator whose stack starts with `root`.
    pub fn with_root(root: impl Into<String>, default_kind: TransitionKind) -> Navigator {
        let mut nav = Navigator::new(default_kind);
        nav.stack.push(root.into());
        nav
    }

    /// The page keys from root to the current top.
    pub fn stack(&self) -> &[String] {
        &self.stack
    }

    /// The current top page, or `None` when the stack is empty.
    pub fn top(&self) -> Option<&str> {
        self.stack.last().map(String::as_str)
    }

    /// Depth of the stack.
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// True while a transition is still in flight (progress < 1.0).
    pub fn in_flight(&self) -> bool {
        matches!(&self.transition, Some(t) if t.progress < ONE)
    }

    /// Push `page` and start a transition. The previous top is remembered only
    /// as the outgoing frame the caller renders.
    pub fn push(&mut self, page: impl Into<String>) -> Result<Navigation, NavError> {
        if self.stack.len() >= MAX_STACK_DEPTH {
            return Err(NavError::StackOverflow);
        }
        self.stack.push(page.into());
        Ok(self.start())
    }

    /// Pop the top page. The root page cannot be popped.
    pub fn pop(&mut self) -> Result<Navigation, NavError> {
        if self.stack.len() <= 1 {
            return Err(NavError::PopUnderflow);
        }
        self.stack.pop();
        Ok(self.start())
    }

    /// Replace the top page without changing depth.
    pub fn replace(&mut self, page: impl Into<String>) -> Result<Navigation, NavError> {
        if self.stack.is_empty() {
            return Err(NavError::EmptyReplace);
        }
        *self.stack.last_mut().expect("non-empty") = page.into();
        Ok(self.start())
    }

    /// Reset the stack to a single root page, cancelling any transition.
    pub fn reset(&mut self, root: impl Into<String>) {
        self.stack.clear();
        self.stack.push(root.into());
        self.transition = None;
    }

    fn start(&mut self) -> Navigation {
        self.transition = Some(Transition::new(self.default_kind));
        Navigation {
            kind: self.default_kind,
            duration_ms: crate::anim::TRANSITION_MS,
        }
    }

    /// Advance the in-flight transition by `dt_ms` and return the new progress
    /// in `0..=ONE`. A finished or absent transition returns `ONE`.
    pub fn tick(&mut self, dt_ms: i64) -> i64 {
        match &mut self.transition {
            Some(t) => t.tick(dt_ms),
            None => ONE,
        }
    }

    /// Current transition progress in `0..=ONE` (`ONE` when idle).
    pub fn progress(&self) -> i64 {
        self.transition.as_ref().map_or(ONE, |t| t.progress)
    }

    /// Compose the outgoing and incoming frames at the current progress.
    /// When no transition is in flight this returns the incoming frame as-is.
    pub fn compose(&self, from: &Frame, to: &Frame) -> Result<Frame, NavError> {
        match &self.transition {
            Some(t) => t.compose(from, to).map_err(|_| NavError::FrameMismatch),
            None => Ok(to.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::TRANSITION_MS;
    use crate::{DirtyRect, Frame};

    fn frame(width: u32, height: u32, rgb: [u8; 3]) -> Frame {
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..width * height {
            pixels.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
        Frame {
            width,
            height,
            pixels,
            dirty: vec![DirtyRect {
                x: 0,
                y: 0,
                w: width,
                h: height,
            }],
        }
    }

    #[test]
    fn push_pop_and_replace_keep_the_stack_consistent() {
        let mut nav = Navigator::with_root("home", TransitionKind::Slide);
        assert_eq!(nav.depth(), 1);
        nav.push("details").unwrap();
        nav.push("settings").unwrap();
        assert_eq!(nav.stack(), ["home", "details", "settings"]);
        assert_eq!(nav.top(), Some("settings"));
        nav.pop().unwrap();
        assert_eq!(nav.top(), Some("details"));
        nav.replace("profile").unwrap();
        assert_eq!(nav.stack(), ["home", "profile"]);
        assert_eq!(nav.depth(), 2);
    }

    #[test]
    fn pop_underflow_and_overflow_are_rejected() {
        let mut nav = Navigator::with_root("home", TransitionKind::Fade);
        assert_eq!(nav.pop(), Err(NavError::PopUnderflow));
        for i in 1..MAX_STACK_DEPTH {
            nav.push(format!("p{i}")).unwrap();
        }
        assert_eq!(nav.push("overflow"), Err(NavError::StackOverflow));
        assert_eq!(nav.depth(), MAX_STACK_DEPTH);
    }

    #[test]
    fn replace_on_empty_stack_is_rejected() {
        let mut nav = Navigator::new(TransitionKind::Scale);
        assert_eq!(nav.replace("x"), Err(NavError::EmptyReplace));
    }

    #[test]
    fn transition_advances_with_ticks_and_settles() {
        let mut nav = Navigator::with_root("home", TransitionKind::Slide);
        assert_eq!(nav.progress(), ONE);
        nav.push("details").unwrap();
        assert_eq!(nav.progress(), 0);
        assert!(nav.in_flight());
        let half = nav.tick(TRANSITION_MS / 2);
        assert!((0..ONE).contains(&half));
        nav.tick(TRANSITION_MS);
        assert_eq!(nav.progress(), ONE);
        assert!(!nav.in_flight());
    }

    #[test]
    fn compose_returns_incoming_frame_when_idle() {
        let nav = Navigator::with_root("home", TransitionKind::Slide);
        let a = frame(8, 4, [10, 20, 30]);
        let b = frame(8, 4, [200, 210, 220]);
        let composed = nav.compose(&a, &b).unwrap();
        assert_eq!(composed.pixels, b.pixels);
    }

    #[test]
    fn compose_at_full_progress_equals_incoming() {
        let mut nav = Navigator::with_root("home", TransitionKind::Fade);
        let a = frame(8, 4, [10, 20, 30]);
        let b = frame(8, 4, [200, 210, 220]);
        nav.push("next").unwrap();
        nav.tick(TRANSITION_MS);
        let composed = nav.compose(&a, &b).unwrap();
        assert_eq!(composed.pixels, b.pixels);
    }

    #[test]
    fn same_operation_and_tick_sequence_is_deterministic() {
        let run = || {
            let mut nav = Navigator::with_root("home", TransitionKind::Slide);
            nav.push("a").unwrap();
            nav.tick(100);
            nav.push("b").unwrap();
            nav.tick(40);
            (nav.stack().to_vec(), nav.progress())
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn mismatched_frames_are_rejected() {
        let mut nav = Navigator::with_root("home", TransitionKind::Scale);
        nav.push("next").unwrap();
        let a = frame(8, 4, [0, 0, 0]);
        let b = frame(4, 4, [0, 0, 0]);
        assert_eq!(nav.compose(&a, &b), Err(NavError::FrameMismatch));
    }
}
