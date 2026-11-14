//! Five-tier adaptive layout, the integer fluid grid and the resize diff.

use opurom_render::{
    digest_hex, fluid_grid, layout, parse_ir, Breakpoint, Direction, FluidGrid, Ir, LaidOut,
    Renderer, StateValue, StateView, MAX_GRID_COLUMNS, MIN_COLUMN_WIDTH,
};

/// Golden IR v0.1 from opurom-spec/counter.opuir.json (embedded so this crate
/// stays independent of the spec repository).
const COUNTER_IR: &str = r##"{
  "ir": "0.1",
  "app": "Counter",
  "state": [
    { "name": "count", "type": "int", "init": 0 }
  ],
  "tree": {
    "id": 0,
    "kind": "Column",
    "props": { "gap": 8, "padding": 16, "align": "center" },
    "children": [
      {
        "id": 1,
        "kind": "Text",
        "props": { "value": "Count: {count}", "size": 24 },
        "children": [],
        "events": {}
      },
      {
        "id": 2,
        "kind": "Row",
        "props": { "gap": 8 },
        "children": [
          {
            "id": 3,
            "kind": "Button",
            "props": { "label": "-" },
            "children": [],
            "events": { "tap": { "op": "add", "target": "count", "value": -1 } }
          },
          {
            "id": 4,
            "kind": "Button",
            "props": { "label": "+" },
            "children": [],
            "events": { "tap": { "op": "add", "target": "count", "value": 1 } }
          }
        ],
        "events": {}
      }
    ],
    "events": {}
  },
  "theme": { "mode": "system", "accent": "#097FEB", "radius": 12 }
}"##;

/// A grid with six cells and no explicit `columns`, so the fluid grid decides
/// the column count from the container width.
const FLUID_GRID_IR: &str = r##"{
  "ir": "0.1",
  "app": "Fluid",
  "state": [],
  "tree": {
    "id": 0,
    "kind": "Grid",
    "props": {},
    "children": [
      { "id": 1, "kind": "Text", "props": { "value": "a" }, "children": [], "events": {} },
      { "id": 2, "kind": "Text", "props": { "value": "b" }, "children": [], "events": {} },
      { "id": 3, "kind": "Text", "props": { "value": "c" }, "children": [], "events": {} },
      { "id": 4, "kind": "Text", "props": { "value": "d" }, "children": [], "events": {} },
      { "id": 5, "kind": "Text", "props": { "value": "e" }, "children": [], "events": {} },
      { "id": 6, "kind": "Text", "props": { "value": "f" }, "children": [], "events": {} }
    ],
    "events": {}
  },
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 }
}"##;

fn counter() -> (Ir, StateView) {
    let ir = parse_ir(COUNTER_IR).expect("golden IR must parse");
    let state = StateView::from_ir(&ir);
    (ir, state)
}

fn node<'a>(laid: &'a [LaidOut], id: u32) -> &'a LaidOut {
    laid.iter().find(|n| n.id == id).expect("node id missing")
}

// --------------------------------------------------------------- five tiers

#[test]
fn every_tier_boundary_selects_its_exact_tier() {
    // Compact < 600.
    assert_eq!(Breakpoint::from_width(0), Breakpoint::Compact);
    assert_eq!(Breakpoint::from_width(599), Breakpoint::Compact);
    // Medium < 900.
    assert_eq!(Breakpoint::from_width(600), Breakpoint::Medium);
    assert_eq!(Breakpoint::from_width(899), Breakpoint::Medium);
    // Expanded < 1200.
    assert_eq!(Breakpoint::from_width(900), Breakpoint::Expanded);
    assert_eq!(Breakpoint::from_width(1199), Breakpoint::Expanded);
    // Wide < 1600.
    assert_eq!(Breakpoint::from_width(1200), Breakpoint::Wide);
    assert_eq!(Breakpoint::from_width(1599), Breakpoint::Wide);
    // Ultra >= 1600.
    assert_eq!(Breakpoint::from_width(1600), Breakpoint::Ultra);
    assert_eq!(Breakpoint::from_width(4000), Breakpoint::Ultra);
}

#[test]
fn the_tier_order_is_strictly_increasing() {
    assert!(Breakpoint::Compact < Breakpoint::Medium);
    assert!(Breakpoint::Medium < Breakpoint::Expanded);
    assert!(Breakpoint::Expanded < Breakpoint::Wide);
    assert!(Breakpoint::Wide < Breakpoint::Ultra);
}

