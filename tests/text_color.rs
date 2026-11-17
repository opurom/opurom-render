//! G7 Button `textColor` (GALLERY_PIXEL_GAP_TRACKER.md §7): explicit foreground
//! independent of fill. Opt-in; absent textColor keeps every golden byte-fixed.

use opurom_render::{digest_hex, layout, parse_ir, Renderer, Rgba, StateView};

const COUNTER: &str = "../opurom-examples/golden/counter.opuir.json";

fn node_ir(props: &str) -> String {
    format!(
        r##"{{
        "ir": "0.1.0", "app": "TextColor", "state": [],
        "tree": {{ "id": 0, "kind": "Column", "props": {{ "gap": 2 }},
            "children": [ {{ "id": 1, "kind": "Button", "props": {{ @PROPS@ }},
                "children": [], "events": {{}} }} ], "events": {{}} }},
        "theme": {{ "mode": "light", "accent": "#097FEB", "radius": 4 }}
    }}"##
    )
    .replace("@PROPS@", props)
}

fn text_color(props: &str) -> Option<Rgba> {
    let ir = parse_ir(&node_ir(props)).unwrap();
    let state = StateView::from_ir(&ir);
    layout(&ir, &state, 240, 200)
        .into_iter()
        .find(|n| n.id == 1)
        .unwrap()
        .text_color
}

#[test]
fn explicit_text_color_is_independent_of_fill() {
    let c = text_color(r##""label":"Go","color":"#000000","textColor":"#FF0000""##);
    assert_eq!(
        c,
        Some(Rgba::rgb(0xff, 0x00, 0x00)),
        "explicit red foreground"
    );
}

#[test]
fn text_color_overrides_variant_foreground() {
    let variant_only = text_color(r##""label":"Go","variant":"primary""##);
    let overridden = text_color(r##""label":"Go","variant":"primary","textColor":"#00FF00""##);
    assert_ne!(
        variant_only, overridden,
        "explicit textColor overrides variant"
    );
    assert_eq!(overridden, Some(Rgba::rgb(0x00, 0xff, 0x00)));
}

#[test]
fn absent_text_color_keeps_existing_behaviour() {
    let with_fill = text_color(r##""label":"Go","color":"#123456""##);
    let plain = text_color(r##""label":"Go""##);
    assert_eq!(
        with_fill, plain,
        "no textColor: foreground unchanged from fill/default"
    );
}

#[test]
fn deterministic_with_text_color_declared() {
    let ir = parse_ir(&node_ir(r##""label":"Go","textColor":"#FF0000""##)).unwrap();
    let state = StateView::from_ir(&ir);
    let a = layout(&ir, &state, 240, 200);
    let b = layout(&ir, &state, 240, 200);
    assert_eq!(a, b, "deterministic with textColor and default contrast");
}

#[test]
fn default_path_keeps_the_two_counter_goldens() {
    let text = std::fs::read_to_string(COUNTER).unwrap();
    let ir = parse_ir(&text).unwrap();
    let state = StateView::from_ir(&ir);
    assert_eq!(
        digest_hex(&Renderer::new(320, 240).render(&ir, &state).pixels),
        "8d45f412996a99c6"
    );
    assert_eq!(
        digest_hex(&Renderer::new(800, 600).render(&ir, &state).pixels),
        "318beb1dd337bc4e"
    );
}
