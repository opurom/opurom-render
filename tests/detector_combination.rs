//! GestureDetector callback composition parity pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors (blob `b1793a60691504f1555dbad45665fabf489418a6`):
//! - `widgets/gesture_detector.dart:1060-1085`: a tap recognizer is
//!   registered only when a tap callback is supplied.
//! - `widgets/gesture_detector.dart:1087-1100`: a double-tap recognizer is
//!   registered only when a double-tap callback is supplied.
//! - `widgets/gesture_detector.dart:1102-1126`: a long-press recognizer is
//!   registered only when a long-press callback is supplied.
//! - `widgets/gesture_detector.dart:1558-1563`: each registered recognizer
//!   receives the same down event and competes in its own arena.
//!
//! These tests pin which recognizers become arena members for each callback
//! combination. They use a host projection and add no ABI or IR field.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Recognizer {
    Tap,
    DoubleTap,
    LongPress,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Callbacks {
    tap: bool,
    double_tap: bool,
    long_press: bool,
}

impl Callbacks {
    /// `gesture_detector.dart:1060-1126`: a recognizer joins only when its
    /// callback family is present.
    fn recognizers(&self) -> Vec<Recognizer> {
        let mut result = Vec::new();
        if self.tap {
            result.push(Recognizer::Tap);
        }
        if self.double_tap {
            result.push(Recognizer::DoubleTap);
        }
        if self.long_press {
            result.push(Recognizer::LongPress);
        }
        result
    }
}

#[test]
fn no_callback_registers_no_recognizer() {
    let callbacks = Callbacks::default();
    assert!(callbacks.recognizers().is_empty());
}

#[test]
fn tap_callback_registers_only_the_tap_recognizer() {
    let callbacks = Callbacks {
        tap: true,
        ..Callbacks::default()
    };
    assert_eq!(callbacks.recognizers(), vec![Recognizer::Tap]);
}

#[test]
fn all_three_callbacks_register_three_competing_recognizers() {
    let callbacks = Callbacks {
        tap: true,
        double_tap: true,
        long_press: true,
    };
    assert_eq!(
        callbacks.recognizers(),
        vec![
            Recognizer::Tap,
            Recognizer::DoubleTap,
            Recognizer::LongPress
        ]
    );
}

#[test]
fn a_double_tap_callback_alone_does_not_register_a_tap_recognizer() {
    let callbacks = Callbacks {
        double_tap: true,
        ..Callbacks::default()
    };
    assert_eq!(callbacks.recognizers(), vec![Recognizer::DoubleTap]);
}

#[test]
fn composition_order_is_stable() {
    let run = || {
        let callbacks = Callbacks {
            tap: true,
            double_tap: true,
            long_press: true,
        };
        callbacks.recognizers()
    };
    assert_eq!(run(), run());
}
