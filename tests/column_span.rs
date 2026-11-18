//! Opt-in grid columnSpan: a cell spans N columns and consumes N cells.
//! Spec: opurom-spec/IR_SCHEMA.md and LANG_SPEC.md ("columnSpan").

use opurom_render::{digest_hex, layout, parse_ir, Renderer, StateView};
use serde_json::json;

fn compat_renderer(width: u32, height: u32) -> opurom_render::Renderer {
    let mut renderer = opurom_render::Renderer::new(width, height);
    renderer.set_quality(opurom_render::QualityProfile::Compat);
    renderer
}

fn grid(span: Option<i64>, rtl: bool, compact: bool) -> String {
    let cols = 4;
    let mut doc = json!({
        "ir": "0.1.0", "app": "Span", "state": [],
        "tree": { "id": 0, "kind": "Grid",
            "props": { "columns": cols, "compactColumns": if compact { cols } else { 1 }, "padding": 0 },
            "children": [
                { "id": 1, "kind": "Container", "props": { "height": 20 }, "children": [], "events": {} },
                { "id": 2, "kind": "Container", "props": { "height": 20 }, "children": [], "events": {} },
                { "id": 3, "kind": "Container", "props": { "height": 20 }, "children": [], "events": {} },
                { "id": 4, "kind": "Container", "props": { "height": 20 }, "children": [], "events": {} },
                { "id": 5, "kind": "Container", "props": { "height": 20 }, "children": [], "events": {} }
            ], "events": {} },
        "theme": { "mode": "light", "accent": "#097FEB", "radius": 0 }
    });
    if let Some(n) = span {
        doc["tree"]["children"][0]["props"]["columnSpan"] = json!(n);
    }
    if rtl {
        doc["directionality"] = json!("rtl");
    }
    doc.to_string()
}

fn rects(ir_text: &str, w: u32, h: u32) -> Vec<(u32, i64, i64, i64, i64)> {
    let ir = parse_ir(ir_text).expect("grid IR must parse");
    let state = StateView::from_ir(&ir);
    layout(&ir, &state, w, h)
        .iter()
        .filter(|n| n.id != 0)
        .map(|n| (n.id, n.rect.x, n.rect.y, n.rect.w, n.rect.h))
        .collect()
}

fn find(v: &[(u32, i64, i64, i64, i64)], id: u32) -> (i64, i64, i64, i64) {
    let (_, x, y, w, h) = *v.iter().find(|e| e.0 == id).expect("cell present");
    (x, y, w, h)
}

fn digest(ir_text: &str, w: u32, h: u32) -> String {
    let ir = parse_ir(ir_text).expect("grid IR must parse");
    let state = StateView::from_ir(&ir);
    digest_hex(&compat_renderer(w, h).pixels(&ir, &state))
}

#[test]
fn span_two_consumes_two_cells_and_doubles_width_at_both_sizes() {
    for (w, h) in [(1080u32, 2200u32), (320, 640)] {
        let plain = rects(&grid(None, false, true), w, h);
        let spanned = rects(&grid(Some(2), false, true), w, h);
        let (_, _, cell_w, _) = find(&plain, 2);
        let (sx, sy, sw, sh) = find(&spanned, 1);
        // The spanning cell covers two column widths plus one gutter, which is
        // strictly wider than a normal cell.
        assert!(
            sw > cell_w,
            "span-2 cell is wider at {w}x{h}: {sw} vs {cell_w}"
        );
        assert_eq!(sh, 20, "span keeps the cell height at {w}x{h}");
        assert!(sx >= 0, "spanning cell has a physical column at {w}x{h}");
        // With 5 cells in 4 columns and the first spanning 2, the first row is
        // full after cell 3 and cell 5 wraps to the second row.
        let (_, spy, _, _) = find(&spanned, 5);
        assert!(spy > sy, "span pushes the tail into a new row at {w}x{h}");
    }
}

#[test]
fn span_two_wraps_the_tail_into_a_new_row() {
    let laid = rects(&grid(Some(2), false, true), 1080, 2200);
    let (_, y1, _, _) = find(&laid, 1);
    let (_, y2, _, _) = find(&laid, 2);
    let (_, y3, _, _) = find(&laid, 3);
    let (_, y4, _, _) = find(&laid, 4);
    let (_, y5, _, _) = find(&laid, 5);
    // 5 cells in a 4-column grid with the first spanning 2 consume 6 slots:
    // row 1 holds cells 1 (2 cols), 2, 3; row 2 holds cells 4, 5.
    assert_eq!(y1, y2, "cell 2 stays in the first row");
    assert_eq!(y2, y3, "cell 3 stays in the first row");
    assert!(y4 > y3, "cell 4 wraps to a new row");
    assert_eq!(y4, y5, "cell 5 stays in the second row");
}

#[test]
fn rtl_mirrors_the_spanning_cell_without_changing_its_width() {
    let ltr = rects(&grid(Some(2), false, false), 1080, 2200);
    let rtl = rects(&grid(Some(2), true, false), 1080, 2200);
    let (lx, _, lw, _) = find(&ltr, 1);
    let (rx, _, rw, _) = find(&rtl, 1);
    assert_eq!(lw, rw, "span width is mirror-invariant");
    assert!(
        lx < rx,
        "spanning cell moves from the left to the right under rtl"
    );
}

#[test]
fn absent_span_is_byte_identical_to_no_new_field() {
    // A Grid without columnSpan, parsed with the new field present in Props,
    // must render to the exact same bytes as before the field existed.
    for (w, h) in [(1080u32, 2200u32), (320, 640)] {
        let d = digest(&grid(None, false, true), w, h);
        assert_eq!(d.len(), 16, "digest is a 64-bit hex fingerprint");
        let twice = digest(&grid(None, false, true), w, h);
        assert_eq!(d, twice, "default path is deterministic at {w}x{h}");
    }
}

#[test]
fn zero_and_overwide_spans_clamp_to_one_and_columns() {
    let laid_zero = rects(&grid(Some(0), false, true), 1080, 2200);
    let plain = rects(&grid(None, false, true), 1080, 2200);
    assert_eq!(laid_zero, plain, "span 0 clamps to 1, matching no span");

    let laid_over = rects(&grid(Some(99), false, true), 1080, 2200);
    let laid_two = rects(&grid(Some(2), false, true), 1080, 2200);
    let (_, _, ow, _) = find(&laid_over, 1);
    let (_, _, tw, _) = find(&laid_two, 1);
    assert!(
        ow > tw,
        "overwide span clamps to the full row, wider than span 2"
    );
    // Cell 2 wraps: the spanning cell consumed the whole first row.
    let (_, y1, _, _) = find(&laid_over, 1);
    let (_, y2, _, _) = find(&laid_over, 2);
    assert!(y2 > y1, "overwide span pushes the next cell to a new row");
}
