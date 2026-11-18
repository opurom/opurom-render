//! Gesture recognizer registration parity pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors (blob `b1793a60691504f1555dbad45665fabf489418a6`):
//! - `widgets/gesture_detector.dart:1536-1556` (`_syncAll`): a recognizer
//!   instance is reused when its type remains registered, a newly requested
//!   type is constructed, and a removed type is disposed.
//! - `widgets/gesture_detector.dart:1558-1563`: the down event is added to
//!   every registered recognizer, in insertion order.
//! - `widgets/gesture_detector.dart:1711-1719`: the semantics handlers are
//!   derived from the same recognizer map.
//!
//! These tests are a deterministic host projection. Opurom's ABI and IR do
//! not carry recognizer objects, so none is added here.

use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum RecognizerType {
    Tap,
    LongPress,
    VerticalDrag,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Factory {
    created: u32,
    initialized: u32,
}

impl Factory {
    fn new() -> Self {
        Self {
            created: 0,
            initialized: 0,
        }
    }

    fn construct(&mut self, id: u32) -> Recognizer {
        self.created += 1;
        Recognizer { id, initialized: 0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Recognizer {
    id: u32,
    initialized: u32,
}

#[derive(Debug, Default)]
struct Detector {
    recognizers: BTreeMap<RecognizerType, Recognizer>,
    factories: BTreeMap<RecognizerType, Factory>,
    next_id: u32,
    disposed: Vec<u32>,
    down_order: Vec<u32>,
}

impl Detector {
    /// `gesture_detector.dart:1536-1556`.
    fn sync(&mut self, requested: &[RecognizerType]) {
        let old = std::mem::take(&mut self.recognizers);
        for kind in requested {
            let entry = self.factories.entry(*kind).or_default();
            let mut recognizer = old.get(kind).copied().unwrap_or_else(|| {
                self.next_id += 1;
                entry.construct(self.next_id)
            });
            recognizer.initialized += 1;
            entry.initialized += 1;
            self.recognizers.insert(*kind, recognizer);
        }
        for (kind, recognizer) in &old {
            if !self.recognizers.contains_key(kind) {
                self.disposed.push(recognizer.id);
            }
        }
    }

    /// `gesture_detector.dart:1558-1563`: every registered recognizer sees
    /// the down, in stable key order.
    fn pointer_down(&mut self) {
        self.down_order = self
            .recognizers
            .values()
            .map(|recognizer| recognizer.id)
            .collect();
    }
}

#[test]
fn a_new_registration_constructs_and_initializes_once() {
    let mut detector = Detector::default();
    detector.sync(&[RecognizerType::Tap]);
    assert_eq!(detector.factories[&RecognizerType::Tap].created, 1);
    assert_eq!(detector.factories[&RecognizerType::Tap].initialized, 1);
    assert_eq!(detector.recognizers[&RecognizerType::Tap].initialized, 1);
}

#[test]
fn an_unchanged_registration_reuses_the_instance() {
    let mut detector = Detector::default();
    detector.sync(&[RecognizerType::Tap, RecognizerType::LongPress]);
    let first = detector.recognizers[&RecognizerType::Tap];
    detector.sync(&[RecognizerType::Tap, RecognizerType::LongPress]);
    assert_eq!(detector.recognizers[&RecognizerType::Tap].id, first.id);
    assert_eq!(detector.factories[&RecognizerType::Tap].created, 1);
    assert_eq!(detector.factories[&RecognizerType::Tap].initialized, 2);
}

#[test]
fn a_removed_registration_disposes_only_its_own_instance() {
    let mut detector = Detector::default();
    detector.sync(&[
        RecognizerType::Tap,
        RecognizerType::LongPress,
        RecognizerType::VerticalDrag,
    ]);
    let tap = detector.recognizers[&RecognizerType::Tap];
    let long_press = detector.recognizers[&RecognizerType::LongPress];
    detector.sync(&[RecognizerType::Tap, RecognizerType::VerticalDrag]);
    assert_eq!(detector.disposed, vec![long_press.id]);
    assert!(!detector.disposed.contains(&tap.id));
    assert!(!detector
        .recognizers
        .contains_key(&RecognizerType::LongPress));
}

#[test]
fn down_events_go_to_every_registered_recognizer() {
    let mut detector = Detector::default();
    detector.sync(&[
        RecognizerType::Tap,
        RecognizerType::LongPress,
        RecognizerType::VerticalDrag,
    ]);
    detector.pointer_down();
    let expected: Vec<u32> = detector.recognizers.values().map(|r| r.id).collect();
    assert_eq!(detector.down_order, expected);
    assert_eq!(detector.down_order.len(), 3);
}

#[test]
fn registration_sync_is_deterministic() {
    let run = || {
        let mut detector = Detector::default();
        detector.sync(&[RecognizerType::Tap, RecognizerType::LongPress]);
        detector.sync(&[RecognizerType::Tap]);
        let result = (detector.recognizers.clone(), detector.disposed.clone());
        result
    };
    assert_eq!(run(), run());
}
