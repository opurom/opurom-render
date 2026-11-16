//! Integer fixed-point cubic Bezier evaluator: known-segment oracle, a probe
//! that a wrong (chord) evaluation is detectable, and pinned default bytes.

use opurom_render::{cubic_bezier, digest_hex, parse_ir, Renderer, StateView};
use serde_json::json;
use std::{fs, path::PathBuf, process::Command};

const P0: (i64, i64) = (0, 0);
const P1: (i64, i64) = (0, 100);
const P2: (i64, i64) = (100, 100);
const P3: (i64, i64) = (100, 0);
const UNIT: i64 = 1 << 16;

/// Exact rational point at each parameter, kept as (numerator, denominator)
/// so the oracle itself uses no float. Each expected coordinate is the rounded
/// integer the evaluator must land on within one unit.
fn oracle(t_num: i64) -> ((i64, i64), (i64, i64)) {
    let t_den = 4i64;
    let u_num = t_den - t_num;
    let denom = t_den * t_den * t_den;
    let axis = |a: i64, b: i64, c: i64, d: i64| -> (i64, i64) {
        let num = u_num.pow(3) * a
            + 3 * u_num.pow(2) * t_num * b
            + 3 * u_num * t_num.pow(2) * c
            + t_num.pow(3) * d;
        (num, denom)
    };
    let (xn, xd) = axis(P0.0, P1.0, P2.0, P3.0);
    let (yn, yd) = axis(P0.1, P1.1, P2.1, P3.1);
    // Truncating the exact rational keeps the oracle on integers; the
    // evaluator rounds, so the two stay within one unit.
    ((xn / xd, yn / yd), (xd, yd))
}

#[test]
fn known_segment_matches_oracle_points_within_one_integer_unit() {
    // (t numerator over 4, expected x, expected y) from the analytic curve.
    // Values are rounded hand results: 15.625->16, 56.25->56, 84.375->84.
    let cases: &[(i64, (i64, i64))] = &[
        (0, (0, 0)),
        (1, (16, 56)),
        (2, (50, 75)),
        (3, (84, 56)),
        (4, (100, 0)),
    ];
    for &(n, want) in cases {
        let t = (UNIT / 4) * n;
        let got = cubic_bezier(P0, P1, P2, P3, t);
        assert!(
            (got.0 - want.0).abs() <= 1 && (got.1 - want.1).abs() <= 1,
            "t={n}/4 got {got:?} want within 1 of {want:?}"
        );
        let (exact, _den) = oracle(n);
        assert!(
            (got.0 - exact.0).abs() <= 1 && (got.1 - exact.1).abs() <= 1,
            "rational oracle mismatch at t={n}/4"
        );
    }
    assert_eq!(cubic_bezier(P0, P1, P2, P3, UNIT / 2), (50, 75));
    assert_eq!(
        cubic_bezier(P0, P1, P2, P3, UNIT / 2),
        cubic_bezier(P0, P1, P2, P3, UNIT / 2)
    );
}

#[test]
fn chord_approximation_is_detected_and_control_points_matter() {
    let mid = cubic_bezier(P0, P1, P2, P3, UNIT / 2);
    // A straight chord from (0,0) to (100,0) would give (50,0) at the midpoint.
    let chord = ((P0.0 + P3.0) / 2, (P0.1 + P3.1) / 2);
    assert!(
        (mid.1 - chord.1).abs() > 1,
        "the cubic must differ from a chord by more than one unit"
    );
    // Moving one control point changes the evaluated point.
    let moved = cubic_bezier(P0, (0, 80), P2, P3, UNIT / 2);
    assert_ne!(mid, moved, "control points must influence the curve");
}

#[test]
fn default_path_bytes_and_command_stream_stay_identical() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let counter = fs::read_to_string("../opurom-examples/golden/counter.opuir.json").unwrap();
    let ir = parse_ir(&counter).unwrap();
    let state = StateView::from_ir(&ir);
    assert_eq!(
        digest_hex(&Renderer::new(320, 240).render(&ir, &state).pixels),
        "8d45f412996a99c6"
    );
    assert_eq!(
        digest_hex(&Renderer::new(800, 600).render(&ir, &state).pixels),
        "318beb1dd337bc4e"
    );

    let effects_ppm = dir.join("bezier-effects.ppm");
    let out = Command::new(env!("CARGO_BIN_EXE_opurom-render"))
        .arg("tests/fixtures/effects.opuir.json")
        .args(["96", "160"])
        .arg(&effects_ppm)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        format!("digest={}", digest_hex(&fs::read(&effects_ppm).unwrap())),
        fs::read_to_string("tests/fixtures/effects.digest.txt")
            .unwrap()
            .trim()
    );

    // The curve flag is a contract signal only until a vector-path consumer
    // exists; it must not perturb the recorded command stream.
    let base = json!({
        "ir": "0.1.0", "app": "B", "state": [],
        "tree": { "id": 0, "kind": "Column", "props": { "padding": 0 },
            "children": [], "events": {} },
        "theme": { "mode": "light", "accent": "#097FEB", "radius": 0 }
    });
    let mut curved = base.clone();
    curved["renderQuality"] = json!({ "curve": "bezier" });
    let a = parse_ir(&base.to_string()).unwrap();
    let b = parse_ir(&curved.to_string()).unwrap();
    let sa = StateView::from_ir(&a);
    let sb = StateView::from_ir(&b);
    assert_eq!(a.render_quality, None);
    assert_eq!(
        b.render_quality.as_ref().unwrap().curve.as_deref(),
        Some("bezier")
    );
    assert_eq!(
        Renderer::new(64, 64)
            .display_list(&a, &sa)
            .to_json()
            .unwrap(),
        Renderer::new(64, 64)
            .display_list(&b, &sb)
            .to_json()
            .unwrap()
    );
}
