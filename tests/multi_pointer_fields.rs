//! Multi-pointer field, ordering, and reset parity pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `gestures/events.dart:284-310`: an event carries view id, embedder id,
//!   timestamp, a unique per-down pointer id, device id and global position.
//! - `gestures/events.dart:328-356`: delta is zero for down/up/cancel and is
//!   carried per move event.
//! - `gestures/events.dart:370-432`: pressure, pressure range, contact area
//!   (`size`) and contact radii are independent per-pointer metrics.
//! - `gestures/binding.dart:336-346`: `cancelPointer` is keyed by pointer id
//!   and schedules a cancel before the next pointer event.
//! - `gestures/binding.dart:407-448`: down creates a path, move/up/cancel
//!   reuse or remove exactly that pointer's path.
//! - `gestures/arena.dart:117-129,157-179`: one arena per pointer; sweep
//!   accepts its first member and rejects the others.
//!
//! Opurom's ABI and IR intentionally do not carry pressure/area fields yet.
//! This suite uses a host-side event fixture to pin their per-pointer identity
//! and ordering without changing either contract.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Down,
    Move,
    Up,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PointerSample {
    phase: Phase,
    pointer_id: u32,
    time_ms: i64,
    x: i64,
    y: i64,
    pressure_milli: u32,
    size_milli: u32,
    radius_major: i64,
    radius_minor: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HostRecord {
    pointer_id: u32,
    delta: (i64, i64),
    path_reused: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HostInjector {
    paths: Vec<(u32, (i64, i64))>,
    arenas: Vec<u32>,
    records: Vec<HostRecord>,
}

impl HostInjector {
    fn new() -> Self {
        Self {
            paths: Vec::new(),
            arenas: Vec::new(),
            records: Vec::new(),
        }
    }

    fn feed(&mut self, sample: PointerSample) {
        match sample.phase {
            Phase::Down => {
                self.paths.retain(|(id, _)| *id != sample.pointer_id);
                self.paths.push((sample.pointer_id, (sample.x, sample.y)));
                if !self.arenas.contains(&sample.pointer_id) {
                    self.arenas.push(sample.pointer_id);
                }
                // Flutter reports zero delta on down (`events.dart:328-332`).
                self.records.push(HostRecord {
                    pointer_id: sample.pointer_id,
                    delta: (0, 0),
                    path_reused: false,
                });
            }
            Phase::Move => {
                let origin = self
                    .paths
                    .iter()
                    .find(|(id, _)| *id == sample.pointer_id)
                    .map(|(_, p)| *p)
                    .unwrap_or((sample.x, sample.y));
                self.records.push(HostRecord {
                    pointer_id: sample.pointer_id,
                    delta: (sample.x - origin.0, sample.y - origin.1),
                    path_reused: true,
                });
            }
            Phase::Up | Phase::Cancel => {
                self.paths.retain(|(id, _)| *id != sample.pointer_id);
                // Cancel removes the per-pointer arena; up sweeps it. Both
                // end the sequence (`binding.dart:428-431,536-543`).
                self.arenas.retain(|id| *id != sample.pointer_id);
                self.records.push(HostRecord {
                    pointer_id: sample.pointer_id,
                    delta: (0, 0),
                    path_reused: true,
                });
            }
        }
    }
}

fn sample(phase: Phase, pointer_id: u32, t: i64, x: i64, y: i64) -> PointerSample {
    PointerSample {
        phase,
        pointer_id,
        time_ms: t,
        x,
        y,
        pressure_milli: 500 + pointer_id * 10,
        size_milli: 100 + pointer_id,
        radius_major: 8 + pointer_id as i64,
        radius_minor: 5 + pointer_id as i64,
    }
}

#[test]
fn pressure_and_area_are_independent_per_pointer() {
    let first = sample(Phase::Down, 1, 0, 40, 40);
    let second = sample(Phase::Down, 2, 1, 200, 40);
    assert_eq!(first.pressure_milli, 510);
    assert_eq!(second.pressure_milli, 520);
    assert_eq!(first.size_milli, 101);
    assert_eq!(second.size_milli, 102);
    assert_eq!(first.radius_major, 9);
    assert_eq!(second.radius_major, 10);
    assert_ne!(first.pressure_milli, second.pressure_milli);
    assert_ne!(first.size_milli, second.size_milli);
}

#[test]
fn down_move_up_order_is_preserved_per_pointer() {
    let mut injector = HostInjector::new();
    let stream = [
        sample(Phase::Down, 1, 0, 40, 40),
        sample(Phase::Down, 2, 1, 200, 40),
        sample(Phase::Move, 1, 2, 50, 45),
        sample(Phase::Move, 2, 3, 190, 45),
        sample(Phase::Up, 1, 4, 50, 45),
        sample(Phase::Up, 2, 5, 190, 45),
    ];
    for event in stream {
        injector.feed(event);
    }
    assert_eq!(
        injector
            .records
            .iter()
            .map(|r| r.pointer_id)
            .collect::<Vec<_>>(),
        vec![1, 2, 1, 2, 1, 2]
    );
    assert_eq!(injector.records[0].delta, (0, 0));
    assert_eq!(injector.records[2].delta, (10, 5));
    assert_eq!(injector.records[3].delta, (-10, 5));
    assert!(injector.records[2..].iter().all(|r| r.path_reused));
    assert!(injector.arenas.is_empty());
}

#[test]
fn interleaved_moves_stay_on_each_down_path() {
    let mut injector = HostInjector::new();
    injector.feed(sample(Phase::Down, 7, 0, 20, 20));
    injector.feed(sample(Phase::Down, 8, 1, 180, 180));
    injector.feed(sample(Phase::Move, 7, 2, 30, 30));
    injector.feed(sample(Phase::Move, 8, 3, 170, 170));
    assert_eq!(injector.records[2].delta, (10, 10));
    assert_eq!(injector.records[3].delta, (-10, -10));
}

#[test]
fn cancel_resets_only_its_pointer_and_makes_later_moves_inert() {
    let mut injector = HostInjector::new();
    injector.feed(sample(Phase::Down, 3, 0, 40, 40));
    injector.feed(sample(Phase::Down, 4, 1, 200, 40));
    injector.feed(sample(Phase::Cancel, 3, 2, 0, 0));
    assert_eq!(injector.arenas, vec![4]);
    assert!(injector.paths.iter().all(|(id, _)| *id != 3));
    // A late move for a cancelled pointer creates a reuse record with a zero
    // base but cannot reopen its arena.
    injector.feed(sample(Phase::Move, 3, 3, 50, 50));
    assert_eq!(injector.arenas, vec![4]);
}

#[test]
fn multi_pointer_event_stream_is_byte_identical_on_replay() {
    let run = || {
        let mut injector = HostInjector::new();
        for s in [
            sample(Phase::Down, 1, 0, 40, 40),
            sample(Phase::Down, 2, 1, 240, 40),
            sample(Phase::Move, 1, 2, 60, 60),
            sample(Phase::Move, 2, 3, 220, 60),
            sample(Phase::Up, 2, 4, 220, 60),
            sample(Phase::Up, 1, 5, 60, 60),
        ] {
            injector.feed(s);
        }
        injector.records.clone()
    };
    assert_eq!(run(), run());
}
