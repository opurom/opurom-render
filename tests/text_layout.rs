//! Text wrapping and clipping in the software renderer: a long label wraps
//! into lines that stay inside its node, overflowing lines are clipped, short
//! labels and the pinned golden frames are untouched.

use opurom_render::{digest_hex, font, layout, parse_ir, Renderer, StateValue, StateView};

const LONG_LABEL_IR: &str = r##"{
  "ir": "0.1", "app": "WrapProof",
  "state": [],
  "tree": { "id": 0, "kind": "Column", "props": { "gap": 0, "padding": 16, "align": "start" },
    "children": [
      { "id": 1, "kind": "Text",
        "props": { "value": "Alpha Beta Gamma Delta Epsilon", "width": 80, "height": 56,
                   "size": 16, "color": "#101010" },
        "children": [], "events": {} }
    ], "events": {} },
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 }
}"##;

/// A node so short that the wrapped lines must be clipped at its bottom edge.
const TALL_TEXT_CLIP_IR: &str = r##"{
  "ir": "0.1", "app": "ClipProof",
  "state": [],
  "tree": { "id": 0, "kind": "Column", "props": { "gap": 0, "padding": 16, "align": "start" },
    "children": [
      { "id": 1, "kind": "Text",
        "props": { "value": "Alpha Beta Gamma Delta Epsilon", "width": 80, "height": 20,
                   "size": 16, "color": "#101010" },
        "children": [], "events": {} }
    ], "events": {} },
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 }
}"##;

const SHORT_LABEL_IR: &str = r##"{
  "ir": "0.1", "app": "ShortProof",
  "state": [],
  "tree": { "id": 0, "kind": "Column", "props": { "gap": 0, "padding": 16, "align": "start" },
    "children": [
      { "id": 1, "kind": "Text",
        "props": { "value": "OK", "width": 120, "height": 40, "size": 16, "color": "#101010" },
        "children": [], "events": {} }
    ], "events": {} },
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 }
}"##;

/// Rows/columns containing the text ink, within the light background.
fn ink_rows(pixels: &[u8], w: u32, h: u32) -> Vec<(i64, i64)> {
    (0..h)
        .flat_map(|y| {
            (0..w).filter_map(move |x| {
                let i = ((y * w + x) as usize) * 4;
                let r = pixels[i];
                let g = pixels[i + 1];
                let b = pixels[i + 2];
                if r < 128 && g < 128 && b < 128 {
                    Some((x as i64, y as i64))
                } else {
                    None
                }
            })
        })
        .collect()
}

fn text_rect(ir: &opurom_render::Ir, state: &StateView, w: u32, h: u32) -> opurom_render::Rect {
    layout(ir, state, w, h)
        .into_iter()
        .find(|n| n.text.is_some())
        .expect("a text node is laid out")
        .rect
}

fn text_scale(ir: &opurom_render::Ir, state: &StateView, w: u32, h: u32) -> i64 {
    layout(ir, state, w, h)
        .into_iter()
        .find(|n| n.text.is_some())
        .expect("a text node is laid out")
        .text_scale
}

#[test]
fn long_label_wraps_into_multiple_lines_inside_its_node() {
    let ir = parse_ir(LONG_LABEL_IR).expect("parse long label IR");
    let state = StateView::from_ir(&ir);
    let renderer = Renderer::new(200, 160);
    let pixels = renderer.pixels(&ir, &state);
    let rect = text_rect(&ir, &state, 200, 160);
    let scale = text_scale(&ir, &state, 200, 160);

    let ink = ink_rows(&pixels, 200, 160);
    assert!(!ink.is_empty(), "the long label must paint ink");
    // More than one glyph row-block: wrapping produced stacked lines.
    let y_min = ink.iter().map(|&(_, y)| y).min().unwrap();
    let y_max = ink.iter().map(|&(_, y)| y).max().unwrap();
    assert!(
        (y_max - y_min) > font::GLYPH_H * scale,
        "a single line is only {} px tall, ink spans {y_min}..{y_max}",
        font::GLYPH_H * scale
    );
    // Every ink pixel stays inside the node rectangle.
    let x_min = ink.iter().map(|&(x, _)| x).min().unwrap();
    let x_max = ink.iter().map(|&(x, _)| x).max().unwrap();
    assert!(
        x_min >= rect.x && x_max < rect.x + rect.w,
        "ink leaks horizontally"
    );
    assert!(
        y_min >= rect.y && y_max < rect.y + rect.h,
        "ink leaks vertically"
    );
}

#[test]
fn overflowing_lines_are_clipped_to_the_node_rect() {
    let ir = parse_ir(TALL_TEXT_CLIP_IR).expect("parse clip IR");
    let state = StateView::from_ir(&ir);
    let renderer = Renderer::new(200, 160);
    let pixels = renderer.pixels(&ir, &state);
    let rect = text_rect(&ir, &state, 200, 160);

    let ink = ink_rows(&pixels, 200, 160);
    assert!(
        !ink.is_empty(),
        "a wrapped label must still paint its first lines"
    );
    for &(x, y) in &ink {
        assert!(
            x >= rect.x && x < rect.x + rect.w,
            "ink at x={x} outside node"
        );
        assert!(
            y >= rect.y && y < rect.y + rect.h,
            "ink at y={y} outside node"
        );
    }
}

#[test]
fn short_label_is_unchanged() {
    let ir = parse_ir(SHORT_LABEL_IR).expect("parse short label IR");
    let state = StateView::from_ir(&ir);
    let digest = digest_hex(&Renderer::new(200, 160).pixels(&ir, &state));
    assert_eq!(
        digest, "bbe890f7678bd4f6",
        "a short label must keep its historical frame"
    );
}

#[test]
fn golden_digests_are_unchanged_by_wrapping() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../opurom-spec/counter.opuir.json"
    );
    let src = std::fs::read_to_string(path).expect("golden counter IR is present");
    let ir = parse_ir(&src).expect("parse golden counter IR");

    let mut state = StateView::from_ir(&ir);
    state.set("count", StateValue::Int(0));
    let d0 = digest_hex(&Renderer::new(320, 240).pixels(&ir, &state));
    assert_eq!(d0, "16b69e96b1534324");
    let d0_800 = digest_hex(&Renderer::new(800, 600).pixels(&ir, &state));
    assert_eq!(d0_800, "aaf572509f15d60c");

    state.set("count", StateValue::Int(7));
    let d7_800 = digest_hex(&Renderer::new(800, 600).pixels(&ir, &state));
    assert_eq!(d7_800, "e5d71acb7602eab2");
}
