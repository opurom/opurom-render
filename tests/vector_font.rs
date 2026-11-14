//! The high-resolution vector glyph set used at large scales: every common
//! character renders deterministically, scale-4 text is anti-aliased across
//! many coverage levels, and the block-font path (scale 1..=3) keeps the three
//! pinned golden frames byte-identical.

use opurom_render::{digest_hex, font, layout, parse_ir, Renderer, StateValue, StateView};

fn text_ir(value: &str, size: i64) -> String {
    format!(
        r##"{{
  "ir": "0.1", "app": "VectorProof", "state": [],
  "tree": {{ "id": 0, "kind": "Column", "props": {{ "padding": 6, "align": "start" }},
    "children": [
      {{ "id": 1, "kind": "Text",
        "props": {{ "value": "{value}", "size": {size}, "color": "#101010" }},
        "children": [], "events": {{}} }}
    ], "events": {{}} }},
  "theme": {{ "mode": "light", "accent": "#097FEB", "radius": 4 }}
}}"##
    )
}

/// Distinct gray levels below the background in a rendered frame (a proxy for
/// how much anti-aliasing a glyph produced).
fn sub_background_levels(pixels: &[u8]) -> usize {
    let mut seen = std::collections::BTreeSet::new();
    for i in (0..pixels.len()).step_by(4) {
        if pixels[i] < 250 {
            seen.insert(pixels[i]);
        }
    }
    seen.len()
}

fn scale_of(ir: &opurom_render::Ir, state: &StateView, w: u32, h: u32) -> i64 {
    layout(ir, state, w, h)
        .into_iter()
        .find(|n| n.text.is_some())
        .expect("a text node")
        .text_scale
}

#[test]
fn every_common_ascii_glyph_renders_deterministically() {
    // A run of every ASCII letter, digit and common punctuation must paint ink
    // through the high-resolution path and be identical across repeated renders.
    let charset: String = ('0'..='9')
        .chain('A'..='Z')
        .chain('a'..='z')
        .chain([',', '.', ':', '+', '-', '%'])
        .collect();
    let src = text_ir(&charset, 32); // size 32 -> scale 5 (smooth path)
    let ir = parse_ir(&src).expect("parse");
    let state = StateView::from_ir(&ir);
    assert!(
        scale_of(&ir, &state, 1200, 120) >= font::SMOOTH_MIN_SCALE,
        "sample must exercise the high-resolution path"
    );

    let first = Renderer::new(1200, 120).pixels(&ir, &state);
    let second = Renderer::new(1200, 120).pixels(&ir, &state);
    assert_eq!(first, second, "smooth text must be deterministic");
    assert!(
        sub_background_levels(&first) > 3,
        "a smooth run must paint anti-aliased ink"
    );

    // Each character on its own must be non-empty and stable.
    for ch in charset.chars() {
        let one = text_ir(&ch.to_string(), 32);
        let one_ir = parse_ir(&one).expect("parse single glyph");
        let one_state = StateView::from_ir(&one_ir);
        let a = Renderer::new(120, 120).pixels(&one_ir, &one_state);
        let b = Renderer::new(120, 120).pixels(&one_ir, &one_state);
        assert_eq!(a, b, "glyph {ch:?} render must be stable");
        assert!(sub_background_levels(&a) > 0, "glyph {ch:?} must paint ink");
    }
}

#[test]
fn scale_four_glyph_is_smoothly_anti_aliased() {
    // A curved glyph at scale 4 blends through many partial-coverage pixels
    // rather than a two-tone block upscale.
    let src = text_ir("o", 25); // (25+3)/7 = 4
    let ir = parse_ir(&src).expect("parse");
    let state = StateView::from_ir(&ir);
    assert_eq!(scale_of(&ir, &state, 200, 120), 4);
    let pixels = Renderer::new(200, 120).pixels(&ir, &state);

    // Count pixels that are neither full background (>=250) nor fully dark
    // (<=24): these are the anti-aliased edge pixels.
    let mut edge = 0usize;
    let mut levels = std::collections::BTreeSet::new();
    for i in (0..pixels.len()).step_by(4) {
        let g = pixels[i];
        if g < 250 {
            levels.insert(g);
        }
        if (25..250).contains(&g) {
            edge += 1;
        }
    }
    assert!(
        levels.len() >= 8,
        "scale-4 'o' should span many coverage levels, got {}",
        levels.len()
    );
    assert!(
        edge >= 8,
        "scale-4 'o' should have a soft edge, got {edge} partial pixels"
    );
}

#[test]
fn pinned_golden_frames_are_stable() {
    // The three canonical counter frames are pinned to the current rendering;
    // they render through the high-resolution glyph path at every scale the
    // counter program uses.
    let spec = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../opurom-spec/counter.opuir.json"
    );
    let src = std::fs::read_to_string(spec).expect("golden counter IR");
    let ir = parse_ir(&src).expect("parse golden");
    let mut state = StateView::from_ir(&ir);
    state.set("count", StateValue::Int(0));
    assert_eq!(
        digest_hex(&Renderer::new(320, 240).pixels(&ir, &state)),
        "8d45f412996a99c6"
    );
    assert_eq!(
        digest_hex(&Renderer::new(800, 600).pixels(&ir, &state)),
        "318beb1dd337bc4e"
    );
    state.set("count", StateValue::Int(7));
    assert_eq!(
        digest_hex(&Renderer::new(800, 600).pixels(&ir, &state)),
        "40073cb95c912d80"
    );
}

#[test]
fn every_scale_uses_the_high_resolution_glyphs() {
    // The high-resolution atlas covers every printable ASCII glyph and every
    // integer scale renders through it; the block font is only a fallback for
    // codepoints the atlas does not define.
    let small = parse_ir(&text_ir("Vector", 10)).unwrap();
    let big = parse_ir(&text_ir("Vector", 11)).unwrap();
    let small_state = StateView::from_ir(&small);
    let big_state = StateView::from_ir(&big);
    assert!(scale_of(&small, &small_state, 400, 80) >= font::SMOOTH_MIN_SCALE);
    assert!(scale_of(&big, &big_state, 400, 80) >= font::SMOOTH_MIN_SCALE);

    // Each glyph fills the shared mono advance, so the laid-out text width is
    // identical at every scale.
    let adv = font::advance(scale_of(&big, &big_state, 400, 80));
    let tw = layout(&big, &big_state, 400, 80)
        .into_iter()
        .find(|n| n.text.as_deref() == Some("Vector"))
        .expect("text node")
        .rect
        .w;
    assert_eq!(tw, "Vector".chars().count() as i64 * adv);
}
