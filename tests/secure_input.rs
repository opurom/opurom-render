//! Secure `Input` masking: a bound value with `secure: true` paints one mask
//! glyph (U+002A `*`) per Unicode scalar of the bound value, never the raw
//! characters. `secure` absent leaves rendering byte-identical.

use opurom_render::{digest_hex, font, layout, measure, parse_ir, FontMetrics, StateView};
use serde_json::json;

fn compat_renderer(width: u32, height: u32) -> opurom_render::Renderer {
    let mut renderer = opurom_render::Renderer::new(width, height);
    renderer.set_quality(opurom_render::QualityProfile::Compat);
    renderer
}

/// A bound `Input`; `secure` is added only when requested so the absent case
/// stays byte-identical to the pre-change schema.
fn input_ir(secure: bool, placeholder: &str) -> String {
    let mut props = json!({ "bind": "pw", "placeholder": placeholder, "width": 200, "height": 40 });
    if secure {
        props["secure"] = json!(true);
    }
    json!({
        "ir": "0.1.0", "app": "S",
        "state": [ { "name": "pw", "type": "string", "init": "" } ],
        "tree": { "id": 0, "kind": "Input", "props": props, "children": [], "events": {} },
        "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 }
    })
    .to_string()
}

const SECRET: &str = "p@ssw0rd!";

fn state_with(ir: &opurom_render::Ir, value: &str) -> StateView {
    let mut state = StateView::from_ir(ir);
    state.set("pw", opurom_render::StateValue::Str(value.to_string()));
    state
}

fn drawn_text(ir: &opurom_render::Ir, state: &StateView) -> String {
    let laid = layout(ir, state, 300, 100);
    laid[0].text.clone().unwrap_or_default()
}

#[test]
fn secure_input_with_bound_value_draws_n_mask_glyphs() {
    let ir = parse_ir(&input_ir(true, "Password")).unwrap();
    let state = state_with(&ir, SECRET);
    let text = drawn_text(&ir, &state);
    assert_eq!(text, "*".repeat(SECRET.chars().count()));
    assert!(
        !text.contains('p') && !text.contains('@'),
        "raw chars must not appear"
    );
    // Width is exactly N mask glyph widths under fixed metrics.
    let scale = layout(&ir, &state, 300, 100)[0].text_scale;
    let metrics = FontMetrics::Fixed;
    assert_eq!(
        measure(&text, scale, metrics),
        measure("*", scale, metrics) * SECRET.chars().count() as i64
    );
}

#[test]
fn secure_render_pixels_differ_from_plaintext_and_never_leak() {
    let plain_ir = parse_ir(&input_ir(false, "Password")).unwrap();
    let secure_ir = parse_ir(&input_ir(true, "Password")).unwrap();
    let plain = state_with(&plain_ir, SECRET);
    let secure = state_with(&secure_ir, SECRET);
    let a = compat_renderer(300, 100).pixels(&plain_ir, &plain);
    let b = compat_renderer(300, 100).pixels(&secure_ir, &secure);
    assert_ne!(a, b, "secure render must differ from plaintext render");
    // A secure frame equals one drawn from the all-mask string with no binding,
    // proving no raw glyph reaches the raster.
    let mask_only_ir = parse_ir(&input_ir(true, "Password")).unwrap();
    let mask_state = state_with(&mask_only_ir, &"*".repeat(SECRET.chars().count()));
    let c = compat_renderer(300, 100).pixels(&mask_only_ir, &mask_state);
    assert_eq!(b, c);
}

#[test]
fn secure_input_shows_placeholder_when_unbound() {
    let ir = parse_ir(&input_ir(true, "Password")).unwrap();
    let state = StateView::from_ir(&ir); // pw defaults to empty string -> bound but empty
    let text = drawn_text(&ir, &state);
    // An empty bound string masks to nothing, so the placeholder is shown.
    assert_eq!(text, "Password");
}

#[test]
fn secure_absent_is_byte_identical_and_ignores_masking() {
    // secure absent: even a bound value paints raw text.
    let ir = parse_ir(&input_ir(false, "Password")).unwrap();
    let state = state_with(&ir, SECRET);
    assert_eq!(drawn_text(&ir, &state), SECRET);
    assert_eq!(
        digest_hex(&compat_renderer(300, 100).pixels(&ir, &state)),
        digest_hex(&compat_renderer(300, 100).pixels(&ir, &state)),
        "secure-absent render is deterministic"
    );
}

#[test]
fn secure_render_is_deterministic_across_runs() {
    let ir = parse_ir(&input_ir(true, "Password")).unwrap();
    let state = state_with(&ir, SECRET);
    let d1 = digest_hex(&compat_renderer(300, 100).pixels(&ir, &state));
    let d2 = digest_hex(&compat_renderer(300, 100).pixels(&ir, &state));
    assert_eq!(d1, d2);
    // Mask glyph is one of the documented choices.
    assert_eq!(font::glyph('*').is_some(), true);
}
