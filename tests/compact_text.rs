//! Compact vertical stacks must never squeeze a text child below one glyph
//! line: the raster paints the full glyph band regardless of the rect height,
//! so short rects make adjacent siblings overprint. Wider tiers use height as
//! the cross axis and stay untouched.

use opurom_render::{digest_hex, parse_ir, Kind, Renderer, StateView};

const IR: &str = r##"{
  "ir": "0.1", "app": "CompactText",
  "state": [],
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 8 },
  "tree": { "id": 0, "kind": "Column", "props": { "gap": 6, "padding": 6 },
    "children": [
      { "id": 1, "kind": "Row", "props": { "height": 24, "gap": 2 },
        "children": [
          { "id": 2, "kind": "Text", "props": { "value": "Edge", "size": 14 }, "children": [], "events": {} },
          { "id": 3, "kind": "Text", "props": { "value": "gateway", "size": 14 }, "children": [], "events": {} },
          { "id": 4, "kind": "Text", "props": { "value": "82", "size": 14 }, "children": [], "events": {} }
        ], "events": {} }
    ], "events": {} }
}"##;

fn text_rects(width: u32, height: u32) -> Vec<(u32, i64, i64)> {
    let ir = parse_ir(IR).unwrap();
    let state = StateView::from_ir(&ir);
    let laid = opurom_render::layout(&ir, &state, width, height);
    laid.iter()
        .filter(|n| n.kind == Kind::Text)
        .map(|n| (n.id, n.rect.y, n.rect.h))
        .collect()
}

#[test]
fn compact_text_children_keep_a_full_line_and_do_not_overlap() {
    let rects = text_rects(320, 200);
    assert_eq!(rects.len(), 3, "all three text children are laid out");
    // size 14 -> scale 2, GLYPH_H 7 -> a 14 px band.
    for (id, _, h) in &rects {
        assert!(*h >= 14, "node {id} must keep at least one glyph line, got {h}");
    }
    for pair in rects.windows(2) {
        let (_, y0, h0) = pair[0];
        let (id1, y1, _) = pair[1];
        assert!(
            y1 >= y0 + h0,
            "node {id1} starts at {y1} before the previous line ends at {}",
            y0 + h0
        );
    }
}

#[test]
fn a_wide_row_is_unchanged_by_the_compact_floor() {
    // At 800 px the Row is horizontal: height is the cross axis and each text
    // keeps its natural line height. This pins the non-compact path.
    let compact = {
        let ir = parse_ir(IR).unwrap();
        let state = StateView::from_ir(&ir);
        digest_hex(&Renderer::new(320, 200).pixels(&ir, &state))
    };
    let wide = {
        let ir = parse_ir(IR).unwrap();
        let state = StateView::from_ir(&ir);
        digest_hex(&Renderer::new(800, 200).pixels(&ir, &state))
    };
    assert_ne!(compact, wide, "compact and wide render differently");
    for (_, _, h) in text_rects(800, 200) {
        assert!(h >= 14, "wide text also keeps a full line");
    }
}
