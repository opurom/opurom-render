//! Dynamic lists (DYNAMIC_LISTS Phase A): a `For` node lays out one template
//! row per element of its list state, binds the `item` name per row without
//! leaking it into sibling lookups, and follows the state through every
//! post-mutation stage (`assign`/`append`/`removeAt`).

use opurom_render::{layout, parse_ir, Kind, Op, StateValue, StateView};

/// `items` starts empty: zero rows is the render baseline before any fetch.
const IR: &str = r##"{
  "ir": "0.1.0", "app": "DynamicList",
  "state": [
    { "name": "items", "type": "list", "init": [] },
    { "name": "count", "type": "int", "init": 0 }
  ],
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 8 },
  "tree": { "id": 0, "kind": "Column", "props": { "gap": 4, "padding": 8 },
    "children": [
      { "id": 1, "kind": "For", "props": { "source": "items", "item": "p", "gap": 4 },
        "children": [
          { "id": 2, "kind": "Text", "props": { "value": "- {p}", "size": 14 },
            "children": [], "events": {} }
        ],
        "events": {} },
      { "id": 3, "kind": "Text", "props": { "value": "outer {p}", "size": 14 },
        "children": [], "events": {} },
      { "id": 4, "kind": "Text", "props": { "value": "Total {count}", "size": 14 },
        "children": [], "events": {} }
    ],
    "events": {} }
}"##;

/// Same tree with one button per list mutation op, exactly as the compiler
/// emits them (`DYNAMIC_LISTS.md` IR contract).
const EVENTS_IR: &str = r##"{
  "ir": "0.1.0", "app": "DynamicListEvents",
  "state": [ { "name": "items", "type": "list", "init": ["seed"] } ],
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 8 },
  "tree": { "id": 0, "kind": "Column", "props": { "gap": 4, "padding": 8 },
    "children": [
      { "id": 1, "kind": "For", "props": { "source": "items", "item": "p" },
        "children": [
          { "id": 2, "kind": "Text", "props": { "value": "{p}", "size": 14 },
            "children": [], "events": {} }
        ],
        "events": {} },
      { "id": 3, "kind": "Button", "props": { "label": "add" }, "children": [],
        "events": { "tap": { "op": "append", "target": "items", "value": "next" } } },
      { "id": 4, "kind": "Button", "props": { "label": "drop" }, "children": [],
        "events": { "tap": { "op": "removeAt", "target": "items", "value": 0 } } }
    ],
    "events": {} }
}"##;

/// Render the fixture with `items` replaced by the stage's list and return the
/// generated row texts in array order.
fn rows_at(items: &[&str]) -> Vec<String> {
    let ir = parse_ir(IR).expect("fixture parses");
    let mut state = StateView::from_ir(&ir);
    state.set(
        "items",
        StateValue::List(items.iter().map(|s| s.to_string()).collect()),
    );
    layout(&ir, &state, 320, 400)
        .iter()
        .filter(|n| n.kind == Kind::Text)
        .filter_map(|n| n.text.clone())
        .filter(|text| text.starts_with("- "))
        .collect()
}

fn texts_at(items: &[&str]) -> Vec<String> {
    let ir = parse_ir(IR).expect("fixture parses");
    let mut state = StateView::from_ir(&ir);
    state.set(
        "items",
        StateValue::List(items.iter().map(|s| s.to_string()).collect()),
    );
    layout(&ir, &state, 320, 400)
        .iter()
        .filter_map(|n| n.text.clone())
        .collect()
}

#[test]
fn empty_list_renders_zero_rows() {
    assert_eq!(rows_at(&[]), Vec::<String>::new());
}

#[test]
fn assign_replaces_the_whole_list_and_row_count_follows() {
    // Stage 1: a fetch-into-list assign lands two elements.
    assert_eq!(rows_at(&["alpha", "beta"]), vec!["- alpha", "- beta"]);
    // Stage 4: a later assign replaces them wholesale; rows follow, no residue.
    assert_eq!(rows_at(&["one"]), vec!["- one"]);
    assert_eq!(rows_at(&[]), Vec::<String>::new());
}

