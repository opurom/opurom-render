//! B9: opt-in `fontPx` text sizing (SYSTEM_FONT_DISPLAY.md §2 / spec 01a5b26).
//! With `fontPx` declared, natural width/height come from it so the system-font
//! box matches the painted glyph size and leaves descender room. The default
//! quantized-scale path and all pinned goldens are byte-identical.

use opurom_render::{digest, digest_hex, layout, parse_ir, Renderer, StateView};
use serde_json::json;

const COUNTER_PATH: &str = "../opurom-examples/golden/counter.opuir.json";

fn child_rect(fp: Option<i64>, kind: &str, w: u32, h: u32) -> (i64, i64) {
    let mut props = json!({ "value": "Typography" });
    if let Some(px) = fp {
        props["fontPx"] = json!(px);
    }
    let doc = json!({
        "ir": "0.1.0", "app": "B9", "state": [],
        "tree": { "id": 0, "kind": kind, "props": { "padding": 0 },
            "children": [ { "id": 1, "kind": "Text", "props": props,
                "children": [], "events": {} } ], "events": {} },
        "theme": { "mode": "light", "accent": "#097FEB", "radius": 0 }
    })
    .to_string();
    let ir = parse_ir(&doc).unwrap();
    let state = StateView::from_ir(&ir);
    let laid = layout(&ir, &state, w, h);
    let r = laid.iter().find(|n| n.id == 1).unwrap().rect;
    (r.w, r.h)
}

fn rect_digest(kind: &str) -> String {
    let mut bytes = Vec::new();
    for fp in [16, 34, 56, 96, 224] {
        let (w, h) = child_rect(Some(fp), kind, 1080, 400);
        bytes.extend_from_slice(&w.to_le_bytes());
        bytes.extend_from_slice(&h.to_le_bytes());
    }
    format!("{:016x}", digest(&bytes))
}

#[test]
fn font_px_drives_natural_height_with_1_5_line_box() {
    // h = ceil(3*fontPx/2), verified across sizes incl. the 96sp headline.
    for fp in [16, 34, 56, 96, 224] {
        let (_, h) = child_rect(Some(fp), "Row", 1080, 400);
        let expected = (3 * fp + 1) / 2; // i128 ceil, fp positive
        assert_eq!(h, expected, "fontPx {fp} height");
    }
    // The 1.5 box is strictly taller than the font size, leaving descender room
    // so the host clipRect does not cut y/g/p tails.
    let (_, h96) = child_rect(Some(96), "Row", 1080, 400);
    assert!(h96 > 96, "96px box leaves descender room: {h96}");
}

#[test]
fn font_px_drives_natural_width_from_quantized_scale() {
    // w = text_width(text, ceil(fontPx/7), metrics): grows with fontPx.
    let mut prev = 0;
    for fp in [16, 34, 56, 96, 224] {
        let (w, _) = child_rect(Some(fp), "Column", 1080, 400);
        assert!(w >= prev, "width monotonic in fontPx: fp={fp}");
        prev = w;
    }
    let (w96, _) = child_rect(Some(96), "Column", 1080, 400);
    assert!(w96 > 0, "headline has natural width: {w96}");
}

#[test]
fn sized_rects_are_byte_pinned() {
    // Byte-level locks on the (w,h) sequence for both layouts.
    assert_eq!(
        rect_digest("Column"),
        "769527e151b62b43",
        "Column w/h digest"
    );
    assert_eq!(rect_digest("Row"), "a5858c2a9be184b1", "Row w/h digest");
}

#[test]
fn absent_font_px_keeps_legacy_quantized_box() {
    // No fontPx: the old scale path, unchanged by B9.
    let (w_plain, h_plain) = child_rect(None, "Row", 1080, 400);
    let (w_none, h_none) = child_rect(None, "Row", 1080, 400);
    assert_eq!(
        (w_plain, h_plain),
        (w_none, h_none),
        "default deterministic"
    );
    // GLYPH_H(7) * scale where scale = (size+3)/7 clamp 1..8; default size 16
    // gives scale 2, so h = 14 and is far smaller than the 1.5 fontPx box.
    assert_eq!(h_plain, 14, "legacy quantized height unchanged");
}

#[test]
fn default_path_keeps_the_two_counter_goldens() {
    let text = std::fs::read_to_string(COUNTER_PATH).unwrap();
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
fn span_golden_input_stays_byte_identical() {
    // The pinned span golden is owned by tests/text_span.rs (MIXED fixture, no
    // fontPx). B9 does not touch that fixture; its golden test stays green.
    // Here we only assert that declaring no fontPx emits no fontPx key.
    use opurom_render::text_spans_for;
    let doc = json!({
        "ir": "0.1.0", "app": "Spans", "state": [],
        "tree": { "id": 0, "kind": "Text",
            "props": { "value": "plain", "size": 20 }, "children": [], "events": {} },
        "theme": { "mode": "light", "accent": "#097FEB", "radius": 0 }
    })
    .to_string();
    let ir = parse_ir(&doc).unwrap();
    let state = StateView::from_ir(&ir);
    let json = text_spans_for(&ir, &state, 240, 200);
    assert!(
        !json.contains("fontPx"),
        "no fontPx key on default text: {json}"
    );
}
