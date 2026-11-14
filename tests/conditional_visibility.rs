//! Tests for the inverted conditional visibility node field.

use opurom_render::{
    a11y_tree, digest_hex, layout, parse_ir, svg, Ir, Renderer, StateValue, StateView,
};
use std::process::Command;

fn ir_of(text: &str) -> Ir {
    parse_ir(text).expect("fixture IR must parse")
}

const UNLESS_IR: &str = r##"{
  "ir": "0.1", "app": "Else",
  "state": [ { "name": "dark", "type": "bool", "init": false } ],
  "tree": { "id": 0, "kind": "Column", "props": { "gap": 4, "padding": 8 },
    "children": [
      { "id": 1, "kind": "Text", "props": { "value": "always" }, "children": [], "events": {} },
      { "id": 2, "kind": "Container", "props": { "width": 40, "height": 40, "color": "#FF0000" },
        "when": "dark", "children": [
          { "id": 3, "kind": "Text", "props": { "value": "dark" }, "children": [], "events": {} }
        ], "events": {} },
      { "id": 4, "kind": "Container", "props": { "width": 40, "height": 40, "color": "#0000FF" },
        "unless": "dark", "children": [
          { "id": 5, "kind": "Text", "props": { "value": "light" }, "children": [], "events": {} }
        ], "events": {} }
    ], "events": {} },
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 }
}"##;

const UNLESS_ONLY: &str = r##"{
  "ir": "0.1", "app": "Else",
  "state": [ { "name": "dark", "type": "bool", "init": false } ],
  "tree": { "id": 0, "kind": "Text", "props": { "value": "conditional" },
    "unless": "dark", "children": [], "events": {} },
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 }
}"##;

#[test]
fn unless_inverts_when_and_hides_the_subtree() {
    let ir = ir_of(UNLESS_IR);
    let mut state = StateView::from_ir(&ir);

    let light = layout(&ir, &state, 200, 120);
    assert!(light.iter().any(|n| n.id == 4));
    assert!(light.iter().any(|n| n.id == 5));
    assert!(light.iter().all(|n| n.id != 2 && n.id != 3));

    state.set("dark", StateValue::Bool(true));
    let dark = layout(&ir, &state, 200, 120);
    assert!(dark.iter().any(|n| n.id == 2));
    assert!(dark.iter().any(|n| n.id == 3));
    assert!(dark.iter().all(|n| n.id != 4 && n.id != 5));
}

#[test]
fn unless_changes_the_frame_and_keeps_dirty_rects_bounded() {
    let ir = ir_of(UNLESS_IR);
    let mut state = StateView::from_ir(&ir);
    let mut renderer = Renderer::new(200, 120);
    let before = renderer.render(&ir, &state);
    state.set("dark", StateValue::Bool(true));
    let after = renderer.render(&ir, &state);
    assert_ne!(digest_hex(&before.pixels), digest_hex(&after.pixels));
    assert!(!after.dirty.is_empty());
    assert!(after.dirty.len() <= 64);
    for r in &after.dirty {
        assert!(r.w > 0 && r.h > 0);
        assert!(r.x + r.w <= 200 && r.y + r.h <= 120);
    }
    let repeated = renderer.render(&ir, &state);
    assert_eq!(digest_hex(&after.pixels), digest_hex(&repeated.pixels));
    assert!(repeated.dirty.is_empty());
}

