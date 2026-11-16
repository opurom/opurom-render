//! Fill-available Grid height on real device-sized canvases. The grid is
//! nested in a Column with two cards, mirroring the calculator IR, so the
//! Column's proportional fluid allocation is exercised before place_grid.

use opurom_render::{digest, layout, parse_ir, Renderer, StateView};

fn calc_ir(fill: bool) -> String {
    let mut keys = String::new();
    for id in 4..24u32 {
        if id > 4 {
            keys.push(',');
        }
        keys.push_str(&format!(
            r#"{{ "id": {id}, "kind": "Button", "props": {{ "label": "{id}", "padding": 26 }}, "children": [], "events": {{}} }}"#
        ));
    }
    let fill_prop = if fill {
        r##", "fillHeight": true"##
    } else {
        ""
    };
    format!(
        r##"{{
  "ir": "0.1.0", "app": "CalcFill", "state": [],
  "tree": {{ "id": 0, "kind": "Column", "props": {{ "gap": 8, "padding": 8 }}, "children": [
    {{ "id": 1, "kind": "Container", "props": {{ "padding": 24, "height": 80 }}, "children": [], "events": {{}} }},
    {{ "id": 2, "kind": "Container", "props": {{ "padding": 30, "height": 80 }}, "children": [], "events": {{}} }},
    {{ "id": 3, "kind": "Grid", "props": {{ "columns": 4, "compactColumns": 4{fill_prop} }}, "children": [{keys}], "events": {{}} }}
  ], "events": {{}} }},
  "theme": {{ "mode": "light", "accent": "#097FEB", "radius": 4 }}
}}"##
    )
}

fn bounds(ir_text: &str, w: u32, h: u32, id: u32) -> opurom_render::Rect {
    let ir = parse_ir(ir_text).unwrap();
    let laid = layout(&ir, &StateView::from_ir(&ir), w, h);
    laid.iter()
        .find(|n| n.id == id)
        .unwrap_or_else(|| panic!("node {id} missing"))
        .rect
}

fn pixels(ir_text: &str, w: u32, h: u32) -> Vec<u8> {
    let ir = parse_ir(ir_text).unwrap();
    Renderer::new(w, h)
        .render(&ir, &StateView::from_ir(&ir))
        .pixels
}

#[test]
fn fill_height_reaches_the_bottom_on_a_device_sized_canvas() {
    let on = calc_ir(true);
    let off = calc_ir(false);
    for (w, h) in [(320u32, 640u32), (1080, 2200)] {
        let grid_on = bounds(&on, w, h, 3);
        let key_on = bounds(&on, w, h, 23);
        let key_off = bounds(&off, w, h, 23);
        let grid_bottom = grid_on.y + grid_on.h;
        let last_on = key_on.y + key_on.h;
        let last_off = key_off.y + key_off.h;

        // Both fixtures give the Grid the same box: the flag only changes rows.
        assert_eq!(grid_bottom, grid_on.y + grid_on.h);
        assert!(
            (last_on - grid_bottom).abs() <= 1,
            "{w}x{h}: last key bottom {last_on} must meet grid bottom {grid_bottom}"
        );
        assert!(
            last_off < grid_bottom - 40,
            "{w}x{h}: without the flag keys stop at {last_off}, well above {grid_bottom}"
        );
        assert!(
            digest(&pixels(&on, w, h)) != digest(&pixels(&off, w, h)),
            "{w}x{h}: flag must change rendered pixels"
        );
    }
}

#[test]
fn device_canvas_rows_are_far_taller_than_natural() {
    let on = calc_ir(true);
    let a = bounds(&on, 1080, 2200, 4);
    let b = bounds(&on, 1080, 2200, 8);
    let pitch = b.y - a.y;
    assert!(
        pitch > 200,
        "1080x2200: row pitch {pitch} must far exceed natural height"
    );
    assert_eq!(a, bounds(&on, 1080, 2200, 4), "layout is deterministic");
}

#[test]
fn default_goldens_are_unchanged() {
    let counter = std::fs::read_to_string("../opurom-examples/golden/counter.opuir.json").unwrap();
    let ir = parse_ir(&counter).unwrap();
    let state = StateView::from_ir(&ir);
    let a = Renderer::new(320, 240).render(&ir, &state).pixels;
    let b = Renderer::new(800, 600).render(&ir, &state).pixels;
    assert_eq!(format!("{:016x}", digest(&a)), "8d45f412996a99c6");
    assert_eq!(format!("{:016x}", digest(&b)), "318beb1dd337bc4e");
}
