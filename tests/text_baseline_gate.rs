//! Signed text-baseline gate, Phase 1 (PIXEL_GATE_CI_PROPOSAL.md).
//! Metrics are a real CI door; the pixel diff% part reports N/A in CI because
//! no font file, CoreText or Flutter oracle exists on the runner.

mod common;

use common::text_baseline_metrics::{
    cases, check, computed, find_case, metrics_fixture, per_em, pixel_fixture, tolerance_px, Case,
    CheckResult, PINNED_FONT_SHA256, PINNED_SOURCE_COMMIT, PIXEL_NA_REASON,
};

#[test]
fn signed_fixture_provenance_is_pinned() {
    let f = metrics_fixture();
    assert_eq!(
        f["source"]["commit"].as_str().unwrap(),
        PINNED_SOURCE_COMMIT,
        "fixture must name the measured Flutter revision"
    );
    assert_eq!(
        f["source"]["font_sha256"].as_str().unwrap(),
        PINNED_FONT_SHA256,
        "fixture must name the exact measured font file"
    );
    assert_eq!(f["source"]["font"].as_str().unwrap(), "Roboto-Regular.ttf");
    let m = per_em(&f);
    assert_eq!((m.ascent, m.descent, m.leading), (950, 250, 0));
    assert_eq!(tolerance_px(&f), 1.0);
    assert_eq!(f["thresholds"]["latinDiffPct"].as_i64().unwrap(), 2);
    assert_eq!(f["thresholds"]["cjkDiffPct"].as_i64().unwrap(), 12);
    assert_eq!(
        f["thresholds"]["maxErrGated"].as_bool().unwrap(),
        false,
        "maxErr is report-only"
    );
}

#[test]
fn signed_metrics_gate_passes_every_case() {
    let f = metrics_fixture();
    let signed = cases(&f);
    assert_eq!(signed.len(), 112, "7 sizes x 4 strings x 4 lineHeights");
    assert_eq!(
        signed.iter().filter(|c| c.script == "latin").count(),
        56,
        "Latin rows signed"
    );
    assert_eq!(
        signed.iter().filter(|c| c.script == "cjk").count(),
        56,
        "CJK rows signed"
    );
    let result = check(&per_em(&f), &signed, tolerance_px(&f));
    assert_eq!(result.checked, signed.len(), "no case may be skipped");
    assert_eq!(result.baseline_checked, 28, "7 sizes x 4 strings, unset");
    assert!(
        result.violations.is_empty(),
        "signed gate turned red: {:?}",
        result.violations
    );
}

#[test]
fn boundary_calibration_px48_unset_is_within_one_px() {
    let f = metrics_fixture();
    let signed = cases(&f);
    let row = find_case(&signed, 48, "Hg", None);
    assert_eq!(row.ascent_px, 44.53125, "signed ascent copied from oracle");
    assert_eq!(
        row.descent_px, 11.71875,
        "signed descent copied from oracle"
    );
    let (box_px, baseline_px) = computed(&per_em(&f), row);
    assert_eq!(box_px, 57.0, "render box at px48 unset");
    assert_eq!(baseline_px, 45.0, "render baseline at px48 unset");
    let diff = (baseline_px - row.baseline_px).abs();
    assert!(
        diff <= tolerance_px(&f),
        "signed px48 baseline {:.5}, render {}, diff {:.5}",
        row.baseline_px,
        baseline_px,
        diff
    );
    // The same reference shifted +2 px must be caught by the comparator.
    let mut shifted = row.clone();
    shifted.baseline_px += 2.0;
    let bad: CheckResult = check(&per_em(&f), &[shifted], tolerance_px(&f));
    assert_eq!(
        bad.violations.len(),
        1,
        "a gate that cannot fail is rejected"
    );
}

#[test]
fn mutation_plus_two_is_detected_once() {
    let f = metrics_fixture();
    let signed: Vec<Case> = cases(&f)
        .into_iter()
        .map(|mut c| {
            if c.font_px == 48 && c.sample == "Hg" && c.line_height.is_none() {
                c.baseline_px += 2.0;
            }
            c
        })
        .collect();
    let result = check(&per_em(&f), &signed, tolerance_px(&f));
    assert_eq!(
        result.checked,
        signed.len(),
        "vacuous empty passes are impossible"
    );
    assert_eq!(
        result.violations.len(),
        1,
        "one signed value +2 px must yield exactly one violation: {:?}",
        result.violations
    );
    assert_eq!(result.violations[0].field, "baseline_px");
    assert_eq!(result.violations[0].font_px, 48);
    assert_eq!(result.violations[0].sample, "Hg");
    assert_eq!(result.violations[0].got, 45.0);
    assert_eq!(result.violations[0].want, 46.28125);
}

#[test]
fn pixel_diff_percent_reports_na_and_never_gates() {
    let p = pixel_fixture();
    assert_eq!(
        p["note"].as_str().unwrap(),
        PIXEL_NA_REASON,
        "CI must state why the pixel part is not gated"
    );
    assert_eq!(
        p["source"]["font_sha256"].as_str().unwrap(),
        PINNED_FONT_SHA256
    );
    let rows = p["cases"].as_array().expect("pixel reference cases");
    assert_eq!(rows.len(), 6, "6 signed diff% cases");
    for row in rows {
        assert!(
            row["diffPct"].as_f64().is_some(),
            "each pixel case must carry signed diff%"
        );
        assert_eq!(row["maxErr"].as_i64().unwrap(), 255, "report-only maxErr");
    }
    assert_eq!(
        metrics_fixture()["thresholds"]["maxErrGated"]
            .as_bool()
            .unwrap(),
        false
    );
    println!("pixel diff% gate in CI: {PIXEL_NA_REASON}");
}