#[test]
fn unless_requires_a_declared_bool_state() {
    let undeclared = UNLESS_ONLY.replace("\"unless\": \"dark\"", "\"unless\": \"missing\"");
    let err = parse_ir(&undeclared).expect_err("an undeclared state must be rejected");
    assert!(
        err.to_string()
            .contains("unless refers to undeclared state"),
        "unexpected diagnostic: {err}"
    );

    let non_bool = UNLESS_ONLY.replace(
        "{ \"name\": \"dark\", \"type\": \"bool\", \"init\": false }",
        "{ \"name\": \"dark\", \"type\": \"int\", \"init\": 0 }",
    );
    let err = parse_ir(&non_bool).expect_err("a non-bool state must be rejected");
    assert!(
        err.to_string()
            .contains("unless must reference a bool state"),
        "unexpected diagnostic: {err}"
    );

    let not_a_string = UNLESS_ONLY.replace("\"unless\": \"dark\"", "\"unless\": true");
    let err = parse_ir(&not_a_string).expect_err("a non-string condition must be rejected");
    assert!(
        err.to_string().contains("unless must be a string"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn when_and_unless_on_one_node_both_must_hold() {
    let ir = ir_of(
        r##"{
  "ir": "0.1", "app": "Both",
  "state": [
    { "name": "on", "type": "bool", "init": false },
    { "name": "off", "type": "bool", "init": false }
  ],
  "tree": { "id": 0, "kind": "Column", "props": {},
    "children": [
      { "id": 1, "kind": "Text", "props": { "value": "conditional" },
        "when": "on", "unless": "off", "children": [], "events": {} }
    ], "events": {} },
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 }
}"##,
    );
    let state = StateView::from_ir(&ir);
    for (on, off, expected) in [
        (false, false, false),
        (false, true, false),
        (true, false, true),
        (true, true, false),
    ] {
        let mut view = state.clone();
        view.set("on", StateValue::Bool(on));
        view.set("off", StateValue::Bool(off));
        let laid = layout(&ir, &view, 200, 120);
        assert_eq!(
            laid.iter().any(|n| n.id == 1),
            expected,
            "on={on}, off={off}"
        );
    }
}

#[test]
fn unless_hidden_nodes_are_marked_in_the_a11y_tree_and_svg() {
    let ir = ir_of(UNLESS_IR);
    let light_state = StateView::from_ir(&ir);
    let mut dark_state = light_state.clone();
    dark_state.set("dark", StateValue::Bool(true));

    let light = a11y_tree(&ir, &light_state, 200, 120);
    let hidden: Vec<u32> = light.iter().filter(|n| n.hidden).map(|n| n.id).collect();
    assert_eq!(hidden, vec![2, 3]);
    let dark = a11y_tree(&ir, &dark_state, 200, 120);
    let hidden: Vec<u32> = dark.iter().filter(|n| n.hidden).map(|n| n.id).collect();
    assert_eq!(hidden, vec![4, 5]);

    let light_svg = svg(&ir, &light_state, 200, 120);
    assert_eq!(light_svg.matches("data-hidden=\"true\"").count(), 2);
    assert!(light_svg.contains("data-id=\"2\" data-role=\"none\" data-hidden=\"true\""));
    let dark_svg = svg(&ir, &dark_state, 200, 120);
    assert_eq!(dark_svg.matches("data-hidden=\"true\"").count(), 2);
    assert!(dark_svg.contains("data-id=\"4\" data-role=\"none\" data-hidden=\"true\""));
    assert_ne!(light_svg, dark_svg);
}

#[test]
fn unless_leaves_the_golden_digests_unchanged() {
    let counter = std::fs::read_to_string("../opurom-spec/counter.opuir.json")
        .expect("the golden IR fixture must be readable");
    let ir = ir_of(&counter);
    let state = StateView::from_ir(&ir);
    assert_eq!(
        digest_hex(&Renderer::new(320, 240).pixels(&ir, &state)),
        "8d45f412996a99c6"
    );
    assert_eq!(
        digest_hex(&Renderer::new(800, 600).pixels(&ir, &state)),
        "318beb1dd337bc4e"
    );
    let mut overridden = state.clone();
    overridden.set("count", StateValue::Int(7));
    assert_eq!(
        digest_hex(&Renderer::new(800, 600).pixels(&ir, &overridden)),
        "40073cb95c912d80"
    );
}

#[test]
fn the_cli_accepts_unless_and_state_overrides() {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ir_path = dir.join("unless.opuir.json");
    std::fs::write(&ir_path, UNLESS_ONLY).unwrap();
    let exe = env!("CARGO_BIN_EXE_opurom-render");
    let run = |value: &str| {
        let out = Command::new(exe)
            .arg(&ir_path)
            .args(["200", "120"])
            .arg(dir.join("unless.ppm"))
            .args(["--state", value])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    };
    let shown = run("dark=false");
    let hidden = run("dark=true");
    assert_ne!(shown, hidden);
    assert!(shown.starts_with("digest="));
    assert!(shown.contains("dirty=1"));
    assert_eq!(shown, run("dark=false"));
}
