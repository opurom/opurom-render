//! Native RTL mirror (PLATFORM_DELIVERY §8 / IR_ADDITIONS §15.2). Layout
//! geometry mirrors under root `directionality:"rtl"`; glyph runs do not.

use opurom_render::{layout, parse_ir, Align, StateView};
use serde_json::json;

fn compat_renderer(width: u32, height: u32) -> opurom_render::Renderer {
    let mut renderer = opurom_render::Renderer::new(width, height);
    renderer.set_quality(opurom_render::QualityProfile::Compat);
    renderer
}

const W: u32 = 800;
const H: u32 = 120;

fn ir_with(root: serde_json::Value, directionality: Option<&str>) -> String {
    let mut doc = root;
    if let Some(d) = directionality {
        doc["directionality"] = json!(d);
    }
    doc.to_string()
}

/// A horizontal Row of three fixed-width, gapped cells plus a right-aligned
/// text sibling; wide viewport keeps the Row horizontal (non-compact).
fn row_ir() -> serde_json::Value {
    json!({
        "ir": "0.1.0", "app": "Rtl", "state": [],
        "tree": { "id": 0, "kind": "Row", "props": { "padding": 0, "gap": 10 },
            "children": [
                { "id": 1, "kind": "Container", "props": { "width": 30, "height": 20, "color": "#FF0000" }, "children": [], "events": {} },
                { "id": 2, "kind": "Container", "props": { "width": 30, "height": 20, "color": "#00FF00" }, "children": [], "events": {} },
                { "id": 3, "kind": "Container", "props": { "width": 30, "height": 20, "color": "#0000FF" }, "children": [], "events": {} }
            ], "events": {} },
        "theme": { "mode": "light", "accent": "#097FEB", "radius": 0 }
    })
}

fn laid(root: serde_json::Value, dir: Option<&str>) -> Vec<opurom_render::LaidOut> {
    let ir = parse_ir(&ir_with(root, dir)).unwrap();
    let state = StateView::from_ir(&ir);
    layout(&ir, &state, W, H)
}

fn centers(nodes: &[opurom_render::LaidOut], id: u32) -> (i64, i64, i64) {
    let r = nodes.iter().find(|n| n.id == id).unwrap().rect;
    (r.x, r.x + r.w / 2, r.w)
}

#[test]
fn horizontal_row_reverses_child_order_but_keeps_sizes_and_gap() {
    let ltr = laid(row_ir(), None);
    let rtl = laid(row_ir(), Some("rtl"));
    // Source order 1,2,3 in ltr; rtl places them at mirrored visual slots.
    let (lx1, _, lw1) = centers(&ltr, 1);
    let (lx2, _, lw2) = centers(&ltr, 2);
    let (lx3, _, lw3) = centers(&ltr, 3);
    let (rx1, _, rw1) = centers(&rtl, 1);
    let (rx2, _, rw2) = centers(&rtl, 2);
    let (rx3, _, rw3) = centers(&rtl, 3);
    assert!(lx1 < lx2 && lx2 < lx3, "ltr keeps source order");
    assert!(rx1 > rx2 && rx2 > rx3, "rtl reverses row children");
    assert_eq!((lw1, lw2, lw3), (rw1, rw2, rw3), "child sizes preserved");
    // The three cells occupy the same visual x slots, just with swapped ids.
    assert_eq!(
        (lx1, lx2, lx3),
        (rx3, rx2, rx1),
        "visual slots mirror exactly"
    );
    // Gap (10) is preserved between every adjacent pair on both sides.
    assert_eq!(lx2 - (lx1 + lw1), rx2 - (rx3 + rw3));
}

#[test]
fn align_and_text_align_swap_start_end_but_not_center() {
    // Row align=End pushes the packed children to the far edge; rtl flips it.
    let mut end = row_ir();
    end["tree"]["props"]["align"] = json!("end");
    let ltr = laid(end.clone(), None);
    let rtl = laid(end, Some("rtl"));
    // Row main axis is x (child order), cross axis is y (align end/start).
    let ly = ltr.iter().find(|n| n.id == 1).unwrap().rect.y;
    let ry = rtl.iter().find(|n| n.id == 1).unwrap().rect.y;
    assert!(
        ly > ry,
        "cross align=end moves to the far edge in ltr, near in rtl"
    );
    assert_eq!(
        ly + ry + ltr.iter().find(|n| n.id == 1).unwrap().rect.h,
        H as i64,
        "cross start/end offsets mirror around the viewport"
    );

    // Explicit start/end text alignment flips; center is untouched.
    let text_ir = |a: &str| {
        json!({
            "ir": "0.1.0", "app": "Rtl", "state": [],
            "tree": { "id": 0, "kind": "Text", "props": { "value": "abc", "align": a },
                "children": [], "events": {} },
            "theme": { "mode": "light", "accent": "#097FEB", "radius": 0 }
        })
    };
    let a = |d: Option<&str>| laid(text_ir("start"), d)[0].text_align;
    assert_eq!(a(None), Align::Start);
    assert_eq!(a(Some("rtl")), Align::End);
    let c = |d: Option<&str>| laid(text_ir("center"), d)[0].text_align;
    assert_eq!(c(None), Align::Center);
    assert_eq!(c(Some("rtl")), Align::Center, "center must not flip");
}

