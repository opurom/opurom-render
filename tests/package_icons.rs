//! Package icons: the renderer must consume the IR `icons` table and rasterise
//! the entry's own SVG path, falling back to the built-in glyph set only when
//! the name is not in the table. These tests pin that priority and the
//! deterministic output.

use opurom_render::{parse_ir, Renderer, StateView};

fn compat_renderer(width: u32, height: u32) -> opurom_render::Renderer {
    let mut renderer = opurom_render::Renderer::new(width, height);
    renderer.set_quality(opurom_render::QualityProfile::Compat);
    renderer
}

/// A 24x24 `disc` glyph: an outer ring and an inner dot (two full-circle arcs
/// plus a small filled circle drawn as arcs). Distinct from the built-in disc.
const DISC_D: &str = "M3 12a9 9 0 1 0 18 0a9 9 0 1 0 -18 0M9 12a3 3 0 1 0 6 0a3 3 0 1 0 -6 0";

fn ir_with_icon(name: &str, paths: &[&str], icon_name: &str) -> String {
    let paths_json = paths
        .iter()
        .map(|p| format!("{p:?}"))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r##"{{
  "ir": "0.1.0", "app": "Icons",
  "state": [],
  "theme": {{ "mode": "light", "accent": "#097FEB", "radius": 4, "surface": "#FFFFFF", "onSurface": "#000000" }},
  "icons": [ {{ "name": "{name}", "viewBox": "0 0 24 24", "paths": [{paths_json}] }} ],
  "tree": {{ "id": 0, "kind": "Column", "props": {{ "padding": 10, "gap": 8 }}, "children": [
    {{ "id": 1, "kind": "Icon", "props": {{ "name": "{icon_name}", "width": 64, "height": 64, "color": "#000000" }}, "children": [], "events": {{}} }}
  ], "events": {{}} }}
}}"##
    )
}

fn render(ir_text: &str) -> Vec<u8> {
    let ir = parse_ir(ir_text).unwrap();
    let state = StateView::from_ir(&ir);
    let renderer = compat_renderer(96, 96);
    renderer.pixels(&ir, &state)
}

/// Count non-transparent ink pixels and the max alpha, for a region.
fn ink(pixels: &[u8]) -> (usize, u8) {
    let mut count = 0;
    let mut max = 0u8;
    for px in pixels.chunks_exact(4) {
        // The frame is opaque; "ink" is a non-white pixel on the white surface.
        if px[0] < 250 || px[1] < 250 || px[2] < 250 {
            count += 1;
            max = max.max(255 - px[0]);
        }
    }
    (count, max)
}

#[test]
fn package_icon_paints_ink() {
    let pixels = render(&ir_with_icon("disc", &[DISC_D], "disc"));
    let (count, _) = ink(&pixels);
    assert!(count > 0, "a package icon must paint ink, not an empty box");
}

#[test]
fn package_path_wins_over_the_builtin_glyph() {
    // The built-in set has no `disc`, so it would fall back to the disc
    // placeholder. A package `disc` with a real path must paint differently
    // from the built-in fallback for the same name.
    let with_pkg = render(&ir_with_icon("disc", &[DISC_D], "disc"));
    // Same name, empty table: the node resolves no package icon and uses the
    // built-in fallback glyph.
    let fallback = render(&ir_with_icon("other", &["M1 1 L2 2"], "disc"));
    assert_ne!(
        with_pkg, fallback,
        "the package path must change the pixels versus the built-in fallback"
    );
}

#[test]
fn identical_ir_renders_identical_pixels() {
    let a = render(&ir_with_icon("disc", &[DISC_D], "disc"));
    let b = render(&ir_with_icon("disc", &[DISC_D], "disc"));
    assert_eq!(a, b, "package icon raster must be deterministic");
}

#[test]
fn unknown_package_name_degrades_to_builtin_set() {
    // A package table that does not contain the node's name: the node uses the
    // built-in glyph set (e.g. `home`), never a fabricated shape.
    let ir = ir_with_icon("disc", &[DISC_D], "home");
    let pixels = render(&ir);
    let (count, _) = ink(&pixels);
    assert!(count > 0, "the built-in `home` glyph must still paint");
}

/// The real music app IR, compiled by the sibling compiler when available. It
/// carries 13 Tabler package icons (`music`, `disc`, `player-play`, ...). The
/// test compiles it via `opuromc` from the sibling repo, skipping when the
/// toolchain is absent so the crate stays independent of the examples repo.
#[test]
fn music_app_renders_package_icons_natively() {
    use std::path::PathBuf;
    use std::process::Command;
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.parent().unwrap();
    let compiler = root.join("opurom-lang/target/debug/opuromc");
    let source = root.join("opurom-examples/apps/music/main.opu");
    if !compiler.exists() || !source.exists() {
        eprintln!("SKIP music app package-icon render: opuromc or music source absent");
        return;
    }
    let out = std::env::temp_dir().join(format!("opurom-music-ir-{}.json", std::process::id()));
    let status = Command::new(&compiler)
        .args([
            "compile",
            source.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ])
        .status();
    if !matches!(status, Ok(s) if s.success()) {
        eprintln!("SKIP music app package-icon render: compile failed");
        return;
    }
    let text = std::fs::read_to_string(&out).unwrap();
    let ir = parse_ir(&text).unwrap();
    assert!(!ir.icons.is_empty(), "music IR must carry package icons");
    assert!(ir.icons.contains_key("music"), "music icon must resolve");
    let state = StateView::from_ir(&ir);
    let renderer = compat_renderer(390, 1200);
    let pixels = renderer.pixels(&ir, &state);
    // The music icon sits in the header chip; its 16x16 box must carry ink that
    // is the package glyph, not an empty square.
    let laid = opurom_render::layout(&ir, &state, 390, 1200);
    // The header chip's music icon (top-left, y < 40) paints the package glyph
    // in white on the accent chip; assert that region carries package ink.
    let mut checked = 0;
    let mut painted = false;
    for n in &laid {
        if n.icon_name.as_deref() == Some("music") && n.icon_shape.is_some() && n.rect.y < 40 {
            checked += 1;
            for y in n.rect.y..n.rect.y + n.rect.h {
                for x in n.rect.x..n.rect.x + n.rect.w {
                    if x < 0 || y < 0 || x >= 390 || y >= 1200 {
                        continue;
                    }
                    let i = ((y as usize) * 390 + x as usize) * 4;
                    if pixels[i] > 150 && pixels[i + 1] > 150 && pixels[i + 2] > 150 {
                        painted = true;
                    }
                }
            }
        }
    }
    assert!(
        checked > 0,
        "the header music icon must carry a package shape"
    );
    assert!(painted, "the header music icon must paint package ink");
    let _ = std::fs::remove_file(&out);
}