// --------------------------------------------------------------- fluid grid

#[test]
fn the_fluid_grid_formula_is_integer_only_and_exact() {
    // columns = clamp(width / 120, 1, 12), margin = clamp(width / 40, 8, 32),
    // gutter = clamp(width / 60, 8, 24).
    for width in [
        0i64, 1, 119, 120, 599, 600, 899, 900, 1199, 1200, 1599, 1600, 2400,
    ] {
        let g: FluidGrid = fluid_grid(width);
        assert_eq!(
            g.columns,
            (width / MIN_COLUMN_WIDTH).clamp(1, MAX_GRID_COLUMNS)
        );
        assert_eq!(g.margin, (width / 40).clamp(8, 32));
        assert_eq!(g.gutter, (width / 60).clamp(8, 24));
    }
}

#[test]
fn the_fluid_grid_clamps_at_both_ends() {
    // Narrow: one column, the smallest margin and gutter.
    let narrow = fluid_grid(0);
    assert_eq!(narrow.columns, 1);
    assert_eq!(narrow.margin, 8);
    assert_eq!(narrow.gutter, 8);
    // Wide: the largest column count, margin and gutter.
    let wide = fluid_grid(100_000);
    assert_eq!(wide.columns, MAX_GRID_COLUMNS);
    assert_eq!(wide.margin, 32);
    assert_eq!(wide.gutter, 24);
    // The column count saturates at the cap.
    assert_eq!(fluid_grid(1440).columns, 12);
    assert_eq!(fluid_grid(1439).columns, 11);
}

#[test]
fn the_fluid_grid_is_deterministic_across_calls() {
    for width in [320i64, 800, 1280, 1920] {
        assert_eq!(fluid_grid(width), fluid_grid(width));
    }
}

#[test]
fn the_fluid_grid_reaches_the_layout_of_a_real_grid_node() {
    let ir = parse_ir(FLUID_GRID_IR).expect("the grid IR must parse");
    let state = StateView::from_ir(&ir);
    // At 1600 px the container is 1600 wide: 13 columns by the raw formula,
    // capped at 12, so six cells form a single row.
    let laid = layout(&ir, &state, 1600, 900);
    let cells: Vec<&LaidOut> = (1..=6).map(|id| node(&laid, id)).collect();
    for cell in &cells {
        assert_eq!(cell.rect.y, cells[0].rect.y, "all cells share one row");
    }
    for pair in cells.windows(2) {
        assert!(
            pair[1].rect.x > pair[0].rect.x,
            "cells advance to the right"
        );
    }
    // At 320 px the formula gives two columns, so six cells wrap into three rows.
    let narrow = layout(&ir, &state, 320, 900);
    let a = node(&narrow, 1).rect;
    let b = node(&narrow, 2).rect;
    let c = node(&narrow, 3).rect;
    assert_eq!(a.y, b.y, "the first two cells share a row");
    assert!(c.y > a.y, "the third cell wraps to the next row");
}

#[test]
fn an_explicit_column_count_keeps_the_fixed_grid_at_every_tier() {
    let ir = parse_ir(&FLUID_GRID_IR.replace("\"props\": {}", "\"props\": { \"columns\": 3 }"))
        .expect("the grid IR must parse");
    let state = StateView::from_ir(&ir);
    for width in [320u32, 800, 1280, 1920] {
        let laid = layout(&ir, &state, width, 900);
        // Three columns: cells 1..3 share the first row.
        assert_eq!(node(&laid, 1).rect.y, node(&laid, 2).rect.y);
        assert_eq!(node(&laid, 2).rect.y, node(&laid, 3).rect.y);
        assert!(node(&laid, 4).rect.y > node(&laid, 1).rect.y);
    }
}

#[test]
fn the_fluid_grid_reaches_the_raster_output() {
    let ir = parse_ir(FLUID_GRID_IR).unwrap();
    let state = StateView::from_ir(&ir);
    // The wide viewport lays the six cells out in one row, the narrow one in
    // three rows, so the rendered pixels differ.
    let wide = Renderer::new(1600, 900).pixels(&ir, &state);
    let narrow = Renderer::new(320, 900).pixels(&ir, &state);
    assert_ne!(wide, narrow, "the fluid grid must change the pixels");
    assert_ne!(digest_hex(&wide), digest_hex(&narrow));
}

