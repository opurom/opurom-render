//! IR gesture/press binding: a pointer hit on any descendant of an ink host
//! must resolve to the same host as the event node that owns the tap action.
//! Flutter's MaterialButton routes onTapDown/onTapUp through InkWell's
//! MaterialStatesController (a WidgetStatesController alias,
//! `material_state.dart:599`), so the pressed state belongs to the button, not
//! to the inner Text/Row that happened to receive the pointer
//! (`button_style_button.dart:555-571`, `ink_well.dart:1010-1014,1205`).

use opurom_render::{a11y_tree, digest_hex, parse_ir, PressPhase, Renderer, StateView};

const IR: &str = r##"{
  "ir": "0.1.0", "app": "GesturePressBinding", "state": [{"name":"x","type":"int","init":0}],
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 },
  "tree": { "id": 0, "kind": "Column", "props": { "padding": 8, "gap": 8 }, "children": [
    { "id": 1, "kind": "Button", "props": { "color": "#FFFFFF", "width": 160, "height": 80 },
      "children": [
        { "id": 2, "kind": "Text", "props": { "value": "Press", "color": "#101010", "size": 20 },
          "children": [], "events": { "tap": { "op": "set", "target": "x", "value": 1 } } }
      ], "events": {} }
  ], "events": {} }
}"##;

fn press(node_id: u32) -> PressPhase {
    PressPhase {
        node_id,
        press_start_tick: 0,
        confirm_tick: None,
        touch: None,
    }
}

fn frame(node_id: u32, pressed: bool) -> String {
    let ir = parse_ir(IR).unwrap();
    let state = StateView::from_ir(&ir);
    let phase = if pressed { Some(press(node_id)) } else { None };
    let frame = Renderer::new(240, 200).render_with_press(&ir, &state, 0, phase.as_ref());
    digest_hex(&frame.pixels)
}

#[test]
fn descendant_hit_and_event_node_share_the_same_press_host() {
    // The child owns the `tap` contract; the Button owns the ink. A press on
    // either id must produce the same pressed host and the same pixels.
    assert_ne!(frame(1, false), frame(1, true));
    assert_eq!(frame(1, true), frame(2, true));
}

#[test]
fn both_nodes_are_visible_in_the_accessibility_tree() {
    let ir = parse_ir(IR).unwrap();
    let state = StateView::from_ir(&ir);
    let a11y = a11y_tree(&ir, &state, 240, 200);
    assert!(a11y.iter().any(|n| n.id == 2));
    assert!(a11y.iter().any(|n| n.id == 1));
}
