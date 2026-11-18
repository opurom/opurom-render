//! Raster-parity proof that opt-in proportional metrics read as tighter
//! typography through the real draw path, not only as smaller width numbers.
//!
//! The same mixed string is rendered with `Fixed` and `Proportional` into two
//! small RGBA buffers via `Renderer::pixels` (the production raster). The two
//! frames must differ (proportional reflows to a narrower pen), each must be
//! byte-deterministic across two runs, and `measure` must place the
//! proportional run strictly left of the fixed one.

use opurom_render::{digest_hex, measure, parse_ir, FontMetrics, StateView};
use serde_json::json;

fn compat_renderer(width: u32, height: u32) -> opurom_render::Renderer {
    let mut renderer = opurom_render::Renderer::new(width, height);
    renderer.set_quality(opurom_render::QualityProfile::Compat);
    renderer
}

const TEXT: &str = "Wide il Wide";
const SCALE: i64 = 3; // size 24 resolves to scale (24 + 3) / 7 = 3.

/// A `Text` node at a fixed size; `theme_metrics` is the opt-in theme flag.
fn text_ir(theme_metrics: Option<&str>) -> String {
    let mut theme = json!({ "mode": "light", "accent": "#097FEB", "radius": 4 });
    if let Some(m) = theme_metrics {
        theme["fontMetrics"] = json!(m);
    }
    json!({
        "ir": "0.1.0", "app": "T", "state": [],
        "tree": { "id": 0, "kind": "Column", "props": { "padding": 4 },
            "children": [ { "id": 1, "kind": "Text", "props": {
                "value": TEXT, "size": 24 },
                "children": [], "events": {} } ], "events": {} },
        "theme": theme
    })
    .to_string()
}

/// Render the string at one metrics mode into a small RGBA buffer.
fn frame(metrics: Option<&str>) -> Vec<u8> {
    let ir = parse_ir(&text_ir(metrics)).unwrap();
    let state = StateView::from_ir(&ir);
    // 240 px keeps the whole run on one line for both modes at scale 3.
    compat_renderer(240, 40).pixels(&ir, &state)
}

#[test]
fn proportional_reflows_the_same_string_but_stays_byte_deterministic() {
    let fixed = frame(None);
    let prop = frame(Some("proportional"));
    // Proportional packs glyphs tighter, so the same string reflows:
    // the two frames are not byte-equal.
    assert_ne!(
        digest_hex(&fixed),
        digest_hex(&prop),
        "proportional must reflow the same string"
    );
    // Each mode is byte-deterministic across two independent runs.
    assert_eq!(fixed, frame(None), "fixed frame is not reproducible");
    assert_eq!(
        prop,
        frame(Some("proportional")),
        "proportional frame is not reproducible"
    );
}

#[test]
fn proportional_is_strictly_narrower_by_measure() {
    let fixed = measure(TEXT, SCALE, FontMetrics::Fixed);
    let prop = measure(TEXT, SCALE, FontMetrics::Proportional);
    assert!(
        prop < fixed,
        "measure(Proportional)={prop} must be < measure(Fixed)={fixed}"
    );
}

#[test]
fn narrow_glyphs_are_measured_narrower_than_wide_glyphs() {
    // i/l/. are narrow; W/m/M are wide; every narrow glyph must measure
    // strictly under every wide glyph at the same scale.
    let narrow = ['i', 'l', '.'];
    let wide = ['W', 'm', 'M'];
    let width = |c: char| measure(&c.to_string(), SCALE, FontMetrics::Proportional);
    let narrow_max = narrow.iter().map(|c| width(*c)).max().unwrap();
    let wide_min = wide.iter().map(|c| width(*c)).min().unwrap();
    assert!(
        narrow_max < wide_min,
        "narrow glyphs (i l .) must measure under wide glyphs (W m M): \
         narrow_max={narrow_max} wide_min={wide_min}"
    );
}
