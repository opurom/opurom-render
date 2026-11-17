//! Ink gating: the mobile hit test can land on a Button's Text child, but
//! Flutter paints the splash on the button's Material box. A press whose IR
//! ancestors contain no button-semantic node must never paint a ripple.

use opurom_render::{parse_ir, PressPhase, Renderer, StateView};

fn ir() -> String {
    r##"{
  "ir": "0.1.0", "app": "InkGating", "state": [],
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 4, "surface": "#FFFFFF", "onSurface": "#000000" },
  "tree": { "id": 0, "kind": "Column", "props": { "padding": 8, "gap": 8 }, "children": [
    { "id": 1, "kind": "Button", "props": { "color": "#FFFFFF", "width": 120, "height": 60 },
      "children": [
        { "id": 2, "kind": "Text", "props": { "value": "Press", "color": "#101010", "size": 20 },
          "children": [], "events": { "tap": { "actions": [] } } }
      ], "events": {} },
    { "id": 3, "kind": "Text", "props": { "value": "Plain", "color": "#101010", "size": 20 },
      "children": [], "events": { "tap": { "actions": [] } } },
    { "id": 4, "kind": "Container", "props": { "color": "#EEEEEE", "width": 120, "height": 30 },
      "children": [], "events": { "tap": { "actions": [] } } }
  ], "events": {} }
}"##.to_string()
}

fn frame(node_id: u32, at: i64) -> Vec<u8> {
    let text = ir();
    let ir = parse_ir(&text).unwrap();
    let state = StateView::from_ir(&ir);
    let press = PressPhase {
        node_id,
        press_start_tick: 0,
        confirm_tick: None,
        touch: None,
    };
    Renderer::new(240, 200)
        .render_with_press(&ir, &state, at, Some(&press))
        .pixels
}

fn idle() -> Vec<u8> {
    let text = ir();
    let ir = parse_ir(&text).unwrap();
    let state = StateView::from_ir(&ir);
    Renderer::new(240, 200)
        .render_with_press(&ir, &state, 300, None)
        .pixels
}

// Accent splash over the white button is light blue; grey state layers and
// plain dark text are not. This isolates the ripple disc from press styling.
fn is_ripple(p: &[u8]) -> bool {
    p[2] as i32 > p[0] as i32 + 30 && p[2] as i32 > p[1] as i32 + 15 && p[2] > 120
}

fn ripple_count(f: &[u8]) -> usize {
    f.chunks_exact(4).filter(|p| is_ripple(p)).count()
}

#[test]
fn hit_text_child_paints_ripple_on_the_button_host() {
    let child = frame(2, 300);
    let button = frame(1, 300);
    let (nc, nb) = (ripple_count(&child), ripple_count(&button));
    assert!(nb > 3000, "pressing the Button paints a full ripple");
    assert!(nc > 3000, "hitting the Text child must paint a full ripple");
    // Both paint the same Button-sized disc; the small delta is the child's
    // grey Text state layer drawn over part of it.
    assert!((nc as i64 - nb as i64).abs() < 800);
}

#[test]
fn plain_text_and_container_have_no_ripple_host() {
    assert_eq!(ripple_count(&idle()), 0);
    assert!(
        ripple_count(&frame(3, 300)) < 500,
        "plain tappable Text has no ink host"
    );
    assert!(
        ripple_count(&frame(4, 300)) < 500,
        "plain tappable Container has no ink host"
    );
}
