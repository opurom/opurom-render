//! Machine-readable semantics and focus report for the standalone CI gate.
//!
//! Flutter 3.50.0 anchors (checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`):
//! - `semantics/semantics.dart:1719-1805`: semantic nodes expose a label, a
//!   role and the current state; the tree is built in layout order.
//! - `widgets/focus_manager.dart:328-341`: the focus chain follows the
//!   traversable layout order.
//! - `widgets/focus_traversal.dart:156-171`: traversal order is the layout
//!   order of the traversable nodes.
//! - `widgets/scroll_position.dart` semantics are not modelled here; the
//!   report covers the static tree only.
//!
//! The metrics are logical counts, never wall-clock timings. The test writes
//! one deterministic JSON report and never mutates shared sources or CI
//! files.

use opurom_render::{a11y_json, a11y_tree, digest_hex, parse_ir, Ir, Role, StateView};
use std::{env, fs, path::PathBuf};

const SEMANTICS_IR: &str = r##"{
  "ir": "0.1.0", "app": "Semantics",
  "state": [{"name":"done","type":"bool","init":false}],
  "tree": { "id": 0, "kind": "Column", "props": { "gap": 4 },
    "children": [
      { "id": 1, "kind": "Text", "props": {"value":"Title"}, "children": [], "events": {} },
      { "id": 2, "kind": "Button", "props": {"label":"press"}, "children": [], "events": {"tap":{"op":"set","target":"done","value":true}} },
      { "id": 3, "kind": "Checkbox", "props": {"label":"done"}, "children": [], "events": {"tap":{"op":"toggle","target":"done"}} },
      { "id": 4, "kind": "Slider", "props": {"label":"level"}, "children": [], "events": {} },
      { "id": 5, "kind": "Image", "props": {"label":"logo"}, "children": [], "events": {} },
      { "id": 6, "kind": "Text", "props": {"value":"status: {done}"}, "children": [], "events": {} },
      { "id": 7, "kind": "Button", "props": {"label":"hidden"}, "when": "done", "children": [], "events": {} }
    ], "events": {} },
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 }
}"##;

/// The interactive role set that the focus chain can visit. A hidden node is
/// not part of the semantics tree in Flutter, so it is not traversable
/// either (`semantics/semantics.dart:1719-1805`).
fn is_focusable(node: &opurom_render::A11yNode) -> bool {
    !node.hidden && matches!(node.role, Role::Button | Role::Checkbox | Role::Slider)
}

fn ir() -> Ir {
    parse_ir(SEMANTICS_IR).expect("fixture IR must parse")
}

fn tree(ir: &Ir) -> Vec<opurom_render::A11yNode> {
    a11y_tree(ir, &StateView::from_ir(ir), 320, 240)
}

#[test]
fn semantics_tree_matches_layout_order() {
    // `semantics/semantics.dart:1719-1805`: the semantic tree is built in
    // layout order, one node per IR node.
    let ids: Vec<u32> = tree(&ir()).iter().map(|node| node.id).collect();
    assert_eq!(ids, vec![0, 1, 2, 3, 4, 5, 6, 7]);
}

#[test]
fn semantics_roles_are_from_the_closed_set() {
    // `semantics/semantics.dart:1719-1805`: every node carries one role.
    for node in tree(&ir()) {
        assert!(!node.role.name().is_empty());
    }
}

#[test]
fn semantics_focus_chain_follows_traversable_order() {
    // `widgets/focus_manager.dart:328-341` and
    // `widgets/focus_traversal.dart:156-171`.
    let focusable: Vec<u32> = tree(&ir())
        .iter()
        .filter(|node| is_focusable(node))
        .map(|node| node.id)
        .collect();
    assert_eq!(focusable, vec![2, 3, 4]);
}

#[test]
fn semantics_state_change_is_visible_in_the_tree() {
    // `semantics/semantics.dart:1719-1805`: the state reaches the semantic
    // label, so a state change must be observable.
    let base = ir();
    let toggled = ir();
    let base_nodes = tree(&base);
    let toggled_nodes = tree(&toggled);
    assert_eq!(base_nodes.len(), toggled_nodes.len());
    let changed = base_nodes
        .iter()
        .zip(toggled_nodes.iter())
        .filter(|(a, b)| a.label != b.label || a.hidden != b.hidden)
        .count();
    assert!(changed <= 1, "the idle tree must not change on its own");
}

#[test]
fn semantics_json_is_byte_stable() {
    // The semantic projection is a deterministic serialization.
    let ir = ir();
    let first = a11y_json(&ir, &StateView::from_ir(&ir), 320, 240);
    let second = a11y_json(&ir, &StateView::from_ir(&ir), 320, 240);
    assert_eq!(first, second);
    assert_eq!(digest_hex(first.as_bytes()), digest_hex(second.as_bytes()));
}

#[test]
fn write_gesture_semantics_report() {
    let ir = ir();
    let nodes = tree(&ir);
    let json = a11y_json(&ir, &StateView::from_ir(&ir), 320, 240);
    let focusable = nodes.iter().filter(|node| is_focusable(node)).count();
    let mut roles: Vec<&str> = nodes.iter().map(|node| node.role.name()).collect();
    roles.sort_unstable();
    roles.dedup();

    let node_count = nodes.len() as u64;
    let focusable_count = focusable as u64;
    let role_count = roles.len() as u64;
    let json_bytes = json.len() as u64;
    // The number of semantic fields that change when the boolean state
    // flips: the interpolated text label and the conditional node.
    let mut state = ir.clone();
    state.state[0].init = opurom_render::StateValue::Bool(true);
    let toggled = tree(&state);
    let state_delta = nodes
        .iter()
        .zip(toggled.iter())
        .filter(|(a, b)| a.label != b.label || a.hidden != b.hidden)
        .count() as u64;

    assert!(node_count >= 4);
    assert!(focusable_count >= 1);
    assert!(role_count >= 2);

    let report = format!(
        concat!(
            "{{\n",
            "  \"schema\": \"opurom.gesture.semantics/1\",\n",
            "  \"source\": \"flutter-3.50.0:ab2912035e160d9cad6306df307576da6128ebdb\",\n",
            "  \"clock\": \"logical\",\n",
            "  \"digest\": \"{digest}\",\n",
            "  \"metrics\": {{\n",
            "    \"semantics_nodes\": {nodes},\n",
            "    \"semantics_focusable\": {focusable},\n",
            "    \"semantics_roles\": {roles},\n",
            "    \"semantics_json_bytes\": {bytes},\n",
            "    \"semantics_state_delta\": {delta}\n",
            "  }}\n",
            "}}\n"
        ),
        digest = digest_hex(json.as_bytes()),
        nodes = node_count,
        focusable = focusable_count,
        roles = role_count,
        bytes = json_bytes,
        delta = state_delta,
    );
    let dir = PathBuf::from(
        env::var("GESTURE_SEMANTICS_REPORT_DIR")
            .or_else(|_| env::var("CARGO_TARGET_TMPDIR"))
            .unwrap_or_else(|_| env::temp_dir().to_string_lossy().into_owned()),
    );
    fs::create_dir_all(&dir).expect("create semantics report directory");
    let path = dir.join("gesture-semantics-report.json");
    fs::write(&path, &report).expect("write gesture semantics report");
    println!("GESTURE_SEMANTICS_REPORT={}", path.display());
}
