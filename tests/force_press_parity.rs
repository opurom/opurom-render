//! Force-press gesture parity pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `gestures/force_press.dart:220-233`: a pointer can only join when its
//!   device reports `pressureMax > 1.0`.
//! - `gestures/force_press.dart:235-258`: the interpolated pressure crossing
//!   `startPressure` accepts the recognizer, while pointer travel past hit
//!   slop rejects it.
//! - `gestures/force_press.dart:259-306`: `onStart`, `onPeak` and `onUpdate`
//!   fire from the interpolated pressure in `[0, 1]`.
//! - `gestures/force_press.dart:361-371`: `_inverseLerp` maps the reported
//!   pressure to `[0, 1]` and clamps out-of-range device values.
//! - `gestures/events.dart:266-279,370-432`: pressure, pressureMin and
//!   pressureMax are host-supplied event fields.
//!
//! These tests use a host-side projection only: Opurom's ABI and IR do not
//! carry pressure, so no ABI or IR field is added.

/// The pressure sample carried by a host event.
#[derive(Clone, Copy, Debug, PartialEq)]
struct PressureSample {
    pressure: i64,
    pressure_min: i64,
    pressure_max: i64,
}

impl PressureSample {
    fn new(pressure: i64, pressure_min: i64, pressure_max: i64) -> Self {
        Self {
            pressure,
            pressure_min,
            pressure_max,
        }
    }
}

/// Flutter's `_inverseLerp` (`force_press.dart:361-371`) in thousandths.
fn inverse_lerp_milli(sample: PressureSample, start_milli: i64, peak_milli: i64) -> i64 {
    let span = sample.pressure_max - sample.pressure_min;
    if span == 0 {
        return 0;
    }
    let mut value = ((sample.pressure - sample.pressure_min) as i128 * 1000 / span as i128) as i64;
    value = value.clamp(0, 1000);
    let _ = (start_milli, peak_milli);
    value
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ForcePhase {
    Ready,
    Possible,
    Started,
    Peaked,
    Rejected,
}

/// The host-side force-press state machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ForcePress {
    phase: ForcePhase,
    start_milli: i64,
    peak_milli: i64,
    travelled_sq: i128,
}

impl ForcePress {
    fn new(start_milli: i64, peak_milli: i64) -> Self {
        assert!(peak_milli > start_milli);
        Self {
            phase: ForcePhase::Possible,
            start_milli,
            peak_milli,
            travelled_sq: 0,
        }
    }

    /// `force_press.dart:220-233`: pressure-sensing devices join the arena.
    fn can_join(sample: PressureSample) -> bool {
        sample.pressure_max > 1000
    }

    /// `force_press.dart:235-258,259-306`.
    fn update(&mut self, sample: PressureSample, dx: i64, dy: i64) {
        self.travelled_sq += (dx as i128) * (dx as i128) + (dy as i128) * (dy as i128);
        let pressure = inverse_lerp_milli(sample, self.start_milli, self.peak_milli);
        match self.phase {
            ForcePhase::Possible if pressure > self.start_milli => {
                self.phase = ForcePhase::Started;
            }
            ForcePhase::Possible if self.travelled_sq > 18 * 18 => {
                self.phase = ForcePhase::Rejected;
            }
            ForcePhase::Started if pressure > self.peak_milli => {
                self.phase = ForcePhase::Peaked;
            }
            _ => {}
        }
    }
}

#[test]
fn only_pressure_sensing_devices_join_the_arena() {
    // force_press.dart:221-227: pressureMax <= 1 rejects immediately.
    assert!(!ForcePress::can_join(PressureSample::new(500, 0, 1000)));
    assert!(ForcePress::can_join(PressureSample::new(500, 0, 2000)));
}

#[test]
fn inverse_lerp_maps_and_clamps_the_pressure_range() {
    // force_press.dart:361-371.
    let sample = PressureSample::new(1500, 1000, 2000);
    assert_eq!(inverse_lerp_milli(sample, 400, 850), 500);
    assert_eq!(
        inverse_lerp_milli(PressureSample::new(3000, 1000, 2000), 400, 850),
        1000
    );
    assert_eq!(
        inverse_lerp_milli(PressureSample::new(0, 1000, 2000), 400, 850),
        0
    );
}

#[test]
fn pressure_crossing_start_accepts_and_peak_follows() {
    // force_press.dart:251-254,276-290.
    let mut recognizer = ForcePress::new(400, 850);
    recognizer.update(PressureSample::new(1300, 1000, 2000), 0, 0);
    assert_eq!(recognizer.phase, ForcePhase::Possible);
    recognizer.update(PressureSample::new(1500, 1000, 2000), 0, 0);
    assert_eq!(recognizer.phase, ForcePhase::Started);
    recognizer.update(PressureSample::new(1950, 1000, 2000), 0, 0);
    assert_eq!(recognizer.phase, ForcePhase::Peaked);
}

#[test]
fn travel_past_hit_slop_rejects_before_pressure_starts() {
    // force_press.dart:255-257: kTouchSlop is 18 logical pixels
    // (gestures/constants.dart:65).
    let mut recognizer = ForcePress::new(400, 850);
    recognizer.update(PressureSample::new(1100, 1000, 2000), 18, 0);
    assert_eq!(recognizer.phase, ForcePhase::Possible);
    recognizer.update(PressureSample::new(1100, 1000, 2000), 1, 0);
    assert_eq!(recognizer.phase, ForcePhase::Rejected);
}

#[test]
fn pressure_projection_is_deterministic() {
    let run = || {
        let mut recognizer = ForcePress::new(400, 850);
        for pressure in [1100, 1300, 1500, 1950] {
            recognizer.update(PressureSample::new(pressure, 1000, 2000), 0, 0);
        }
        recognizer
    };
    assert_eq!(run(), run());
}
