//! Opt-in exact text typography in the system-font span JSON: weight,
//! letterSpacing and fontPx (DEFAULT_UI_GLASS_REFINEMENT / spec). Fields are
//! emitted only when explicitly declared, so default spans and all pinned
//! frame goldens keep their exact bytes.

use opurom_render::{
    digest_hex, layout, parse_ir, text_spans, text_spans_for, text_spans_json, StateView,
};
use serde_json::json;

fn compat_renderer(width: u32, height: u32) -> opurom_render::Renderer {
    let mut renderer = opurom_render::Renderer::new(width, height);
    renderer.set_quality(opurom_render::QualityProfile::Compat);
    renderer
}

const COUNTER_PATH: &str = "../opurom-examples/golden/counter.opuir.json";

fn text_doc(props: serde_json::Value) -> String {
    let mut p = json!({ "value": "Hi" });
    for (k, v) in props.as_object().unwrap() {
        p[k] = v.clone();
    }
    json!({
        "ir": "0.1.0", "app": "Typo", "state": [],
        "tree": { "id": 0, "kind": "Text", "props": p,
            "children": [], "events": {} },
        "theme": { "mode": "light", "accent": "#097FEB", "radius": 0 }
    })
    .to_string()
}

fn span_json(props: serde_json::Value, w: u32, h: u32) -> String {
    let ir = parse_ir(&text_doc(props)).expect("text IR must parse");
    text_spans_for(&ir, &StateView::from_ir(&ir), w, h)
}

#[test]
fn explicit_typography_fields_are_emitted_in_fixed_order() {
    let j = span_json(
        json!({ "weight": "bold", "letterSpacing": -2, "fontPx": 34 }),
        240,
        200,
    );
    assert!(j.contains("\"weight\":\"bold\""), "weight key: {j}");
    assert!(
        j.contains("\"letterSpacing\":-2"),
        "negative letterSpacing: {j}"
    );
    assert!(j.contains("\"fontPx\":34"), "exact font size: {j}");
    // Fixed order: origin before the three opt-in keys.
    let w = j.find("\"weight\"").unwrap();
    let ls = j.find("\"letterSpacing\"").unwrap();
    let fp = j.find("\"fontPx\"").unwrap();
    let origin = j.find("\"origin\"").unwrap();
    assert!(
        origin < w && w < ls && ls < fp,
        "deterministic key order: {j}"
    );
    assert!(!j.contains('.'), "no floats in span JSON: {j}");
}

#[test]
fn absent_typography_fields_leave_span_json_unchanged() {
    let plain = span_json(json!({}), 240, 200);
    for key in ["weight", "letterSpacing", "fontPx"] {
        assert!(
            !plain.contains(&format!("\"{key}\"")),
            "absent {key} not emitted: {plain}"
        );
    }
    // Even with legacy size set, no typography keys leak in.
    let sized = span_json(json!({ "size": 34 }), 240, 200);
    for key in ["weight", "letterSpacing", "fontPx"] {
        assert!(
            !sized.contains(&format!("\"{key}\"")),
            "size-only has no {key}: {sized}"
        );
    }
}

#[test]
fn each_field_is_independently_opt_in() {
    let only_weight = span_json(json!({ "weight": "normal" }), 240, 200);
    assert!(only_weight.contains("\"weight\":\"normal\""));
    assert!(!only_weight.contains("letterSpacing"));
    assert!(!only_weight.contains("fontPx"));

    let only_ls = span_json(json!({ "letterSpacing": 0 }), 240, 200);
    assert!(only_ls.contains("\"letterSpacing\":0"));
    assert!(!only_ls.contains("weight"));
    assert!(!only_ls.contains("fontPx"));

    let only_fp = span_json(json!({ "fontPx": 16 }), 240, 200);
    assert!(only_fp.contains("\"fontPx\":16"));
    assert!(!only_fp.contains("weight"));
    assert!(!only_fp.contains("letterSpacing"));
}

#[test]
fn typography_reaches_textspan_struct_not_only_json() {
    let ir = parse_ir(&text_doc(
        json!({ "weight": "bold", "letterSpacing": 3, "fontPx": 28 }),
    ))
    .unwrap();
    let laid = layout(&ir, &StateView::from_ir(&ir), 240, 200);
    let spans = text_spans(&laid);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].weight.as_deref(), Some("bold"));
    assert_eq!(spans[0].letter_spacing, Some(3));
    assert_eq!(spans[0].font_px, Some(28));
    // text_spans_json is byte-identical to text_spans_for on the same layout.
    assert_eq!(
        text_spans_json(&laid),
        text_spans_for(&ir, &StateView::from_ir(&ir), 240, 200)
    );
}

#[test]
fn font_px_rejects_zero_and_negative_but_letter_spacing_allows_them() {
    assert!(
        parse_ir(&text_doc(json!({ "fontPx": 0 }))).is_err(),
        "fontPx 0 rejected"
    );
    assert!(
        parse_ir(&text_doc(json!({ "fontPx": -1 }))).is_err(),
        "fontPx negative rejected"
    );
    let neg = parse_ir(&text_doc(json!({ "letterSpacing": -5 })));
    assert!(neg.is_ok(), "negative letterSpacing accepted");
}

#[test]
fn typography_does_not_touch_the_raster_frame_bytes() {
    // The three fields feed only the system-font span layer, never the bitmap
    // raster path; the canonical counter frames stay pinned.
    let text = std::fs::read_to_string(COUNTER_PATH).unwrap();
    let ir = parse_ir(&text).unwrap();
    let state = StateView::from_ir(&ir);
    assert_eq!(
        digest_hex(&compat_renderer(320, 240).render(&ir, &state).pixels),
        "8d45f412996a99c6"
    );
    assert_eq!(
        digest_hex(&compat_renderer(800, 600).render(&ir, &state).pixels),
        "318beb1dd337bc4e"
    );

    // Adding typography to a text node changes the span JSON but not its pixels.
    let plain_ir = parse_ir(&text_doc(json!({}))).unwrap();
    let typed_ir = parse_ir(&text_doc(json!({ "weight": "bold", "fontPx": 40 }))).unwrap();
    let ps = StateView::from_ir(&plain_ir);
    let ts = StateView::from_ir(&typed_ir);
    assert_eq!(
        digest_hex(&compat_renderer(240, 200).render(&plain_ir, &ps).pixels),
        digest_hex(&compat_renderer(240, 200).render(&typed_ir, &ts).pixels),
        "typography must not alter raster bytes"
    );
}
