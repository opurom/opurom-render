//! Incremental dirty-region raster: reuse the previous framebuffer and redraw
//! only the nodes that changed. Every incremental frame is byte-identical to
//! the full reference, while unchanged regions skip all rasterization work.

use opurom_render::{parse_ir, Renderer, StateValue, StateView, SETTLED_MS};

const IR: &str = r##"{
  "ir": "0.1", "app": "IncrementalProof",
  "state": [ { "name": "count", "type": "int", "init": 0 } ],
  "tree": { "id": 0, "kind": "Column", "props": { "gap": 12, "padding": 24, "align": "start" },
    "children": [
      { "id": 1, "kind": "Text",
        "props": { "value": "Static Header", "size": 22, "color": "#101010" },
        "children": [], "events": {} },
      { "id": 2, "kind": "Text",
        "props": { "value": "Body line one", "size": 16, "color": "#202020" },
        "children": [], "events": {} },
      { "id": 3, "kind": "Text",
        "props": { "value": "N: {count}", "size": 24, "color": "#097FEB" },
        "children": [], "events": {} }
    ], "events": {} },
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 }
}"##;

const IR_WITH_EFFECT: &str = r##"{
  "ir": "0.1", "app": "IncrementalEffectProof",
  "state": [ { "name": "count", "type": "int", "init": 0 } ],
  "tree": { "id": 0, "kind": "Column", "props": { "gap": 12, "padding": 24, "align": "start" },
    "children": [
      { "id": 1, "kind": "Text",
        "props": { "value": "N: {count}", "size": 24, "color": "#101010",
                   "effects": [{"kind":"blur","radius":3}] },
        "children": [], "events": {} }
    ], "events": {} },
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 }
}"##;

fn state_at(ir: &opurom_render::Ir, v: i64) -> StateView {
    let mut s = StateView::from_ir(ir);
    s.set("count", StateValue::Int(v));
    s
}

#[test]
fn incremental_pixels_match_full_reference_every_frame() {
    let ir = parse_ir(IR).expect("parse");
    let states = [0i64, 1, 7, 2, 7, 7, 3];
    let mut full = Renderer::new(400, 300);
    let mut inc = Renderer::new(400, 300);
    let mut full_frames = Vec::new();
    let mut inc_frames = Vec::new();
    for v in states {
        let s = state_at(&ir, v);
        full_frames.push(full.render_at(&ir, &s, SETTLED_MS));
        inc_frames.push(inc.render_incremental_at(&ir, &s, SETTLED_MS));
    }
    for (i, (f, g)) in full_frames.iter().zip(&inc_frames).enumerate() {
        assert_eq!(
            f.pixels, g.pixels,
            "frame {i} pixels differ from the full reference"
        );
        assert_eq!(
            f.dirty, g.dirty,
            "frame {i} dirty list differs from the full reference"
        );
        assert_eq!(f.width, g.width);
        assert_eq!(f.height, g.height);
    }
}

#[test]
fn first_frame_is_a_full_raster() {
    let ir = parse_ir(IR).expect("parse");
    let mut r = Renderer::new(200, 150);
    let s = state_at(&ir, 0);
    let frame = r.render_incremental_at(&ir, &s, SETTLED_MS);
    assert_eq!(frame.dirty.len(), 1, "a first frame is fully dirty");
    let (nodes, pixels) = r.redraw_stats();
    assert_eq!(nodes, r.compute_layout(&ir, &s).len());
    assert_eq!(pixels, 200u64 * 150);
}

#[test]
fn unchanged_rerender_redraws_nothing() {
    let ir = parse_ir(IR).expect("parse");
    let mut r = Renderer::new(300, 200);
    let s = state_at(&ir, 5);
    let first = r.render_incremental_at(&ir, &s, SETTLED_MS);
    assert!(r.redraw_stats().0 > 0, "the first frame must redraw");
    let again = r.render_incremental_at(&ir, &s, SETTLED_MS);
    assert_eq!(
        again.pixels, first.pixels,
        "an unchanged re-render must reuse the framebuffer"
    );
    assert!(
        again.dirty.is_empty(),
        "an unchanged re-render must report no dirty rects"
    );
    assert_eq!(
        r.redraw_stats(),
        (0, 0),
        "an unchanged re-render must not recompute any pixel"
    );
}

