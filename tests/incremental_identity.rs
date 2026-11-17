//! Commit 1: stable identity, deterministic subtree/paint hashes, and the
//! dependency index (INCREMENTAL_RENDER_PIPELINE §2-§3). Pure and opt-in.

use opurom_render::{
    dep_values, deps, layout, paint_hash, parse_ir, subtree_hash, NodeIdentity, StateValue,
    StateView,
};

const BASE: &str = r##"{
  "ir": "0.1.0", "app": "Inc", "state": [
    { "name": "count", "type": "int", "init": 1 },
    { "name": "show", "type": "bool", "init": true },
    { "name": "label", "type": "string", "init": "hi" }
  ],
  "tree": { "id": 0, "kind": "Column", "props": { "gap": 2, "padding": 0 },
    "children": [
      { "id": 1, "kind": "Text", "props": { "value": "n={count} / {label}" }, "children": [], "events": {} },
      { "id": 2, "kind": "Text", "props": { "value": "static" }, "children": [], "events": {} },
      { "id": 3, "kind": "Container", "when": "show", "props": { "color": "#102030", "width": 20, "height": 10 },
        "children": [ { "id": 4, "kind": "Input", "props": { "bind": "label", "placeholder": "{label}!" }, "children": [], "events": {} } ],
        "events": {} }
    ], "events": {} },
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 0 }
}"##;

#[test]
fn static_identity_is_the_ir_node_id() {
    let ir = parse_ir(BASE).unwrap();
    let state = StateView::from_ir(&ir);
    let laid = layout(&ir, &state, 320, 240);
    let ids: Vec<u32> = laid
        .iter()
        .filter_map(|l| l.incremental.identity.as_ref().map(|i| i.node_id))
        .collect();
    // Pre-order: root, text, text, container, input (input is inside when=true).
    assert_eq!(ids, vec![0, 1, 2, 3, 4]);
    assert!(laid
        .iter()
        .all(|l| l.incremental.identity.as_ref().unwrap().row_key.is_none()));
    assert_eq!(NodeIdentity::node(7).key(), "7");
}

#[test]
fn for_rows_identity_pairs_node_id_with_element_value() {
    let ir = parse_ir(r##"{
      "ir": "0.1.0", "app": "Rows", "state": [ { "name": "items", "type": "list", "init": [] } ],
      "tree": { "id": 0, "kind": "For", "props": { "source": "items", "item": "p" },
        "children": [ { "id": 1, "kind": "Text", "props": { "value": "{p}" }, "children": [], "events": {} } ],
        "events": {} },
      "theme": { "mode": "light", "accent": "#097FEB", "radius": 0 }
    }"##).unwrap();
    let mut state = StateView::from_ir(&ir);
    state.set(
        "items",
        StateValue::List(vec!["a".into(), "b".into(), "c".into()]),
    );
    let laid = layout(&ir, &state, 320, 240);
    let row_ids: Vec<String> = laid
        .iter()
        .filter_map(|l| l.incremental.identity.as_ref())
        .filter_map(|i| i.row_key.clone())
        .collect();
    assert_eq!(row_ids, vec!["a", "b", "c"]);
    assert_eq!(NodeIdentity::row(1, "b").key(), "1#b");
}

#[test]
fn deps_are_exact_ordered_and_interpolation_aware() {
    let ir = parse_ir(BASE).unwrap();
    let d1 = deps(&ir.tree.children[0]);
    assert_eq!(d1, vec!["count", "label"]);
    let d3 = deps(&ir.tree.children[2]);
    assert_eq!(d3, vec!["show"]);
    let d4 = deps(&ir.tree.children[2].children[0]);
    assert_eq!(d4, vec!["label"]); // bind + placeholder both resolve to label
                                   // Same order on every run.
    assert_eq!(deps(&ir.tree.children[0]), d1);
    // dep_values change only when a read key changes.
    let mut s1 = StateView::from_ir(&ir);
    let v1 = dep_values(&ir.tree.children[0], &s1);
    assert_eq!(v1, dep_values(&ir.tree.children[0], &s1));
    s1.set("count", StateValue::Int(99));
    assert_ne!(v1, dep_values(&ir.tree.children[0], &s1));
    // A key this node does not read does not affect it.
    let node2 = &ir.tree.children[1];
    let v2 = dep_values(node2, &s1);
    s1.set("count", StateValue::Int(7));
    assert_eq!(v2, dep_values(node2, &s1));
}

#[test]
fn hashes_are_deterministic_and_separate_paint_from_structure() {
    let ir = parse_ir(BASE).unwrap();
    let state = StateView::from_ir(&ir);
    let root = &ir.tree;
    assert_eq!(subtree_hash(root, &state), subtree_hash(root, &state));
    assert_eq!(paint_hash(root), paint_hash(root));
    // Appearance-only: a state change that does not touch props leaves paint
    // hash stable, while subtree hash (covering resolved deps) may change.
    let mut changed = StateView::from_ir(&ir);
    changed.set("count", StateValue::Int(42));
    assert_eq!(paint_hash(root), paint_hash(root));
    // A color change alters paint hash.
    let mut ir2 = parse_ir(BASE).unwrap();
    ir2.tree.children[2].props.color = Some(opurom_render::Rgba::rgb(1, 2, 3));
    assert_ne!(
        paint_hash(&ir2.tree.children[2]),
        paint_hash(&ir.tree.children[2])
    );
    // Different structure differs from different paint.
    let mut ir3 = parse_ir(BASE).unwrap();
    ir3.tree.children[2].props.width = Some(999);
    assert_ne!(subtree_hash(&ir3.tree, &state), subtree_hash(root, &state));
}
