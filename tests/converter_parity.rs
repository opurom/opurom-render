//! Pointer data conversion parity pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `gestures/converter.dart:16-35`: a down event from a touch, stylus or
//!   inverted stylus synthesizes `kPrimaryButton` when the device reports
//!   no buttons; a mouse or trackpad keeps its own buttons.
//! - `gestures/converter.dart:62-73`: an unknown signal kind is dropped and
//!   a view without a device pixel ratio produces no events.
//! - `gestures/converter.dart:74-80,321-322`: physical positions, deltas and
//!   radii are divided by the device pixel ratio.
//!
//! These tests are a deterministic host projection. Opurom's ABI and IR do
//! not carry device kinds, so none is added here.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeviceKind {
    Mouse,
    Trackpad,
    Touch,
    Stylus,
    InvertedStylus,
    Unknown,
}

/// `converter.dart:16-35`.
fn synthesise_down_buttons(kind: DeviceKind, buttons: u32) -> u32 {
    match kind {
        DeviceKind::Mouse | DeviceKind::Trackpad => buttons,
        DeviceKind::Touch
        | DeviceKind::Stylus
        | DeviceKind::InvertedStylus
        | DeviceKind::Unknown => {
            if buttons == 0 {
                1
            } else {
                buttons
            }
        }
    }
}

/// `converter.dart:74-80,321-322` in thousandths of a logical pixel.
fn to_logical_milli(physical: i64, dpr_milli: i64) -> i64 {
    (physical * 1000) / dpr_milli
}

#[test]
fn touch_synthesizes_the_primary_button() {
    // converter.dart:26-33.
    assert_eq!(synthesise_down_buttons(DeviceKind::Touch, 0), 1);
    assert_eq!(synthesise_down_buttons(DeviceKind::Touch, 2), 2);
    assert_eq!(synthesise_down_buttons(DeviceKind::Stylus, 0), 1);
    assert_eq!(synthesise_down_buttons(DeviceKind::InvertedStylus, 0), 1);
    assert_eq!(synthesise_down_buttons(DeviceKind::Unknown, 0), 1);
}

#[test]
fn mouse_keeps_its_reported_buttons() {
    // converter.dart:23-25.
    assert_eq!(synthesise_down_buttons(DeviceKind::Mouse, 0), 0);
    assert_eq!(synthesise_down_buttons(DeviceKind::Mouse, 2), 2);
    assert_eq!(synthesise_down_buttons(DeviceKind::Trackpad, 4), 4);
}

#[test]
fn physical_coordinates_divide_by_the_device_pixel_ratio() {
    // converter.dart:74-80,321-322.
    assert_eq!(to_logical_milli(1000, 2000), 500);
    assert_eq!(to_logical_milli(1000, 1000), 1000);
    assert_eq!(to_logical_milli(3000, 3000), 1000);
}

#[test]
fn conversion_is_deterministic() {
    let run = || {
        let positions = [1000, 2400, 3000];
        let ratios = [1000, 2000, 3000];
        positions
            .iter()
            .flat_map(|position| {
                ratios
                    .iter()
                    .map(|ratio| to_logical_milli(*position, *ratio))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(run(), run());
}
