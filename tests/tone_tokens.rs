//! Theme-token semantic tones: parsing, palette resolution, pixel override and
//! default-palette fallback. All assertions are integer-only and deterministic.

use opurom_render::{
    digest_hex, layout, parse_ir, Contrast, Palette, Renderer, Rgba, StateView, ThemeTokens, Tone,
    ERROR, INFO, SUCCESS, WARNING,
};
use serde_json::json;

fn compat_renderer(width: u32, height: u32) -> opurom_render::Renderer {
    let mut renderer = opurom_render::Renderer::new(width, height);
    renderer.set_quality(opurom_render::QualityProfile::Compat);
    renderer
}

const W: u32 = 160;
const H: u32 = 96;

fn theme(tokens: serde_json::Value) -> serde_json::Value {
    json!({ "mode": "light", "accent": "#097FEB", "radius": 4, "tokens": tokens })
}

fn ir_with_tokens(tokens: serde_json::Value) -> String {
    serde_json::to_string(&json!({
        "ir": "0.1.0", "app": "Tone",
        "state": [],
        "tree": {
            "id": 0, "kind": "Column",
            "props": { "padding": 8, "align": "start" },
            "children": [
                { "id": 1, "kind": "Badge",
                  "props": { "width": 80, "height": 40, "label": "", "tone": "success" },
                  "children": [], "events": {} }
            ], "events": {}
        },
        "theme": theme(tokens)
    }))
    .unwrap()
}

fn render(ir: &str) -> Vec<u8> {
    let ir = parse_ir(ir).unwrap();
    compat_renderer(W, H).pixels(&ir, &StateView::from_ir(&ir))
}

fn pixel(pixels: &[u8], x: i64, y: i64) -> [u8; 4] {
    let i = (y as usize * W as usize + x as usize) * 4;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

#[test]
fn every_tone_token_parses_with_full_alpha() {
    let ir = parse_ir(&ir_with_tokens(json!({
        "success": "#00AA00",
        "warning": "#CC8800",
        "error": "#991111",
        "info": "#2244AA",
        "primary": "#123456",
        "secondary": "#654321"
    })))
    .unwrap();
    let t = &ir.theme.tokens;
    assert_eq!(t.success, Some(Rgba::rgb(0x00, 0xAA, 0x00)));
    assert_eq!(t.warning, Some(Rgba::rgb(0xCC, 0x88, 0x00)));
    assert_eq!(t.error, Some(Rgba::rgb(0x99, 0x11, 0x11)));
    assert_eq!(t.info, Some(Rgba::rgb(0x22, 0x44, 0xAA)));
    assert_eq!(t.primary, Some(Rgba::rgb(0x12, 0x34, 0x56)));
    assert_eq!(t.secondary, Some(Rgba::rgb(0x65, 0x43, 0x21)));
    // Parsed tokens carry the opaque alpha the surface/stage grammar uses.
    assert_eq!(t.success.unwrap().a, 255);
}

#[test]
fn tone_tokens_use_the_same_strict_hex_grammar() {
    // Reuses the surface/stage parser, so short/long/named forms are rejected.
    let err = parse_ir(&ir_with_tokens(json!({ "success": "#0a0" })))
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("theme.tokens.success must be #RRGGBB"),
        "{err:?}"
    );
}

#[test]
fn absent_tokens_leave_the_default_theme_tokens_untouched() {
    let ir = parse_ir(&ir_with_tokens(json!({}))).unwrap();
    assert_eq!(
        ir.theme.tokens,
        ThemeTokens {
            contrast: Contrast::Normal,
            ..ThemeTokens::default()
        }
    );
}

#[test]
fn palette_tone_resolver_prefers_token_then_constant() {
    let toned = parse_ir(&ir_with_tokens(json!({
        "success": "#00AA00",
        "warning": "#CC8800",
        "error": "#991111",
        "info": "#2244AA"
    })))
    .unwrap();
    let pal = Palette::from_theme(&toned.theme);
    assert_eq!(pal.tone_color(Tone::Success), Rgba::rgb(0x00, 0xAA, 0x00));
    assert_eq!(pal.tone_color(Tone::Warning), Rgba::rgb(0xCC, 0x88, 0x00));
    assert_eq!(pal.tone_color(Tone::Error), Rgba::rgb(0x99, 0x11, 0x11));
    assert_eq!(pal.tone_color(Tone::Info), Rgba::rgb(0x22, 0x44, 0xAA));

    // Absent tokens fall back to the measured interaction constants.
    let plain = parse_ir(&ir_with_tokens(json!({}))).unwrap();
    let pal = Palette::from_theme(&plain.theme);
    assert_eq!(pal.tone_color(Tone::Success), SUCCESS);
    assert_eq!(pal.tone_color(Tone::Warning), WARNING);
    assert_eq!(pal.tone_color(Tone::Error), ERROR);
    assert_eq!(pal.tone_color(Tone::Info), INFO);
    assert_eq!(pal.tone_color(Tone::Neutral), opurom_render::DEEP_INK);
}

