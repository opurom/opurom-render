//! Built-in icon glyphs in the software renderer: every name in the canonical
//! set paints its vector glyph deterministically, icons render at small and
//! large node scales, an unknown name falls back to a visible neutral glyph,
//! and the three pinned golden frames stay byte-identical.

use opurom_render::{digest_hex, icons, layout, parse_ir, Renderer, StateView};

/// A single `Icon(name: ...)` node inside a padding-free column.
fn icon_ir(name: &str, size: i64) -> String {
    format!(
        r##"{{
  "ir": "0.1", "app": "IconProof",
  "state": [],
  "tree": {{ "id": 0, "kind": "Column", "props": {{ "gap": 0, "padding": 0, "align": "start" }},
    "children": [
      {{ "id": 1, "kind": "Icon",
        "props": {{ "name": "{name}", "color": "#101010", "width": {size}, "height": {size} }},
        "children": [], "events": {{}} }}
    ], "events": {{}} }},
  "theme": {{ "mode": "light", "accent": "#097FEB", "radius": 4 }}
}}"##
    )
}

/// The laid-out `Icon` node rect, so ink checks can be confined to the glyph.
fn icon_rect(ir: &opurom_render::Ir, state: &StateView, w: u32, h: u32) -> opurom_render::Rect {
    layout(ir, state, w, h)
        .into_iter()
        .find(|n| n.kind == opurom_render::Kind::Icon)
        .expect("an icon node is laid out")
        .rect
}

/// Pixels that are exactly the glyph colour: the icon fill is near-black and
/// the background is near-white, so only solid glyph interiors are 255.
fn exact_white_in_rect(pixels: &[u8], rect: opurom_render::Rect, width: i64, height: i64) -> usize {
    let mut count = 0usize;
    for y in rect.y.max(0)..rect.y.saturating_add(rect.h).min(height) {
        for x in rect.x.max(0)..rect.x.saturating_add(rect.w).min(width) {
            let i = (y as usize * width as usize + x as usize) * 4;
            if pixels[i] == 255 && pixels[i + 1] == 255 && pixels[i + 2] == 255 {
                count += 1;
            }
        }
    }
    count
}

#[test]
fn every_icon_paints_its_glyph_deterministically() {
    let w = 160u32;
    let h = 160u32;
    for name in icons::ICON_NAMES {
        let ir = parse_ir(&icon_ir(name, 64)).expect("parse icon IR");
        let state = StateView::from_ir(&ir);
        let rect = icon_rect(&ir, &state, w, h);
        let first = Renderer::new(w, h).pixels(&ir, &state);
        let second = Renderer::new(w, h).pixels(&ir, &state);
        assert_eq!(first, second, "icon {name} must render deterministically");
        assert!(
            exact_white_in_rect(&first, rect, w as i64, h as i64) > 0,
            "icon {name} painted no glyph ink inside its rect"
        );
    }
}

#[test]
fn icon_renders_at_small_and_large_node_scales() {
    let w = 200u32;
    let h = 200u32;
    for size in [16i64, 64] {
        let ir = parse_ir(&icon_ir("star", size)).expect("parse icon IR");
        let state = StateView::from_ir(&ir);
        let rect = icon_rect(&ir, &state, w, h);
        let pixels = Renderer::new(w, h).pixels(&ir, &state);
        assert!(
            exact_white_in_rect(&pixels, rect, w as i64, h as i64) > 0,
            "star at {size} painted no glyph ink"
        );
        assert_eq!(
            Renderer::new(w, h).pixels(&ir, &state),
            pixels,
            "star at {size} must render deterministically"
        );
    }
}

#[test]
fn unknown_icon_name_renders_the_fallback_without_panicking() {
    let w = 160u32;
    let h = 160u32;
    let ir = parse_ir(&icon_ir("definitely_not_an_icon", 64)).expect("parse icon IR");
    let state = StateView::from_ir(&ir);
    let rect = icon_rect(&ir, &state, w, h);
    let first = Renderer::new(w, h).pixels(&ir, &state);
    // The fallback glyph must still paint visible ink.
    assert!(
        exact_white_in_rect(&first, rect, w as i64, h as i64) > 0,
        "fallback icon painted nothing"
    );
    assert_eq!(
        Renderer::new(w, h).pixels(&ir, &state),
        first,
        "fallback icon must render deterministically"
    );
}

#[test]
fn the_three_pinned_golden_frames_are_unchanged() {
    let spec = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../opurom-spec/counter.opuir.json"
    );
    let ir = parse_ir(&std::fs::read_to_string(spec).expect("golden counter IR")).expect("parse");
    let mut state = StateView::from_ir(&ir);
    state.set("count", opurom_render::StateValue::Int(0));
    assert_eq!(
        digest_hex(&Renderer::new(320, 240).pixels(&ir, &state)),
        "16b69e96b1534324"
    );
    assert_eq!(
        digest_hex(&Renderer::new(800, 600).pixels(&ir, &state)),
        "aaf572509f15d60c"
    );
    state.set("count", opurom_render::StateValue::Int(7));
    assert_eq!(
        digest_hex(&Renderer::new(800, 600).pixels(&ir, &state)),
        "e5d71acb7602eab2"
    );
}

/// A `Row` of `Icon` nodes: no Container wrapper and no explicit width/height,
/// so each icon must lay out on its own intrinsic 16x16 size.
fn icon_row_ir(names: &[&str]) -> String {
    let kids = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            format!(
                "{{ \"id\": {id}, \"kind\": \"Icon\", \"props\": {{\"name\": \"{name}\", \"color\": \"#101010\"}}, \"children\": [], \"events\": {{}} }}",
                id = i + 1,
                name = name,
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        r##"{{
  "ir": "0.1", "app": "IconRow",
  "state": [],
  "tree": {{ "id": 0, "kind": "Row", "props": {{ "gap": 8, "align": "start" }},
    "children": [ {kids} ], "events": {{}} }},
  "theme": {{ "mode": "light", "accent": "#097FEB", "radius": 4 }}
}}"##
    )
}

#[test]
fn a_row_of_unwrapped_icons_keeps_intrinsic_size_and_draws_glyphs() {
    let w = 600u32;
    let h = 100u32;
    let ir =
        parse_ir(&icon_row_ir(&["star", "heart", "plus", "minus"])).expect("parse icon row IR");
    let state = StateView::from_ir(&ir);
    let laid = layout(&ir, &state, w, h);
    let icons: Vec<_> = laid
        .iter()
        .filter(|n| n.kind == opurom_render::Kind::Icon)
        .collect();
    assert_eq!(icons.len(), 4, "all icons must lay out inside the Row");
    // No Container wrapper: each icon sits beside the next at intrinsic size.
    let starts: Vec<i64> = icons.iter().map(|n| n.rect.x).collect();
    assert_eq!(
        starts,
        vec![0, 24, 48, 72],
        "icons sit side by side, not stretched"
    );
    for icon in &icons {
        assert_eq!(icon.rect.w, 16, "icon must keep its intrinsic width");
        assert_eq!(icon.rect.h, 16, "icon must keep its intrinsic height");
    }
    let pixels = Renderer::new(w, h).pixels(&ir, &state);
    for icon in &icons {
        let ink = exact_white_in_rect(&pixels, icon.rect, w as i64, h as i64);
        assert!(ink > 0, "icon at x={} drew no glyph ink", icon.rect.x);
    }
}