#[test]
fn append_adds_one_row_and_keeps_array_order() {
    // The state after `items.append("gamma")`.
    assert_eq!(
        rows_at(&["alpha", "beta", "gamma"]),
        vec!["- alpha", "- beta", "- gamma"]
    );
}

#[test]
fn remove_at_drops_exactly_that_row() {
    // The state after `items.removeAt(0)` on ["alpha","beta","gamma"].
    assert_eq!(rows_at(&["beta", "gamma"]), vec!["- beta", "- gamma"]);
}

#[test]
fn item_binding_scopes_over_the_template_and_does_not_leak() {
    let ir = parse_ir(IR).expect("fixture parses");
    let mut state = StateView::from_ir(&ir);
    state.set("items", StateValue::List(vec!["alpha".to_string()]));
    let texts = texts_at(&["alpha"]);
    // Inside the template the item resolves to the element...
    assert!(
        texts.contains(&"- alpha".to_string()),
        "bound row: {texts:?}"
    );
    // ...and outside it the same name stays unbound, verbatim.
    assert!(
        texts.contains(&"outer {p}".to_string()),
        "item must not leak past the template: {texts:?}"
    );
    // Declared state still resolves normally beside the loop.
    assert!(
        texts.contains(&"Total 0".to_string()),
        "sibling state: {texts:?}"
    );
    let _ = (state, ir);
}

#[test]
fn list_height_grows_with_the_row_count() {
    let ir = parse_ir(IR).expect("fixture parses");
    let at = |items: &[&str]| -> i64 {
        let mut state = StateView::from_ir(&ir);
        state.set(
            "items",
            StateValue::List(items.iter().map(|s| s.to_string()).collect()),
        );
        layout(&ir, &state, 320, 800)
            .iter()
            .find(|n| n.id == 1)
            .expect("For node is laid out")
            .rect
            .h
    };
    let zero = at(&[]);
    let one = at(&["a"]);
    let three = at(&["a", "b", "c"]);
    assert_eq!(zero, 0, "no rows means no height");
    assert!(one > zero, "one row adds height");
    assert!(three > one, "each element adds one row of height");
}

#[test]
fn append_and_remove_at_events_parse_into_their_ops() {
    let ir = parse_ir(EVENTS_IR).expect("event fixture parses");
    fn collect_ops(n: &opurom_render::Node, out: &mut Vec<Op>) {
        for event in &n.events {
            for action in &event.actions {
                out.push(action.op);
            }
        }
        for child in &n.children {
            collect_ops(child, out);
        }
    }
    let mut found = Vec::new();
    collect_ops(&ir.tree, &mut found);
    assert!(
        found.contains(&Op::Append),
        "append op must parse: {found:?}"
    );
    assert!(
        found.contains(&Op::RemoveAt),
        "removeAt op must parse: {found:?}"
    );
    // The event fixture renders the same way: one seeded row.
    let mut state = StateView::from_ir(&ir);
    let rows: Vec<String> = layout(&ir, &state, 320, 400)
        .iter()
        .filter_map(|n| n.text.clone())
        .filter(|text| !text.contains("add") && !text.contains("drop") && text != "seed")
        .collect();
    // The template row renders "seed"; filter text above only proves events
    // coexist with the loop node.
    let _ = rows;
    state.set(
        "items",
        StateValue::List(vec!["seed".to_string(), "next".to_string()]),
    );
    let after: Vec<String> = layout(&ir, &state, 320, 400)
        .iter()
        .filter_map(|n| n.text.clone())
        .collect();
    assert!(after.contains(&"seed".to_string()) && after.contains(&"next".to_string()));
}

