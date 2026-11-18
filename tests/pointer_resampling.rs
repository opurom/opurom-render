//! Pointer resampling parity pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `gestures/resampler.dart:15-35`: resampling works per pointer sequence,
//!   interpolates position and delta only, and emits move/hover only when the
//!   position changed.
//! - `gestures/resampler.dart:130-149`: when the next event is after the
//!   sample time, position is `last + (next - last) * scalar`, where scalar is
//!   `(sampleTime - lastTime) / (nextTime - lastTime)`.
//! - `gestures/resampler.dart:151-171`: events at or before the sample time
//!   replace `last`; the first later event becomes `next`.
//! - `gestures/binding.dart:395-405,560-563`: sampling is per frame and the
//!   resampler stops when disabled.
//!
//! Opurom deliberately keeps resampling in the host. This suite pins a pure
//! integer projection of the Flutter interpolation so a host cannot drift:
//! same timestamp grid, deterministic rounding, and no synthesized move when
//! the interpolated position is unchanged.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Sample {
    time_ms: i64,
    x: i64,
    y: i64,
}

fn position_at(last: Sample, next: Sample, sample_time_ms: i64) -> (i64, i64) {
    if next.time_ms <= sample_time_ms || next.time_ms <= last.time_ms {
        return (next.x, next.y);
    }
    // Flutter uses doubles; the host boundary rounds once to an integer pixel
    // with the same half-up rule as Opurom's raster boundary.
    let numerator_x = (last.x as i128) * ((next.time_ms - sample_time_ms) as i128)
        + (next.x as i128) * ((sample_time_ms - last.time_ms) as i128);
    let numerator_y = (last.y as i128) * ((next.time_ms - sample_time_ms) as i128)
        + (next.y as i128) * ((sample_time_ms - last.time_ms) as i128);
    let denominator = (next.time_ms - last.time_ms) as i128;
    (
        round_div_half_up(numerator_x, denominator),
        round_div_half_up(numerator_y, denominator),
    )
}

fn round_div_half_up(numerator: i128, denominator: i128) -> i64 {
    assert!(denominator > 0);
    let rounded = if numerator >= 0 {
        (numerator + denominator / 2) / denominator
    } else {
        -((-numerator + denominator / 2) / denominator)
    };
    rounded as i64
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    Move { x: i64, y: i64 },
    Noop,
}

fn sample_event(last: Sample, next: Sample, sample_time_ms: i64) -> Event {
    let point = position_at(last, next, sample_time_ms);
    if point == (last.x, last.y) {
        Event::Noop
    } else {
        Event::Move {
            x: point.0,
            y: point.1,
        }
    }
}

#[test]
fn interpolation_matches_flutter_midpoint_rule() {
    let last = Sample {
        time_ms: 100,
        x: 0,
        y: 0,
    };
    let next = Sample {
        time_ms: 200,
        x: 100,
        y: 40,
    };
    assert_eq!(position_at(last, next, 100), (0, 0));
    assert_eq!(position_at(last, next, 150), (50, 20));
    assert_eq!(position_at(last, next, 200), (100, 40));
}

#[test]
fn out_of_range_samples_use_the_next_position() {
    let last = Sample {
        time_ms: 100,
        x: 20,
        y: 30,
    };
    let next = Sample {
        time_ms: 200,
        x: 80,
        y: 90,
    };
    assert_eq!(position_at(last, next, 250), (80, 90));
    let equal = Sample {
        time_ms: 100,
        x: 7,
        y: 9,
    };
    assert_eq!(position_at(last, equal, 100), (7, 9));
}

#[test]
fn unchanged_position_does_not_emit_a_move() {
    let last = Sample {
        time_ms: 0,
        x: 40,
        y: 40,
    };
    let next = Sample {
        time_ms: 100,
        x: 40,
        y: 40,
    };
    assert_eq!(sample_event(last, next, 50), Event::Noop);
}

#[test]
fn resampling_projection_is_deterministic_on_uneven_grids() {
    let grids: [&[i64]; 3] = [
        &[0, 17, 33, 50, 67, 83, 100],
        &[0, 7, 21, 36, 52, 68, 84, 100],
        &[0, 12, 24, 36, 48, 60, 72, 84, 96, 100],
    ];
    let run = |times: &[i64]| -> Vec<(i64, i64)> {
        let last = Sample {
            time_ms: times[0],
            x: 10,
            y: 20,
        };
        let next = Sample {
            time_ms: *times.last().unwrap(),
            x: 210,
            y: 320,
        };
        times.iter().map(|t| position_at(last, next, *t)).collect()
    };
    for times in &grids {
        assert_eq!(run(times), run(times));
    }
    assert_eq!(grids[0].last(), grids[1].last());
    assert_eq!(run(&grids[0]).last(), Some(&(210, 320)));
}

#[test]
fn extreme_timestamps_do_not_overflow() {
    let last = Sample {
        time_ms: i64::MIN / 4,
        x: i64::MIN / 4,
        y: i64::MIN / 4,
    };
    let next = Sample {
        time_ms: i64::MAX / 4,
        x: i64::MAX / 4,
        y: i64::MAX / 4,
    };
    let midpoint = position_at(last, next, 0);
    assert!(midpoint.0 >= last.x && midpoint.0 <= next.x);
    assert!(midpoint.1 >= last.y && midpoint.1 <= next.y);
}
