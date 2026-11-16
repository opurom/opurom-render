//! Icon path precision at magnified sizes: sub-unit SVG control points must
//! survive parsing and hit-testing, so a 72px glyph shows partial edge
//! coverage instead of whole-icon-unit blocks.

use opurom_render::{parse_ir, Renderer, StateView};

// Genuine outline glyph paths from the icon package (viewBox 0 0 24 24),
// copied verbatim from the shipped SVGs; the `M0 0h24v24H0z` clip rect and
// an empty path are excluded the same way the IR exporter filters them.
const A_B_2: &[&str] = &[
    "M16 21h3c.81 0 1.48 -.67 1.48 -1.48l.02 -.02c0 -.82 -.69 -1.5 -1.5 -1.5h-3v3",
    "M16 15h2.5c.84 -.01 1.5 .66 1.5 1.5s-.66 1.5 -1.5 1.5h-2.5v-3",
    "M4 9v-4c0 -1.036 .895 -2 2 -2s2 .964 2 2v4",
    "M2.99 11.98a9 9 0 0 0 9 9m9 -9a9 9 0 0 0 -9 -9",
    "M8 7h-4",
];
const INFO_CIRCLE: &[&str] = &[
    "M3 12a9 9 0 1 0 18 0a9 9 0 0 0 -18 0",
    "M12 9h.01",
    "M11 12h1v4h1",
];
const CODE: &[&str] = &["M7 8l-4 4l4 4", "M17 8l4 4l-4 4", "M14 4l-4 16"];
const DEVICES: &[&str] = &[
    "M13 9a1 1 0 0 1 1 -1h6a1 1 0 0 1 1 1v10a1 1 0 0 1 -1 1h-6a1 1 0 0 1 -1 -1v-10",
    "M18 8v-3a1 1 0 0 0 -1 -1h-13a1 1 0 0 0 -1 1v12a1 1 0 0 0 1 1h9",
    "M16 9h2",
];

const SIZE: i64 = 72;
const CANVAS: u32 = 96;

fn package_ir(name: &str, paths: &[&str]) -> String {
    let list = paths
        .iter()
        .map(|p| format!("{p:?}"))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r##"{{
  "ir": "0.1.0", "app": "IconPrecision", "state": [],
  "theme": {{ "mode": "light", "accent": "#097FEB", "radius": 4, "surface": "#FFFFFF", "onSurface": "#000000" }},
  "icons": [ {{ "name": "{name}", "viewBox": "0 0 24 24", "paths": [{list}] }} ],
  "tree": {{ "id": 0, "kind": "Column", "props": {{ "padding": 12 }},
    "children": [
      {{ "id": 1, "kind": "Icon",
        "props": {{ "name": "{name}", "width": {SIZE}, "height": {SIZE}, "color": "#000000" }},
        "children": [], "events": {{}} }}
    ], "events": {{}} }}
}}"##
    )
}

fn builtin_ir(name: &str) -> String {
    format!(
        r##"{{
  "ir": "0.1.0", "app": "IconPrecision", "state": [],
  "theme": {{ "mode": "light", "accent": "#097FEB", "radius": 4, "surface": "#FFFFFF", "onSurface": "#000000" }},
  "tree": {{ "id": 0, "kind": "Column", "props": {{ "padding": 12 }},
    "children": [
      {{ "id": 1, "kind": "Icon",
        "props": {{ "name": "{name}", "width": {SIZE}, "height": {SIZE}, "color": "#000000" }},
        "children": [], "events": {{}} }}
    ], "events": {{}} }}
}}"##
    )
}

/// Render one 72px glyph on white.
fn render(ir_text: &str) -> Vec<u8> {
    let ir = parse_ir(ir_text).expect("icon IR must parse");
    let state = StateView::from_ir(&ir);
    Renderer::new(CANVAS, CANVAS).render(&ir, &state).pixels
}