// ------------------------------------------------------------------ resizing

#[test]
fn a_resize_across_tiers_recomputes_the_layout() {
    let (ir, state) = counter();
    let mut renderer = Renderer::new(800, 600);
    let before = renderer.compute_layout(&ir, &state);
    renderer
        .resize(1600, 600)
        .expect("the resize must be accepted");
    let after = renderer.compute_layout(&ir, &state);
    assert_ne!(before, after, "a tier change must recompute the layout");
    assert_eq!(renderer.viewport(), (1600, 600));
}

#[test]
fn an_unchanged_viewport_yields_zero_dirty_rects() {
    let (ir, state) = counter();
    let mut renderer = Renderer::new(800, 600);
    assert_eq!(renderer.render(&ir, &state).dirty.len(), 1);
    assert!(renderer.render(&ir, &state).dirty.is_empty());
    // A resize to the same size is not a change either.
    renderer.resize(800, 600).unwrap();
    assert!(renderer.render(&ir, &state).dirty.is_empty());
}

#[test]
fn a_tier_crossing_resize_yields_a_nonempty_bounded_diff() {
    let (ir, state) = counter();
    let mut renderer = Renderer::new(800, 600);
    renderer.render(&ir, &state);
    renderer.resize(1600, 600).unwrap();
    let frame = renderer.render(&ir, &state);
    assert!(!frame.dirty.is_empty(), "a tier change must report a diff");
    assert!(frame.dirty.len() <= 64, "the dirty list must stay bounded");
    for r in &frame.dirty {
        assert!(r.w > 0 && r.h > 0);
        assert!(
            r.x + r.w <= 1600 && r.y + r.h <= 600,
            "rect {r:?} leaves the frame"
        );
    }
}

#[test]
fn the_resize_diff_covers_every_changed_pixel() {
    let (ir, state) = counter();
    let mut renderer = Renderer::new(800, 600);
    let before = renderer.render(&ir, &state);
    renderer.resize(1000, 600).unwrap();
    let after = renderer.render(&ir, &state);
    // Every changed pixel of the shared region is inside a reported rect.
    // The two frames have different row strides, so each pixel is read with
    // its own stride.
    for y in 0..600u32 {
        for x in 0..800u32 {
            let old = (y as usize * 800 + x as usize) * 4;
            let new = (y as usize * 1000 + x as usize) * 4;
            if before.pixels[old..old + 4] != after.pixels[new..new + 4] {
                assert!(
                    after
                        .dirty
                        .iter()
                        .any(|r| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h),
                    "changed pixel ({x},{y}) not covered by {:?}",
                    after.dirty
                );
            }
        }
    }
}

#[test]
fn a_grown_viewport_reports_the_new_strip() {
    let (ir, state) = counter();
    let mut renderer = Renderer::new(800, 600);
    renderer.render(&ir, &state);
    renderer.resize(1000, 600).unwrap();
    let frame = renderer.render(&ir, &state);
    assert!(
        frame.dirty.iter().any(|r| r.x >= 800),
        "the newly exposed columns must be dirty: {:?}",
        frame.dirty
    );
}

#[test]
fn a_shrunk_viewport_reports_only_the_surviving_region() {
    let (ir, state) = counter();
    let mut renderer = Renderer::new(1000, 600);
    renderer.render(&ir, &state);
    renderer.resize(800, 600).unwrap();
    let frame = renderer.render(&ir, &state);
    assert!(!frame.dirty.is_empty());
    for r in &frame.dirty {
        assert!(r.x + r.w <= 800, "rect {r:?} leaves the shrunk frame");
    }
}

#[test]
fn a_resize_to_a_taller_viewport_reports_the_new_rows() {
    let (ir, state) = counter();
    let mut renderer = Renderer::new(800, 600);
    renderer.render(&ir, &state);
    renderer.resize(800, 800).unwrap();
    let frame = renderer.render(&ir, &state);
    assert!(
        frame.dirty.iter().any(|r| r.y >= 600),
        "the newly exposed rows must be dirty: {:?}",
        frame.dirty
    );
}

