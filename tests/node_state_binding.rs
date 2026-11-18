//! Read-only host-owned node-state table (M3 interaction binding).
//!
//! Source authority: Flutter's Material widgets share one
//! `WidgetStatesController`/`MaterialStatesController` (`material_state.dart:599`)
//! and map resolved states to paint in `button_style_button.dart`; Opurom's
//! IR 0.1.0 has no runtime event column, so the host supplies the relation as
//! `(hit node id, NodeState)` without touching schema or emitters.

use opurom_render::{digest_hex, ink_host, layout, parse_ir, NodeState, Renderer, StateView};

const IR: &str = r##"{
  "ir": "0.1.0", "app": "NodeStates", "state": [{"name":"flag","type":"bool","init":false}],
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 },
  "tree": { "id": 0, "kind": "Column", "props": { "padding": 8, "gap": 8 }, "children": [
    { "id": 1, "kind": "Button", "props": { "color": "#FFFFFF", "width": 160, "height": 40, "elevation": 2 },
      "children": [
        { "id": 2, "kind": "Text", "props": { "value": "Press", "color": "#101010", "size": 20 },
          "children": [], "events": { "tap": { "op": "set", "target": "flag", "value": true } } }
      ], "events": {} },
    { "id": 3, "kind": "Text", "props": { "value": "Plain", "color": "#101010", "size": 20 },
      "children": [], "events": {} }
  ], "events": {} }
}"##;

fn frame(entries: Option<&[(u32, NodeState)]>) -> Vec<u8> {
    let ir = parse_ir(IR).unwrap();
    let state = StateView::from_ir(&ir);
    Renderer::new(240, 200)
        .render_with_node_states(&ir, &state, 0, entries)
        .pixels
}

fn base_frame() -> Vec<u8> {
    let ir = parse_ir(IR).unwrap();
    let state = StateView::from_ir(&ir);
    Renderer::new(240, 200).render_at(&ir, &state, 0).pixels
}

#[test]
fn none_is_byte_identical_to_render_at() {
    assert_eq!(frame(None), base_frame());
    assert_eq!(frame(Some(&[])), base_frame());
}

#[test]
fn default_state_entry_is_byte_identical_to_render_at() {
    assert_eq!(frame(Some(&[(1, NodeState::Default)])), base_frame());
}

#[test]
fn unmapped_id_does_not_change_the_frame() {
    // 99 is not in the IR; an unreachable id must not invent a paint entry.
    assert_eq!(frame(Some(&[(99, NodeState::Hover)])), base_frame());
}

#[test]
fn nested_text_pointer_colors_the_ink_host_not_the_text() {
    let ir = parse_ir(IR).unwrap();
    let state = StateView::from_ir(&ir);
    let laid = layout(&ir, &state, 240, 200);
    let host_id = ink_host(&ir.tree, 2, None).expect("nested Text must map to Button");
    assert_eq!(host_id, 1);
    let host = laid.iter().find(|n| n.id == 1).unwrap().clone();
    let text = laid.iter().find(|n| n.id == 2).unwrap().clone();

    let base = base_frame();
    let pressed = frame(Some(&[(2, NodeState::Active)]));
    assert_ne!(digest_hex(&base), digest_hex(&pressed));

    // Both ids must resolve to the same host paint, and the Button's state
    // must actually change. The Text child's own styled box is untouched: its
    // layout text colour and state are unchanged by the host override.
    assert_eq!(
        digest_hex(&pressed),
        digest_hex(&frame(Some(&[(1, NodeState::Active)])))
    );
    assert_eq!(text.state, NodeState::Default);
    assert_eq!(
        text.text_color,
        Some(opurom_render::Rgba::rgb(0x10, 0x10, 0x10))
    );
}

#[test]
fn disabled_authored_state_wins_over_host_state() {
    let ir: opurom_render::Ir =
        parse_ir(&IR.replace("\"elevation\": 2", "\"elevation\": 2, \"disabled\": true")).unwrap();
    let state = StateView::from_ir(&ir);
    let normal = Renderer::new(240, 200).render_at(&ir, &state, 0).pixels;
    let overridden = Renderer::new(240, 200)
        .render_with_node_states(&ir, &state, 0, Some(&[(1, NodeState::Active)]))
        .pixels;
    assert_eq!(digest_hex(&normal), digest_hex(&overridden));
}