/// Glyph-edge statistics inside the whole canvas: pixels that are neither
/// solid ink nor plain background (partial coverage), and how many distinct
/// grey levels they span.
fn edge_stats(pixels: &[u8]) -> (usize, usize) {
    let mut intermediate = 0usize;
    let mut levels = std::collections::BTreeSet::new();
    for px in pixels.chunks_exact(4) {
        let v = px[0].max(px[1]).max(px[2]);
        if v > 0 && v < 255 {
            intermediate += 1;
            levels.insert(v);
        }
    }
    (intermediate, levels.len())
}

/// The package `a-b-2` glyph: fractional cubic controls (`.81`, `1.48`,
/// `1.036`, `.895`) and a fractional arc start (`2.99`). Whole-unit rounding
/// collapses every 4x4 subsample onto one icon unit and yields binary edges;
/// sub-unit sampling must produce partial coverage.
#[test]
fn fractional_package_curves_keep_subunit_coverage_at_72px() {
    let pixels = render(&package_ir("a-b-2", A_B_2));
    let (intermediate, levels) = edge_stats(&pixels);
    assert!(
        intermediate >= 50,
        "a-b-2 at 72px must show partial edge coverage, got {intermediate} pixels"
    );
    assert!(
        levels >= 4,
        "edge must span several grey levels for smooth curves, got {levels}"
    );
}

/// Package arcs (`info-circle` r=9, `devices` r=1) render as smooth rings and
/// rounded rectangles at 72px, not blocky arcs.
#[test]
fn package_arc_paths_are_smooth_at_72px() {
    for (name, paths) in [("info-circle", INFO_CIRCLE), ("devices", DEVICES)] {
        let pixels = render(&package_ir(name, paths));
        let (intermediate, levels) = edge_stats(&pixels);
        assert!(
            intermediate >= 50,
            "{name} at 72px must show partial edge coverage, got {intermediate} pixels"
        );
        assert!(levels >= 4, "{name} edge levels got {levels}");
    }
}

/// The builtin glyph set samples on the same fixed-point grid, so its strokes
/// also gain partial coverage at magnified sizes.
#[test]
fn builtin_glyphs_are_smooth_at_72px() {
    for name in ["home", "star"] {
        let pixels = render(&builtin_ir(name));
        let (intermediate, levels) = edge_stats(&pixels);
        assert!(
            intermediate >= 50,
            "builtin {name} at 72px must show partial coverage, got {intermediate}"
        );
        assert!(levels >= 4, "builtin {name} edge levels got {levels}");
    }
}

/// Precision changes nothing about determinism, and a magnified package glyph
/// still differs from the built-in fallback for the same name.
#[test]
fn precision_output_is_deterministic_and_package_specific() {
    for (name, paths) in [
        ("a-b-2", A_B_2),
        ("info-circle", INFO_CIRCLE),
        ("code", CODE),
        ("devices", DEVICES),
    ] {
        let ir = package_ir(name, paths);
        assert_eq!(
            render(&ir),
            render(&ir),
            "{name} must render deterministically"
        );
        // Same name absent from the table: the built-in fallback (a
        // different shape) must produce different pixels.
        let fallback = render(&builtin_ir(name));
        assert_ne!(
            render(&ir),
            fallback,
            "{name}: the package path must drive the pixels"
        );
    }
}

/// A small glyph keeps painting ink after the precision change (16px box).
#[test]
fn package_glyph_still_paints_at_small_sizes() {
    let small = package_ir("info-circle", INFO_CIRCLE)
        .replace(&format!("\"width\": {SIZE}"), "\"width\": 16")
        .replace(&format!("\"height\": {SIZE}"), "\"height\": 16");
    let ir = parse_ir(&small).expect("small icon IR must parse");
    let state = StateView::from_ir(&ir);
    let pixels = Renderer::new(40, 40).render(&ir, &state).pixels;
    let ink = pixels
        .chunks_exact(4)
        .filter(|px| px[0].max(px[1]).max(px[2]) < 250)
        .count();
    assert!(ink > 0, "info-circle at 16px must still paint ink");
}
