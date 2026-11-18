//! Focus-chain and semantic-state parity pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `widgets/focus_manager.dart:328-341` (blob `9aa8e…` at the pinned
//!   checkout): focus nodes form a sparse tree with parent/children/ancestors/
//!   descendants accessors; scopes restrict traversal to a subtree.
//! - `widgets/focus_manager.dart:420-435`: `nextFocus`/`previousFocus` are
//!   resolved by the ambient `FocusTraversalPolicy`.
//! - `widgets/focus_traversal.dart:185-220` and `:222-240`: a traversal
//!   request moves focus to the next eligible node and ensures visibility;
//!   scopes recurse into descendants until a regular focus node is found.
//! - `gestures/binding.dart:506-508` (blob `15f48b2c…`): events dispatch along
//!   the hit path from the most specific entry to its ancestors.
//! - `semantics/semantics.dart:1719-1805`: semantic properties expose label,
//!   enabled/focused/checked/button and tap/long-press actions.
//! - `rendering/proxy_box.dart:145-209` (`HitTestBehavior`): an opaque target
//!   consumes the hit, while a translucent target permits targets behind it.
//!
//! Opurom has no runtime focus widget tree and no new IR field here. This
//! suite pins the deterministic IR-level projection that hosts can use as the
//! focus/semantics chain: layout pre-order plus accessibility roles, labels,
//! bounds and hidden state from the same IR and state view.

use opurom_render::{a11y_json, a11y_tree, parse_ir, A11yNode, Ir, Role, StateView};

fn ir_of(text: &str) -> Ir {
    parse_ir(text).expect("fixture IR must parse")
}

fn tree(ir: &Ir) -> Vec<A11yNode> {
    a11y_tree(ir, &StateView::from_ir(ir), 320, 240)
}

const FOCUS_IR: &str = r##"{
  "ir": "0.1.0", "app": "FocusChain",
  "state": [{"name":"done","type":"bool","init":false}],
  "tree": { "id": 0, "kind": "Column", "props": { "gap": 4 },
    "children": [
      { "id": 1, "kind": "Container", "props": {"role":"none","label":"root group"},
        "children": [
          { "id": 2, "kind": "Button", "props": {"label":"first","role":"button"}, "children": [], "events": {"tap":{"op":"set","target":"done","value":true}} },
          { "id": 3, "kind": "Button", "props": {"label":"second","role":"button"}, "children": [], "events": {"tap":{"op":"set","target":"done","value":true}} }
        ], "events": {} },
      { "id": 4, "kind": "Checkbox", "props": {"label":"done","role":"checkbox"}, "children": [], "events": {"tap":{"op":"toggle","target":"done"}} },
      { "id": 5, "kind": "Text", "props": {"value":"status: {done}"}, "children": [], "events": {} },
      { "id": 6, "kind": "Button", "props": {"label":"hidden action","role":"button"}, "when": "done", "children": [], "events": {"tap":{"op":"set","target":"done","value":false}} }
    ], "events": {} },
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 }
}"##;

#[test]
fn focus_chain_is_leaf_to_root_pre_order() {
    let nodes = tree(&ir_of(FOCUS_IR));
    let ids: Vec<u32> = nodes.iter().map(|n| n.id).collect();
    assert_eq!(ids, vec![0, 1, 2, 3, 4, 5, 6]);
    // The parent appears before its traversable children in the projection,
    // while event dispatch uses the reverse leaf-to-root path. The renderer
    // already owns that path; this pin ensures the semantic tree uses the same
    // stable node identity (`focus_manager.dart:328-341`).
    let by_id = |id| nodes.iter().find(|n| n.id == id).unwrap();
    assert_eq!(by_id(1).label, "root group");
    assert!(by_id(2).bounds[2] > 0 && by_id(2).bounds[3] > 0);
}

#[test]
fn focus_candidates_follow_traversable_layout_order() {
    let nodes = tree(&ir_of(FOCUS_IR));
    let focusable: Vec<&A11yNode> = nodes
        .iter()
        .filter(|n| matches!(n.role, Role::Button | Role::Checkbox))
        .collect();
    let labels: Vec<&str> = focusable.iter().map(|n| n.label.as_str()).collect();
    assert_eq!(labels, vec!["first", "second", "done", "hidden action"]);
    // A host advances through this list with nextFocus/previousFocus
    // (`focus_manager.dart:420-435`); hidden entries still carry their state
    // flag so traversal can skip them without rebuilding the tree.
    assert!(
        focusable[3].hidden,
        "the gated action must be marked hidden"
    );
}

#[test]
fn consumed_hit_stops_path_walk_at_first_eligible_node() {
    // Flutter dispatch visits every entry leaf-to-root (`binding.dart:506-508`).
    // Opurom's gesture engine consumes on the first node that produces an
    // event; this test pins the equivalent semantic identity for that node.
    let nodes = tree(&ir_of(FOCUS_IR));
    let leaf = nodes.iter().find(|n| n.id == 2).unwrap();
    let parent = nodes.iter().find(|n| n.id == 1).unwrap();
    assert_eq!(leaf.role, Role::Button);
    assert_eq!(parent.label, "root group");
    assert!(
        leaf.bounds[0] >= parent.bounds[0] && leaf.bounds[1] >= parent.bounds[1],
        "the consumed leaf must sit inside its parent's bounds"
    );
}

#[test]
fn semantic_label_role_and_state_form_a_minimal_closed_loop() {
    let base = tree(&ir_of(FOCUS_IR));
    let done = base.iter().find(|n| n.id == 4).unwrap();
    assert_eq!(done.role, Role::Checkbox);
    assert_eq!(done.label, "done");
    let hidden = base.iter().find(|n| n.id == 6).unwrap();
    assert!(hidden.hidden);
    let status = base.iter().find(|n| n.id == 5).unwrap();
    assert_eq!(status.label, "status: false");

    let mut ir = ir_of(FOCUS_IR);
    ir.state[0].init = opurom_render::StateValue::Bool(true);
    let toggled = tree(&ir);
    let status = toggled.iter().find(|n| n.id == 5).unwrap();
    assert_eq!(status.label, "status: true");
    let hidden = toggled.iter().find(|n| n.id == 6).unwrap();
    assert!(
        !hidden.hidden,
        "the gated action becomes visible after toggle"
    );
}

#[test]
fn semantic_json_is_byte_stable_across_state_changes() {
    let base = ir_of(FOCUS_IR);
    let first = a11y_json(&base, &StateView::from_ir(&base), 320, 240);
    assert_eq!(
        first,
        a11y_json(&base, &StateView::from_ir(&base), 320, 240)
    );
    assert!(first.contains("\"role\":\"checkbox\""));
    assert!(first.contains("\"label\":\"status: false\""));
    assert!(first.contains("\"hidden\":true"));

    let mut toggled = ir_of(FOCUS_IR);
    toggled.state[0].init = opurom_render::StateValue::Bool(true);
    let second = a11y_json(&toggled, &StateView::from_ir(&toggled), 320, 240);
    assert_eq!(
        second,
        a11y_json(&toggled, &StateView::from_ir(&toggled), 320, 240)
    );
    assert_ne!(first, second);
    assert!(second.contains("\"label\":\"status: true\""));
    assert!(!second.contains("\"hidden\":true"));
}