#[test]
fn grid_reverses_columns_only_compact_and_one_column_unchanged() {
    let grid = json!({
        "ir": "0.1.0", "app": "Rtl", "state": [],
        "tree": { "id": 0, "kind": "Grid", "props": { "columns": 3, "padding": 0 },
            "children": [
                { "id": 1, "kind": "Container", "props": { "color": "#FF0000", "height": 20 }, "children": [], "events": {} },
                { "id": 2, "kind": "Container", "props": { "color": "#00FF00", "height": 20 }, "children": [], "events": {} },
                { "id": 3, "kind": "Container", "props": { "color": "#0000FF", "height": 20 }, "children": [], "events": {} }
            ], "events": {} },
        "theme": { "mode": "light", "accent": "#097FEB", "radius": 0 }
    });
    let ltr = laid(grid.clone(), None);
    let rtl = laid(grid.clone(), Some("rtl"));
    let l1 = ltr.iter().find(|n| n.id == 1).unwrap().rect.x;
    let l3 = ltr.iter().find(|n| n.id == 3).unwrap().rect.x;
    let r1 = rtl.iter().find(|n| n.id == 1).unwrap().rect.x;
    let r3 = rtl.iter().find(|n| n.id == 3).unwrap().rect.x;
    assert!(l1 < l3 && r1 > r3, "grid columns reverse under rtl");
    assert_eq!(l1, r3, "source col 1 takes the rightmost slot in rtl");

    // A one-column grid (compactColumns:1) has no visible mirror.
    let one = json!({
        "ir": "0.1.0", "app": "Rtl", "state": [],
        "tree": { "id": 0, "kind": "Grid", "props": { "columns": 1, "compactColumns": 1, "padding": 0 },
            "children": [
                { "id": 1, "kind": "Container", "props": { "color": "#FF0000", "height": 20 }, "children": [], "events": {} },
                { "id": 2, "kind": "Container", "props": { "color": "#00FF00", "height": 20 }, "children": [], "events": {} }
            ], "events": {} },
        "theme": { "mode": "light", "accent": "#097FEB", "radius": 0 }
    });
    let compact = laid(one.clone(), None);
    let compact_rtl = laid(one, Some("rtl"));
    let ids = |v: &[opurom_render::LaidOut]| v.iter().map(|n| (n.id, n.rect)).collect::<Vec<_>>();
    assert_eq!(ids(&compact), ids(&compact_rtl), "1-column grid unchanged");
}

#[test]
fn no_bidi_glyph_run_is_byte_identical_between_ltr_and_rtl() {
    use opurom_render::digest_hex;
    // Center alignment removes placement differences: the glyph run itself
    // must be identical, since v1 has no bidi/glyph reordering.
    let text = json!({
        "ir": "0.1.0", "app": "Rtl", "state": [],
        "tree": { "id": 0, "kind": "Text", "props": { "value": "abc DEF 123", "align": "center", "size": 24 },
            "children": [], "events": {} },
        "theme": { "mode": "light", "accent": "#097FEB", "radius": 0 }
    });
    let render = |d: Option<&str>| {
        let ir = parse_ir(&ir_with(text.clone(), d)).unwrap();
        let state = StateView::from_ir(&ir);
        compat_renderer(240, 60).pixels(&ir, &state)
    };
    let ltr = render(None);
    let rtl = render(Some("rtl"));
    assert_eq!(
        digest_hex(&ltr),
        digest_hex(&rtl),
        "v1 must not reorder glyphs: center-aligned runs are byte-identical"
    );
}

#[test]
fn compact_row_stacks_vertically_unchanged_under_rtl() {
    // 400px is Compact: Row folds to a vertical stack; RTL must not reverse it.
    let compact_ir = row_ir();
    let ltr_ir = parse_ir(&ir_with(compact_ir.clone(), None)).unwrap();
    let ltr_state = StateView::from_ir(&ltr_ir);
    let ltr = layout(&ltr_ir, &ltr_state, 400, 120);
    let rtl_ir = parse_ir(&ir_with(compact_ir, Some("rtl"))).unwrap();
    let rtl_state = StateView::from_ir(&rtl_ir);
    let rtl = layout(&rtl_ir, &rtl_state, 400, 120);
    // cross align (x) still mirrors, but vertical slot order and sizes do not.
    let ys = |v: &[opurom_render::LaidOut]| {
        v.iter()
            .filter(|n| n.id >= 1)
            .map(|n| (n.id, n.rect.y, n.rect.h))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        ys(&ltr),
        ys(&rtl),
        "compact vertical stack unchanged under RTL"
    );
}

#[test]
fn default_path_bytes_and_all_pinned_goldens_stay_identical() {
    use opurom_render::digest_hex;
    use std::{fs, path::PathBuf, process::Command};
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let counter = fs::read_to_string("../opurom-examples/golden/counter.opuir.json").unwrap();
    let ir = parse_ir(&counter).unwrap();
    let state = StateView::from_ir(&ir);
    assert_eq!(
        digest_hex(&compat_renderer(320, 240).render(&ir, &state).pixels),
        "8d45f412996a99c6"
    );
    assert_eq!(
        digest_hex(&compat_renderer(800, 600).render(&ir, &state).pixels),
        "318beb1dd337bc4e"
    );
    let ppm = dir.join("rtl-effects.ppm");
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