/// Every pixel that differs between the two frames, or that the new surface
/// gained, must be inside a reported dirty rect.
#[test]
fn the_resize_diff_covers_every_changed_or_new_pixel() {
    let (ir, state) = counter();
    for (from, to) in [
        ((800u32, 600u32), (1000u32, 600u32)),
        ((800, 600), (800, 800)),
        ((800, 600), (1000, 800)),
        ((1000, 800), (800, 600)),
        ((1000, 800), (1600, 900)),
        ((1600, 900), (320, 240)),
    ] {
        let mut renderer = Renderer::new(from.0, from.1);
        let before = renderer.render(&ir, &state);
        renderer.resize(to.0, to.1).unwrap();
        let after = renderer.render(&ir, &state);
        assert!(
            !after.dirty.is_empty(),
            "{from:?} -> {to:?} must report a diff"
        );
        assert!(
            after.dirty.len() <= 64,
            "{from:?} -> {to:?} produced {} rects",
            after.dirty.len()
        );
        for r in &after.dirty {
            assert!(r.w > 0 && r.h > 0, "empty rect {r:?}");
            assert!(
                r.x + r.w <= to.0 && r.y + r.h <= to.1,
                "rect {r:?} leaves the {to:?} frame"
            );
        }
        // A pixel of the new frame is dirty when it differs from the old frame
        // at the same position, or when the old frame had no such pixel.
        for y in 0..to.1 {
            for x in 0..to.0 {
                let new = (y as usize * to.0 as usize + x as usize) * 4;
                let old = if x < from.0 && y < from.1 {
                    let o = (y as usize * from.0 as usize + x as usize) * 4;
                    before.pixels[o..o + 4] != after.pixels[new..new + 4]
                } else {
                    true
                };
                if old {
                    assert!(
                        after
                            .dirty
                            .iter()
                            .any(|r| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h),
                        "{from:?} -> {to:?}: pixel ({x},{y}) not covered by {:?}",
                        after.dirty
                    );
                }
            }
        }
    }
}

// ----------------------------------------------------------- golden stability

#[test]
fn the_three_golden_digests_are_unchanged() {
    let (ir, state) = counter();
    let mut small = Renderer::new(320, 240);
    assert_eq!(
        digest_hex(&small.render(&ir, &state).pixels),
        "8d45f412996a99c6"
    );
    let mut large = Renderer::new(800, 600);
    assert_eq!(
        digest_hex(&large.render(&ir, &state).pixels),
        "318beb1dd337bc4e"
    );
    let (ir, mut state) = counter();
    state.set("count", StateValue::Int(7));
    let mut seven = Renderer::new(800, 600);
    assert_eq!(
        digest_hex(&seven.render(&ir, &state).pixels),
        "40073cb95c912d80"
    );
}

#[test]
fn the_compact_and_medium_layouts_are_unchanged_by_the_new_tiers() {
    let (ir, state) = counter();
    // The compact row still collapses to a vertical stack.
    let compact = layout(&ir, &state, 320, 240);
    assert_eq!(node(&compact, 2).direction, Direction::Vertical);
    // The medium row still lays out horizontally.
    let medium = layout(&ir, &state, 800, 600);
    assert_eq!(node(&medium, 2).direction, Direction::Horizontal);
}

// ---------------------------------------------------------------- determinism

#[test]
fn two_runs_of_the_same_resize_sequence_agree() {
    let (ir, state) = counter();
    let sequence = [(320u32, 240u32), (800, 600), (1000, 700), (1600, 900)];
    let mut digests = Vec::new();
    for _ in 0..2 {
        let mut renderer = Renderer::new(320, 240);
        let mut run = Vec::new();
        for (w, h) in sequence {
            renderer.resize(w, h).unwrap();
            let frame = renderer.render(&ir, &state);
            run.push((digest_hex(&frame.pixels), frame.dirty.len(), (w, h)));
        }
        digests.push(run);
    }
    assert_eq!(digests[0], digests[1], "two runs must agree");
}

#[test]
fn the_fluid_grid_layout_is_identical_across_two_renderers() {
    let ir = parse_ir(FLUID_GRID_IR).unwrap();
    let state = StateView::from_ir(&ir);
    for width in [320u32, 900, 1280, 1920] {
        let a = layout(&ir, &state, width, 900);
        let b = layout(&ir, &state, width, 900);
        assert_eq!(a, b, "the layout must be deterministic at {width}");
    }
}
