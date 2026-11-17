//! M2 material elevation table (DEFAULT_UI_GLASS_REFINEMENT.md §12.5): the
//! embedded table matches opurom-ui/material-tokens.json verbatim, the backend
//! switch changes node shadows, and every default/capture path stays pinned.

use opurom_render::{digest_hex, elevation_level, parse_ir, Renderer, StateView, ELEVATION_TABLE};
use serde_json::Value;

const TOKENS: &str = "../opurom-ui/theme/material-tokens.json";
const COUNTER: &str = "../opurom-examples/golden/counter.opuir.json";

fn alpha_of(hex: &str) -> u8 {
    // #AARRGGBB -> AA
    u8::from_str_radix(&hex[1..3], 16).unwrap()
}

#[test]
fn embedded_table_matches_material_tokens_json() {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(TOKENS).unwrap()).unwrap();
    let elev = &v["tokens"]["elevation"];
    for level in 0..=6 {
        let Some(layers) = ELEVATION_TABLE[level] else {
            assert!(
                elev.get(format!("md-elev-{level}")).is_none() || level == 0,
                "level {level} absent in table but present in JSON"
            );
            continue;
        };
        let key = format!("md-elev-{level}");
        let node = &elev[&key];
        // Table order: [ambient, penumbra, umbra]; JSON keys match by name.
        for (layer, name) in layers.iter().zip(["ambient", "penumbra", "umbra"]) {
            let j = &node[name];
            let off = j["offset"].as_array().unwrap();
            let (dx, dy, blur, spread, alpha) = *layer;
            assert_eq!(dx, off[0].as_i64().unwrap(), "{key}.{name}.offset.x");
            assert_eq!(dy, off[1].as_i64().unwrap(), "{key}.{name}.offset.y");
            assert_eq!(blur, j["blur"].as_i64().unwrap(), "{key}.{name}.blur");
            assert_eq!(spread, j["spread"].as_i64().unwrap(), "{key}.{name}.spread");
            assert_eq!(
                alpha,
                alpha_of(j["color"].as_str().unwrap()),
                "{key}.{name}.color alpha"
            );
        }
    }
    // Level 5 does not exist in either; 0 is empty.
    assert!(ELEVATION_TABLE[5].is_none());
    assert!(ELEVATION_TABLE[0].is_none());
}

#[test]
fn level_mapping_matches_spec() {
    assert_eq!(elevation_level(0), 0);
    assert_eq!(elevation_level(1), 1);
    assert_eq!(elevation_level(4), 4);
    assert_eq!(elevation_level(5), 4, "5 falls back to 4");
    assert_eq!(elevation_level(6), 6);
    assert_eq!(elevation_level(7), 6, "7 falls back to 6");
    assert_eq!(elevation_level(8), 6, "8 falls back to 6");
}

fn box_ir(shadow: i64) -> String {
    format!(
        r##"{{
        "ir": "0.1.0", "app": "Elev", "state": [],
        "tree": {{ "id": 0, "kind": "Column", "props": {{ "padding": 8 }},
            "children": [ {{ "id": 1, "kind": "Container", "props": {{
                "color": "#102030", "width": 60, "height": 40, "radius": 4, "shadow": {shadow} }},
                "children": [], "events": {{}} }} ], "events": {{}} }},
        "theme": {{ "mode": "light", "accent": "#097FEB", "radius": 4 }}
    }}"##
    )
}

fn render(ir_text: &str, elevation: bool) -> Vec<u8> {
    let ir = parse_ir(ir_text).unwrap();
    let state = StateView::from_ir(&ir);
    let mut r = Renderer::new(240, 200);
    r.set_elevation_shadows(elevation);
    r.render(&ir, &state).pixels
}

#[test]
fn elevation_switch_changes_shadow_and_default_is_unchanged() {
    let ir_text = box_ir(3);
    let legacy = render(&ir_text, false);
    let material = render(&ir_text, true);
    assert_ne!(
        digest_hex(&legacy),
        digest_hex(&material),
        "elevation table draws a different shadow"
    );
    // Switch off must equal the original strength-derived path byte for byte.
    let ir = parse_ir(&ir_text).unwrap();
    let state = StateView::from_ir(&ir);
    let original = Renderer::new(240, 200).render(&ir, &state).pixels;
    assert_eq!(
        digest_hex(&legacy),
        digest_hex(&original),
        "switch off is the default path"
    );
}

#[test]
fn switch_on_is_deterministic_and_level_dependent() {
    let a = render(&box_ir(3), true);
    let b = render(&box_ir(3), true);
    assert_eq!(digest_hex(&a), digest_hex(&b), "deterministic");
    let l1 = render(&box_ir(1), true);
    let l6 = render(&box_ir(6), true);
    assert_ne!(
        digest_hex(&l1),
        digest_hex(&l6),
        "different levels draw differently"
    );
}

#[test]
fn capture_command_stream_ignores_the_switch() {
    // The display-list path builds its own capturing canvas and never consults
    // the elevation switch, so commands are identical with it on or off.
    let ir_text = box_ir(3);
    let ir = parse_ir(&ir_text).unwrap();
    let state = StateView::from_ir(&ir);
    let mut r_off = Renderer::new(240, 200);
    r_off.set_elevation_shadows(false);
    let mut r_on = Renderer::new(240, 200);
    r_on.set_elevation_shadows(true);
    let dl_off = r_off.display_list(&ir, &state);
    let dl_on = r_on.display_list(&ir, &state);
    assert_eq!(
        format!("{:?}", dl_off.commands),
        format!("{:?}", dl_on.commands),
        "display-list command stream ignores the elevation switch"
    );
}

#[test]
fn default_path_keeps_the_two_counter_goldens() {
    let text = std::fs::read_to_string(COUNTER).unwrap();
    let ir = parse_ir(&text).unwrap();
    let state = StateView::from_ir(&ir);
    let mut r = Renderer::new(320, 240);
    r.set_elevation_shadows(false);
    assert_eq!(
        digest_hex(&r.render(&ir, &state).pixels),
        "8d45f412996a99c6"
    );
    let mut r2 = Renderer::new(800, 600);
    r2.set_elevation_shadows(true); // counter has no shadow node -> no effect
    assert_eq!(
        digest_hex(&r2.render(&ir, &state).pixels),
        "318beb1dd337bc4e"
    );
}