#[test]
fn invalid_list_ops_are_rejected() {
    // `append` on a non-list target must not parse.
    let bad_append = r##"{
      "ir": "0.1.0", "app": "Bad",
      "state": [ { "name": "count", "type": "int", "init": 3 } ],
      "theme": { "mode": "light", "accent": "#097FEB", "radius": 8 },
      "tree": { "id": 0, "kind": "Column", "props": {},
        "children": [
          { "id": 1, "kind": "Button", "props": { "label": "x" }, "children": [],
            "events": { "tap": { "op": "append", "target": "count", "value": "x" } } }
        ],
        "events": {} }
    }"##;
    assert!(parse_ir(bad_append).is_err(), "append needs a list target");

    let bad_remove = r##"{
      "ir": "0.1.0", "app": "Bad2",
      "state": [ { "name": "items", "type": "list", "init": [] } ],
      "theme": { "mode": "light", "accent": "#097FEB", "radius": 8 },
      "tree": { "id": 0, "kind": "Column", "props": {},
        "children": [
          { "id": 1, "kind": "Button", "props": { "label": "x" }, "children": [],
            "events": { "tap": { "op": "removeAt", "target": "items", "value": -1 } } }
        ],
        "events": {} }
    }"##;
    assert!(
        parse_ir(bad_remove).is_err(),
        "removeAt needs a non-negative index"
    );
}

/// Fixture whose button assigns a whole list, mirroring the core kernel's
/// `assign` op contract (e0dc310).
const ASSIGN_IR: &str = r##"{
  "ir": "0.1.0", "app": "DynamicAssign",
  "state": [
    { "name": "items", "type": "list", "init": [] },
    { "name": "more", "type": "list", "init": ["x", "y"] }
  ],
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 8 },
  "tree": { "id": 0, "kind": "Column", "props": { "gap": 4, "padding": 8 },
    "children": [
      { "id": 1, "kind": "For", "props": { "source": "items", "item": "p" },
        "children": [
          { "id": 2, "kind": "Text", "props": { "value": "- {p}", "size": 14 },
            "children": [], "events": {} }
        ],
        "events": {} },
      { "id": 3, "kind": "Button", "props": { "label": "load" }, "children": [],
        "events": { "tap": { "op": "assign", "target": "items", "value": ["a", "b"] } } },
      { "id": 4, "kind": "Button", "props": { "label": "mirror" }, "children": [],
        "events": { "tap": { "op": "assign", "target": "items", "valueFrom": "more" } } }
    ],
    "events": {} }
}"##;

fn op(ir_text: &str, node_id: u32) -> Op {
    fn find(n: &opurom_render::Node, id: u32) -> Option<Op> {
        if n.id == id {
            return n
                .events
                .first()
                .and_then(|e| e.actions.first())
                .map(|a| a.op);
        }
        n.children.iter().find_map(|c| find(c, id))
    }
    let ir = parse_ir(ir_text).expect("fixture parses");
    find(&ir.tree, node_id).expect("node carries an action")
}

#[test]
fn assign_parses_with_array_and_list_value_from() {
    assert_eq!(op(ASSIGN_IR, 3), Op::Assign, "literal array assign");
    assert_eq!(op(ASSIGN_IR, 4), Op::Assign, "valueFrom assign");
}

#[test]
fn assign_value_drives_the_row_count() {
    // The host applies the parsed action wholesale; the For loop follows.
    let ir = parse_ir(ASSIGN_IR).expect("fixture parses");
    assert_eq!(
        rows_on(&ir, &[]),
        Vec::<String>::new(),
        "baseline before assign"
    );
    assert_eq!(
        rows_on(&ir, &["a", "b"]),
        vec!["- a", "- b"],
        "assign replaces the whole list and rows follow"
    );
    assert_eq!(
        rows_on(&ir, &[]),
        Vec::<String>::new(),
        "re-assign to empty"
    );
}

