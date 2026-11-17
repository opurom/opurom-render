//! Per-corner radii: named-object parsing, corner mapping, and raster mirroring.

use opurom_render::{
    geometry::RoundingRadii,
    layout, parse_ir,
    raster::{draw, Canvas},
    Ir, Rgba, StateView,
};

fn ir(corner: &str) -> String {
    format!(
        r##"{{
  "ir": "0.1.0", "app": "Corners", "state": [],
  "tree": {{ "id": 0, "kind": "Column", "props": {{ "padding": 20, "color": "#FFFFFF", "border": "#FFFFFF" }}, "events": {{}},
    "children": [{{ "id": 1, "kind": "Container", "props": {{ "width": 80, "height": 80, "color": "#000000", "border": "#000000", "radius": 0, "cornerRadii": {corner} }}, "events": {{}}, "children": [] }}] }},
  "theme": {{ "mode": "light", "accent": "#097FEB", "radius": 0 }}
}}"##
    )
}

fn radii(corner: &str) -> RoundingRadii {
    let ir = parse_ir(&ir(corner)).unwrap();
    let laid = layout(&ir, &StateView::from_ir(&ir), 120, 120);
    laid.iter().find(|n| n.id == 1).unwrap().radii
}

#[test]
fn named_object_maps_by_key_not_by_order() {
    let r = radii(r#"{"bottomRight":30,"topLeft":10,"bottomLeft":5,"topRight":20}"#);
    assert_eq!(r.top_left.x, 10);
    assert_eq!(r.top_right.x, 20);
    assert_eq!(r.bottom_left.x, 5);
    assert_eq!(r.bottom_right.x, 30);
}

#[test]
fn elliptical_pairs_and_explicit_zero_fall_back_per_corner() {
    let r = radii(r#"{"topLeft":[40,10],"topRight":0}"#);
    assert_eq!((r.top_left.x, r.top_left.y), (40, 10));
    assert_eq!(r.top_right.x, 0);
    // Unnamed corners fall back to the scalar radius (0 here).
    assert_eq!(r.bottom_left.x, 0);
    assert_eq!(r.bottom_right.x, 0);
}

#[test]
fn four_element_array_is_rejected() {
    let bad = ir("[10,20,30,40]");
    assert!(parse_ir(&bad).is_err());
}

#[test]
fn diagonal_corners_produce_mirrored_pixels() {
    let tl = parse_ir(&ir(r#"{"topLeft":40}"#)).unwrap();
    let br = parse_ir(&ir(r#"{"bottomRight":40}"#)).unwrap();
    let paint = |parsed: &Ir| {
        let laid = layout(parsed, &StateView::from_ir(parsed), 120, 120);
        let mut canvas = Canvas::new(120, 120, Rgba::rgb(255, 255, 255));
        draw(&mut canvas, laid.iter().find(|n| n.id == 1).unwrap());
        canvas.pixels
    };
    let a = paint(&tl);
    let b = paint(&br);
    let at = |buf: &[u8], x: usize, y: usize| buf[(y * 120 + x) * 4];
    // Swapping the rounded corner mirrors the two corner pixels; absolute
    // colours depend on the theme background, so compare across the pair.
    let (tl_a, br_a) = (at(&a, 21, 21), at(&a, 98, 98));
    let (tl_b, br_b) = (at(&b, 21, 21), at(&b, 98, 98));
    assert_ne!(tl_a, br_a, "a single rounded corner must be visible");
    assert_eq!(tl_a, br_b, "top-left rounding mirrors bottom-right");
    assert_eq!(br_a, tl_b, "bottom-right rounding mirrors top-left");
}

#[test]
fn absent_corner_radii_keeps_scalar_fallback() {
    let json = r##"{
  "ir": "0.1.0", "app": "Corners", "state": [],
  "tree": { "id": 0, "kind": "Column", "props": { "padding": 20, "color": "#FFFFFF", "border": "#FFFFFF" }, "events": {},
    "children": [{ "id": 1, "kind": "Container", "props": { "width": 80, "height": 80, "color": "#000000", "border": "#000000", "radius": 12 }, "events": {}, "children": [] }] },
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 0 }
}"##;
    let ir = parse_ir(json).unwrap();
    let laid = layout(&ir, &StateView::from_ir(&ir), 120, 120);
    let r = laid.iter().find(|n| n.id == 1).unwrap().radii;
    assert_eq!(r.top_left.x, 12);
    assert_eq!(r.top_right.x, 12);
    assert_eq!(r.bottom_left.x, 12);
    assert_eq!(r.bottom_right.x, 12);
}

#[test]
fn fixtures_map_all_four_corners_and_elliptical_pairs() {
    let four = std::fs::read_to_string("tests/fixtures/corner-radii-four.json").unwrap();
    let r = radii_from_json(&four);
    assert_eq!(
        (
            r.top_left.x,
            r.top_right.x,
            r.bottom_left.x,
            r.bottom_right.x
        ),
        (10, 20, 40, 30)
    );
    let ell = std::fs::read_to_string("tests/fixtures/corner-radii-elliptical.json").unwrap();
    let r = radii_from_json(&ell);
    assert_eq!((r.top_left.x, r.top_left.y), (40, 10));
    assert_eq!((r.bottom_right.x, r.bottom_right.y), (8, 24));
    assert_eq!(r.top_right.x, 4);
    assert_eq!(r.bottom_left.x, 4);
}

fn radii_from_json(json: &str) -> RoundingRadii {
    let parsed = parse_ir(json).unwrap();
    let laid = layout(&parsed, &StateView::from_ir(&parsed), 120, 120);
    laid.iter().find(|n| n.id == 1).unwrap().radii
}

#[test]
fn per_corner_display_list_json_uses_gpu_corner_order() {
    let parsed = parse_ir(&ir(
        r#"{"topLeft":10,"topRight":20,"bottomRight":30,"bottomLeft":5}"#,
    ))
    .unwrap();
    let list =
        opurom_render::Renderer::new(120, 120).display_list(&parsed, &StateView::from_ir(&parsed));
    let json = list.to_json().unwrap();
    assert!(
        json.contains("\"kind\":\"round_rect_per_corner\""),
        "{json}"
    );
    // GPU order is TL, TR, BL, BR; the IR writes BR before BL.
    assert!(
        json.contains("\"radii\":[[10,10],[20,20],[5,5],[30,30]]"),
        "{json}"
    );
}

#[test]
fn oversized_radii_are_clamped_for_gpu() {
    let parsed = parse_ir(&ir(r#"{"topLeft":40000}"#)).unwrap();
    let list =
        opurom_render::Renderer::new(120, 120).display_list(&parsed, &StateView::from_ir(&parsed));
    let json = list.to_json().unwrap();
    // Normalization scales the oversized radius down to the 80px box first,
    // so the wire value stays within the GPU limit without a clamp change.
    assert!(!json.contains("[40000,40000]"), "{json}");
    assert!(
        json.contains("\"kind\":\"round_rect_per_corner\""),
        "{json}"
    );
}

#[test]
fn border_and_gradient_respect_per_corner_radii() {
    let parsed = parse_ir(&ir(r#"{"topLeft":40}"#)).unwrap();
    let state = StateView::from_ir(&parsed);
    let paint = |gradient: bool, border_only: bool| {
        let laid = layout(&parsed, &state, 120, 120);
        let mut node = laid.iter().find(|n| n.id == 1).unwrap().clone();
        if gradient {
            node.effects.gradient = Some((
                opurom_render::Rgba::rgb(255, 255, 255),
                opurom_render::Rgba::rgb(0, 0, 0),
            ));
        }
        if border_only {
            node.fill = None;
            node.border_w = 2;
        }
        let mut canvas = Canvas::new(120, 120, Rgba::rgb(255, 255, 255));
        draw(&mut canvas, &node);
        canvas.pixels
    };
    for (gradient, border_only) in [(true, false), (false, true)] {
        let p = paint(gradient, border_only);
        let at = |x: usize, y: usize| p[(y * 120 + x) * 4];
        assert_ne!(
            at(21, 21),
            at(98, 98),
            "gradient={gradient} border={border_only}"
        );
    }
}