#[test]
fn localized_change_redraws_fewer_nodes_than_full() {
    let ir = parse_ir(IR).expect("parse");
    let mut r = Renderer::new(400, 300);
    let s0 = state_at(&ir, 0);
    r.render_incremental_at(&ir, &s0, SETTLED_MS);
    // A second render with the same state settles to zero redraw work.
    r.render_incremental_at(&ir, &s0, SETTLED_MS);
    assert_eq!(r.redraw_stats(), (0, 0), "seed must be settled");

    let s1 = state_at(&ir, 7);
    let frame = r.render_incremental_at(&ir, &s1, SETTLED_MS);
    let (nodes, pixels) = r.redraw_stats();
    let total = r.compute_layout(&ir, &s1).len();
    assert!(
        nodes > 0 && nodes < total,
        "a localized change must redraw some nodes but not all ({nodes}/{total})"
    );
    assert!(
        pixels > 0 && pixels < 400u64 * 300,
        "a localized change must redraw some pixels but not the whole frame ({pixels})"
    );

    let mut ref_r = Renderer::new(400, 300);
    ref_r.render_at(&ir, &s0, SETTLED_MS);
    let ref_frame = ref_r.render_at(&ir, &s1, SETTLED_MS);
    assert_eq!(
        frame.pixels, ref_frame.pixels,
        "incremental must match the full reference"
    );
    assert_eq!(
        frame.dirty, ref_frame.dirty,
        "dirty list must match the full reference"
    );
}

#[test]
fn effect_frame_falls_back_to_a_full_raster() {
    let ir = parse_ir(IR_WITH_EFFECT).expect("parse");
    let mut r = Renderer::new(300, 200);
    let s0 = state_at(&ir, 0);
    r.render_incremental_at(&ir, &s0, SETTLED_MS);
    let s1 = state_at(&ir, 7);
    let frame = r.render_incremental_at(&ir, &s1, SETTLED_MS);
    let (nodes, pixels) = r.redraw_stats();
    assert_eq!(
        nodes,
        r.compute_layout(&ir, &s1).len(),
        "an effect frame must rasterize every node"
    );
    assert_eq!(pixels, 300u64 * 200);

    let mut ref_r = Renderer::new(300, 200);
    ref_r.render_at(&ir, &s0, SETTLED_MS);
    let ref_frame = ref_r.render_at(&ir, &s1, SETTLED_MS);
    assert_eq!(frame.pixels, ref_frame.pixels);
    assert_eq!(frame.dirty, ref_frame.dirty);
}

#[test]
fn resize_still_full_renders() {
    let ir = parse_ir(IR).expect("parse");
    let mut r = Renderer::new(200, 150);
    let s = state_at(&ir, 3);
    let first = r.render_incremental_at(&ir, &s, SETTLED_MS);
    assert_eq!(first.dirty.len(), 1);

    r.resize(300, 220).expect("resize");
    let frame = r.render_incremental_at(&ir, &s, SETTLED_MS);
    let (nodes, pixels) = r.redraw_stats();
    assert_eq!(
        nodes,
        r.compute_layout(&ir, &s).len(),
        "a resize must rasterize every node"
    );
    assert_eq!(pixels, 300u64 * 220);

    let mut ref_r = Renderer::new(200, 150);
    ref_r.render_at(&ir, &s, SETTLED_MS);
    ref_r.resize(300, 220).expect("resize");
    let ref_frame = ref_r.render_at(&ir, &s, SETTLED_MS);
    assert_eq!(
        frame.pixels, ref_frame.pixels,
        "resize output must match the full reference"
    );
    assert_eq!(frame.dirty, ref_frame.dirty);
}