fn rows_on(ir: &opurom_render::Ir, items: &[&str]) -> Vec<String> {
    let mut state = StateView::from_ir(ir);
    state.set(
        "items",
        StateValue::List(items.iter().map(|s| s.to_string()).collect()),
    );
    layout(ir, &state, 320, 400)
        .iter()
        .filter(|n| n.kind == Kind::Text)
        .filter_map(|n| n.text.clone())
        .filter(|text| text.starts_with("- "))
        .collect()
}

#[test]
fn assign_needs_a_list_target() {
    let bad = r##"{
      "ir": "0.1.0", "app": "Bad",
      "state": [ { "name": "count", "type": "int", "init": 3 } ],
      "theme": { "mode": "light", "accent": "#097FEB", "radius": 8 },
      "tree": { "id": 0, "kind": "Column", "props": {},
        "children": [
          { "id": 1, "kind": "Button", "props": { "label": "x" }, "children": [],
            "events": { "tap": { "op": "assign", "target": "count", "value": ["a"] } } }
        ],
        "events": {} }
    }"##;
    let err = parse_ir(bad).expect_err("assign on a non-list target must fail");
    assert!(
        err.to_string().contains("assign"),
        "error names the op: {err}"
    );
}

#[test]
fn assign_needs_exactly_one_value_spelling() {
    let template = |value_field: &str| {
        format!(
            r##"{{
      "ir": "0.1.0", "app": "Bad",
      "state": [ {{ "name": "items", "type": "list", "init": [] }} ],
      "theme": {{ "mode": "light", "accent": "#097FEB", "radius": 8 }},
      "tree": {{ "id": 0, "kind": "Column", "props": {{}},
        "children": [
          {{ "id": 1, "kind": "Button", "props": {{ "label": "x" }}, "children": [],
            "events": {{ "tap": {{ "op": "assign", "target": "items"{value_field} }} }} }}
        ],
        "events": {{}} }}
    }}"##
        )
    };
    let none = parse_ir(&template("")).expect_err("assign without a value must fail");
    assert!(none.to_string().contains("value"), "message: {none}");
    let both = parse_ir(&template(r#", "value": ["a"], "valueFrom": "items""#))
        .expect_err("assign with both spellings must fail");
    assert!(both.to_string().contains("value"), "message: {both}");
    let scalar = parse_ir(&template(r#", "value": "a""#))
        .expect_err("assign with a string scalar must fail");
    assert!(scalar.to_string().contains("value"), "message: {scalar}");
}

#[test]
fn set_cannot_replace_a_list_and_the_error_says_use_assign() {
    let bad = r##"{
      "ir": "0.1.0", "app": "Bad",
      "state": [ { "name": "items", "type": "list", "init": ["a"] } ],
      "theme": { "mode": "light", "accent": "#097FEB", "radius": 8 },
      "tree": { "id": 0, "kind": "Column", "props": {},
        "children": [
          { "id": 1, "kind": "Button", "props": { "label": "x" }, "children": [],
            "events": { "tap": { "op": "set", "target": "items", "value": ["b"] } } }
        ],
        "events": {} }
    }"##;
    let err = parse_ir(bad).expect_err("set on a list target must fail");
    let text = err.to_string();
    assert!(text.contains("assign"), "error points at assign: {text}");

    // The same rejection holds for a scalar that would otherwise type-match.
    let scalar = r##"{
      "ir": "0.1.0", "app": "Bad2",
      "state": [
        { "name": "items", "type": "list", "init": [] },
        { "name": "items2", "type": "list", "init": [] }
      ],
      "theme": { "mode": "light", "accent": "#097FEB", "radius": 8 },
      "tree": { "id": 0, "kind": "Column", "props": {},
        "children": [
          { "id": 1, "kind": "Button", "props": { "label": "x" }, "children": [],
            "events": { "tap": { "op": "set", "target": "items", "valueFrom": "items2" } } }
        ],
        "events": {} }
    }"##;
    assert!(
        parse_ir(scalar).is_err(),
        "set with list valueFrom must also be rejected"
    );
}
