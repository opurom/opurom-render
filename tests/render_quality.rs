//! Root `renderQuality` opt-in contract: independent fields, validation and a
//! default path that leaves every pinned digest byte-identical.

use opurom_render::{digest_hex, parse_ir, RenderQuality, Renderer, StateView};
use serde_json::json;
use std::{fs, path::PathBuf, process::Command};

fn compat_renderer(width: u32, height: u32) -> Renderer {
    let mut renderer = Renderer::new(width, height);
    renderer.set_quality(opurom_render::QualityProfile::Compat);
    renderer
}

fn ir_with(quality: Option<serde_json::Value>) -> String {
    let mut doc = json!({
        "ir": "0.1.0", "app": "RQ", "state": [],
        "tree": { "id": 0, "kind": "Column", "props": { "padding": 0 },
            "children": [ { "id": 1, "kind": "Container", "props": {
                "width": 40, "height": 40, "color": "#112233", "radius": 12
            }, "children": [], "events": {} } ], "events": {} },
        "theme": { "mode": "light", "accent": "#097FEB", "radius": 12 }
    });
    if let Some(q) = quality {
        doc["renderQuality"] = q;
    }
    doc.to_string()
}

#[test]
fn all_five_subfields_parse_independently_and_unknown_keys_are_ignored() {
    let full = parse_ir(&ir_with(Some(json!({
        "msaa": 4, "gradientBits": 16, "curve": "bezier",
        "stroke": "physical", "textBaseline": "grid", "futureHint": 7
    }))))
    .unwrap();
    let rq = full.render_quality.as_ref().unwrap();
    assert_eq!(rq.msaa, Some(4));
    assert_eq!(rq.gradient_bits, Some(16));
    assert_eq!(rq.curve.as_deref(), Some("bezier"));
    assert_eq!(rq.stroke.as_deref(), Some("physical"));
    assert_eq!(rq.text_baseline.as_deref(), Some("grid"));
    assert!(!rq.is_default());

    let partial = parse_ir(&ir_with(Some(json!({ "msaa": 4 }))))
        .unwrap()
        .render_quality
        .unwrap();
    assert_eq!(partial.msaa, Some(4));
    assert_eq!(partial.gradient_bits, None);
    assert_eq!(partial.curve, None);
    assert_eq!(partial.stroke, None);
    assert_eq!(partial.text_baseline, None);

    assert!(parse_ir(&ir_with(None)).unwrap().render_quality.is_none());
    assert!(parse_ir(&ir_with(Some(json!(null))))
        .unwrap()
        .render_quality
        .is_none());
    assert!(parse_ir(&ir_with(Some(json!({ "futureHint": 7 }))))
        .unwrap()
        .render_quality
        .unwrap()
        .is_default());
    assert_eq!(RenderQuality::default().msaa, None);
}

#[test]
fn unsupported_or_malformed_values_are_rejected() {
    for bad in [
        json!({ "msaa": 8 }),
        json!({ "msaa": "4" }),
        json!({ "gradientBits": 10 }),
        json!({ "curve": "polygon" }),
        json!({ "stroke": "logical" }),
        json!({ "textBaseline": "band" }),
        json!("4x"),
        json!([]),
    ] {
        assert!(
            parse_ir(&ir_with(Some(bad.clone()))).is_err(),
            "invalid renderQuality must be rejected: {bad}"
        );
    }
}

#[test]
fn default_path_bytes_and_pinned_digests_stay_identical() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let counter = fs::read_to_string("../opurom-examples/golden/counter.opuir.json").unwrap();
    let ir = parse_ir(&counter).unwrap();
    assert!(ir.render_quality.is_none());
    let state = StateView::from_ir(&ir);
    assert_eq!(
        digest_hex(&compat_renderer(320, 240).render(&ir, &state).pixels),
        "8d45f412996a99c6"
    );
    assert_eq!(
        digest_hex(&compat_renderer(800, 600).render(&ir, &state).pixels),
        "318beb1dd337bc4e"
    );

    let effects = fs::read_to_string("tests/fixtures/effects.opuir.json").unwrap();
    let eir = parse_ir(&effects).unwrap();
    assert!(eir.render_quality.is_none());
    let ppm = dir.join("render-quality-effects.ppm");
    let out = Command::new(env!("CARGO_BIN_EXE_opurom-render"))
        .arg("tests/fixtures/effects.opuir.json")
        .args(["96", "160"])
        .arg(&ppm)
        .args(["--quality", "compat"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        format!("digest={}", digest_hex(&fs::read(&ppm).unwrap())),
        fs::read_to_string("tests/fixtures/effects.digest.txt")
            .unwrap()
            .trim()
    );
}
