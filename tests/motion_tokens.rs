//! Motion token curves from DEFAULT_UI_GLASS_REFINEMENT §7, opt-in via the
//! `animate` prop. Names and fixed durations map together; every evaluation is
//! integer-only and deterministic. Default path keeps all pinned goldens.

use opurom_render::{
    animate_progress, default_duration_ms, digest_hex, layout, parse_ir, Curve, Renderer,
    StateView, ONE,
};
use serde_json::json;
use std::fs;

const COUNTER_PATH: &str = "../opurom-examples/golden/counter.opuir.json";

const TOKENS: [Curve; 4] = [Curve::Fast, Curve::Medium, Curve::Large, Curve::Color];
const DIGESTS: [&str; 4] = [
    "abbff18f8858b61d",
    "a4dbb44443d11cff",
    "5ea51971efe8a6d2",
    "97fa17033da9a12b",
];
const DURATIONS: [i64; 4] = [120, 220, 280, 180];
const NAMES: [&str; 4] = ["fast", "medium", "large", "color"];

fn samples(curve: Curve, duration: i64) -> Vec<i64> {
    (0..=32)
        .map(|i| animate_progress(curve, duration * i / 32, duration, 0))
        .collect()
}

#[test]
fn parse_and_name_round_trip_for_all_four_tokens() {
    for (curve, name) in TOKENS.iter().zip(NAMES) {
        assert_eq!(Curve::parse(name), Some(*curve));
        assert_eq!(curve.name(), name);
    }
    assert_eq!(Curve::parse("fast"), Some(Curve::Fast));
    assert_ne!(
        Curve::parse("Fast"),
        Some(Curve::Fast),
        "names are case-sensitive"
    );
}

#[test]
fn each_token_maps_its_name_to_the_section_seven_duration() {
    for (curve, &ms) in TOKENS.iter().zip(DURATIONS.iter()) {
        assert_eq!(default_duration_ms(*curve), ms, "{} duration", curve.name());
    }
}

#[test]
fn sampled_progress_is_byte_stable_per_curve() {
    // Byte-level pin: the 33-sample i64 sequence for each curve is fingerprinted.
    // Any drift in the fixed-point bezier evaluation breaks these.
    for (i, curve) in TOKENS.iter().enumerate() {
        let d = default_duration_ms(*curve);
        let mut bytes = Vec::new();
        for v in samples(*curve, d) {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        assert_eq!(
            digest_hex(&bytes),
            DIGESTS[i],
            "{} sample digest",
            curve.name()
        );
    }
}

#[test]
fn curves_are_monotonic_with_exact_endpoints_and_bounded_overshoot() {
    for curve in TOKENS {
        let d = default_duration_ms(curve);
        let s = samples(curve, d);
        assert_eq!(s[0], 0, "{} starts at zero", curve.name());
        assert_eq!(*s.last().unwrap(), ONE, "{} settles at ONE", curve.name());
        for pair in s.windows(2) {
            assert!(pair[1] >= pair[0], "{} never decreases", curve.name());
            assert!(pair[1] <= ONE, "{} stays within ONE", curve.name());
        }
        // Before the duration the token is still moving; after it, settled.
        // Medium's overshoot control point (y1=1560) reaches the clamped ONE
        // before half time, so only the other three are asserted mid-flight.
        if curve != Curve::Medium {
            assert!(
                s[16] > 0 && s[16] < ONE,
                "{} is mid-flight at half time",
                curve.name()
            );
        }
        assert_eq!(animate_progress(curve, d + 500, d, 0), ONE);
    }
}

#[test]
fn negative_probe_unknown_animate_value_is_rejected() {
    let doc = |v: &str| {
        json!({
            "ir": "0.1.0", "app": "Motion", "state": [],
            "tree": { "id": 0, "kind": "Text", "props": { "value": "x", "animate": v },
                "children": [], "events": {} },
            "theme": { "mode": "light", "accent": "#097FEB", "radius": 0 }
        })
        .to_string()
    };
    for name in NAMES {
        assert!(
            parse_ir(&doc(name)).is_ok(),
            "{name} must be accepted now that render handles it"
        );
    }
    assert!(
        parse_ir(&doc("fastttt")).is_err(),
        "unknown animate value still rejected"
    );
}

#[test]
fn default_path_keeps_the_two_counter_goldens() {
    // The four curves are opt-in; both canonical counter frames stay pinned.
    let text = fs::read_to_string(COUNTER_PATH).expect("counter golden must be readable");
    let ir = parse_ir(&text).unwrap();
    let state = StateView::from_ir(&ir);
    assert_eq!(
        digest_hex(&Renderer::new(320, 240).render(&ir, &state).pixels),
        "8d45f412996a99c6"
    );
    assert_eq!(
        digest_hex(&Renderer::new(800, 600).render(&ir, &state).pixels),
        "318beb1dd337bc4e"
    );
}

#[test]
fn a_token_drives_layout_animation_without_panicking() {
    let doc = json!({
        "ir": "0.1.0", "app": "Motion", "state": [],
        "tree": { "id": 0, "kind": "Text",
            "props": { "value": "x", "animate": "fast", "size": 24 },
            "children": [], "events": {} },
        "theme": { "mode": "light", "accent": "#097FEB", "radius": 0 }
    })
    .to_string();
    let ir = parse_ir(&doc).unwrap();
    let state = StateView::from_ir(&ir);
    let laid = layout(&ir, &state, 360, 640);
    assert_eq!(laid.len(), 1, "the animated node still lays out");
}