#[test]
fn tone_override_changes_pixels_and_matches_the_token() {
    let default = render(&ir_with_tokens(json!({})));
    let overridden = render(&ir_with_tokens(json!({ "success": "#00AA00" })));
    assert_ne!(
        digest_hex(&default),
        digest_hex(&overridden),
        "a success token must change the rendered frame"
    );

    // The status dot sits at the upper-right inside edge; it must now carry the
    // token colour, not the default SUCCESS constant.
    let ir = parse_ir(&ir_with_tokens(json!({ "success": "#00AA00" }))).unwrap();
    let r = layout(&ir, &StateView::from_ir(&ir), W, H)
        .into_iter()
        .find(|n| n.id == 1)
        .unwrap()
        .rect;
    let dot = pixel(&overridden, r.x + r.w - 4, r.y + 4);
    assert_eq!(dot, [0x00, 0xAA, 0x00, 255]);
    // And the default frame's dot still uses the measured constant.
    let dot_default = pixel(&default, r.x + r.w - 4, r.y + 4);
    assert_eq!(dot_default, [SUCCESS.r, SUCCESS.g, SUCCESS.b, 255]);
}

#[test]
fn tone_border_tint_also_reads_the_token() {
    let overridden = render(&ir_with_tokens(json!({ "success": "#00AA00" })));
    let ir = parse_ir(&ir_with_tokens(json!({ "success": "#00AA00" }))).unwrap();
    let r = layout(&ir, &StateView::from_ir(&ir), W, H)
        .into_iter()
        .find(|n| n.id == 1)
        .unwrap()
        .rect;
    // Top-centre edge is the tone border stroke.
    assert_eq!(
        pixel(&overridden, r.x + r.w / 2, r.y),
        [0x00, 0xAA, 0x00, 255]
    );
}

const GOLDEN_IR: &str = r##"{
  "ir": "0.1.0",
  "app": "Counter",
  "state": [ { "name": "count", "type": "int", "init": 0 } ],
  "tree": {
    "id": 0, "kind": "Column",
    "props": { "gap": 8, "padding": 16, "align": "center" },
    "children": [
      { "id": 1, "kind": "Text", "props": { "value": "Count: {count}", "size": 24 },
        "children": [], "events": {} },
      { "id": 2, "kind": "Row", "props": { "gap": 8 }, "children": [
          { "id": 3, "kind": "Button", "props": { "label": "-" }, "children": [],
            "events": { "tap": { "op": "add", "target": "count", "value": -1 } } },
          { "id": 4, "kind": "Button", "props": { "label": "+" }, "children": [],
            "events": { "tap": { "op": "add", "target": "count", "value": 1 } } }
        ], "events": {} }
    ], "events": {}
  },
  "theme": { "mode": "system", "accent": "#097FEB", "radius": 12 }
}"##;

#[test]
fn three_canonical_goldens_unchanged_by_new_tokens() {
    let ir = parse_ir(GOLDEN_IR).unwrap();
    let mut state = StateView::from_ir(&ir);
    assert_eq!(
        digest_hex(&compat_renderer(320, 240).pixels(&ir, &state)),
        "8d45f412996a99c6"
    );
    assert_eq!(
        digest_hex(&compat_renderer(800, 600).pixels(&ir, &state)),
        "318beb1dd337bc4e"
    );
    state.set("count", opurom_render::StateValue::Int(7));
    assert_eq!(
        digest_hex(&compat_renderer(800, 600).pixels(&ir, &state)),
        "40073cb95c912d80"
    );
}

#[test]
fn tone_override_is_deterministic_across_runs() {
    let ir = ir_with_tokens(json!({ "success": "#00AA00" }));
    let first = digest_hex(&render(&ir));
    for _ in 0..3 {
        assert_eq!(first, digest_hex(&render(&ir)));
    }
}
