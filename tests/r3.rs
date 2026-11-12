//! R3 renderer tests: font scaling, high contrast, theme contrast tokens and
//! the data-only accessibility tree (pre-order roles, labels, bounds, hidden).

use opurom_render::{
    a11y_json, a11y_tree, digest_hex, layout, parse_ir, Contrast, Ir, LaidOut, Renderer, Rgba,
    Role, StateValue, StateView,
};
use serde_json::Value;
use std::{fs, path::PathBuf, process::Command};

fn ir_of(text: &str) -> Ir {
    parse_ir(text).expect("fixture IR must parse")
}

fn node<'a>(laid: &'a [LaidOut], id: u32) -> &'a LaidOut {
    laid.iter().find(|n| n.id == id).expect("node id missing")
}

fn digest_of(ir: &Ir, state: &StateView) -> String {
    digest_hex(&Renderer::new(320, 240).pixels(ir, state))
}

const SCALE_IR: &str = r##"{
  "ir": "0.1", "app": "Scale",
  "state": [],
  "tree": { "id": 0, "kind": "Column", "props": { "gap": 4, "padding": 8 },
    "children": [
      { "id": 1, "kind": "Text", "props": { "value": "Scale me" }, "children": [], "events": {} },
      { "id": 2, "kind": "Container", "props": { "width": 80, "height": 40 },
        "children": [], "events": {} }
    ], "events": {} },
  "theme": { "mode": "light", "accent": "#4F46E5", "radius": 4 }
}"##;

fn with_font_scale(value: &str) -> String {
    SCALE_IR.replace(
        "\"value\": \"Scale me\"",
        &format!("\"value\": \"Scale me\", \"fontScale\": {value}"),
    )
}

fn with_theme_contrast(value: &str) -> String {
    SCALE_IR.replace(
        "\"radius\": 4 }",
        &format!("\"radius\": 4, \"tokens\": {{ \"contrast\": \"{value}\" }} }}"),
    )
}

// ------------------------------------------------------------- font scaling

#[test]
fn font_scale_changes_pixels_and_stays_deterministic() {
    let base = ir_of(SCALE_IR);
    let scaled = ir_of(&with_font_scale("2.0"));
    let one = ir_of(&with_font_scale("1.0"));
    let state = StateView::from_ir(&base);

    let base_laid = layout(&base, &state, 320, 240);
    let scaled_laid = layout(&scaled, &state, 320, 240);
    assert!(
        node(&scaled_laid, 1).text_scale > node(&base_laid, 1).text_scale,
        "fontScale must grow the integer glyph block"
    );
    let base_digest = digest_of(&base, &state);
    let scaled_digest = digest_of(&scaled, &state);
    assert_ne!(base_digest, scaled_digest);
    assert_eq!(scaled_digest, digest_of(&scaled, &state));
    // 1.0 is the identity in exact fixed point, so it cannot drift the pixels.
    assert_eq!(base_digest, digest_of(&one, &state));
}

#[test]
fn font_scale_is_exact_fixed_point_within_range() {
    for (literal, fixed) in [("0.5", 512), ("1.0", 1024), ("2.0", 2048), ("3.0", 3072)] {
        let ir = ir_of(&with_font_scale(literal));
        assert_eq!(ir.tree.children[0].props.font_scale, Some(fixed));
    }
}

#[test]
fn invalid_font_scale_is_rejected_with_a_readable_diagnostic() {
    for value in ["0.49", "3.01", "0", "-1", "\"2\"", "true", "null"] {
        let bad = with_font_scale(value);
        let err = parse_ir(&bad).expect_err("invalid fontScale must be rejected");
        assert!(
            err.to_string().contains("fontScale"),
            "unexpected diagnostic for {value}: {err}"
        );
    }

    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ir_path = dir.join("bad-fontscale.opuir.json");
    fs::write(&ir_path, with_font_scale("3.01")).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_opurom-render"))
        .arg(&ir_path)
        .args(["320", "240"])
        .arg(dir.join("bad-fontscale.ppm"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("fontScale"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("error:"));
    assert!(!stderr.contains("panicked"));
}

// ------------------------------------------------------------ high contrast

#[test]
fn high_contrast_changes_pixels_deterministically() {
    let base = ir_of(SCALE_IR);
    let node_hc = ir_of(&SCALE_IR.replace(
        "\"gap\": 4, \"padding\": 8",
        "\"gap\": 4, \"padding\": 8, \"highContrast\": true",
    ));
    let theme_hc = ir_of(&with_theme_contrast("high"));
    let state = StateView::from_ir(&base);

    let base_digest = digest_of(&base, &state);
    let node_digest = digest_of(&node_hc, &state);
    let theme_digest = digest_of(&theme_hc, &state);
    assert_ne!(base_digest, node_digest);
    assert_ne!(base_digest, theme_digest);
    assert_eq!(node_digest, digest_of(&node_hc, &state));
    // A root highContrast flag and a theme-level token describe the same frame.
    assert_eq!(node_digest, theme_digest);

    let laid = layout(&node_hc, &state, 320, 240);
    let black = Rgba::rgb(0, 0, 0);
    // The prop inherits to descendants: the Text is pure black, not the theme fg.
    assert_eq!(node(&laid, 1).text_color, Some(black));
    // Box-owning nodes get a stronger 2 px black/white border.
    assert_eq!(node(&laid, 2).border_w, 2);
    assert_eq!(node(&laid, 2).border, Some(black));
    assert_eq!(node(&laid, 0).border_w, 2);
}

#[test]
fn theme_contrast_token_is_honoured_and_validated() {
    let normal = ir_of(&with_theme_contrast("normal"));
    let high = ir_of(&with_theme_contrast("high"));
    let state = StateView::from_ir(&normal);
    assert_eq!(normal.theme.tokens.contrast, Contrast::Normal);
    assert_eq!(high.theme.tokens.contrast, Contrast::High);
    assert_eq!(
        digest_of(&normal, &state),
        digest_of(&ir_of(SCALE_IR), &state)
    );
    assert_ne!(digest_of(&normal, &state), digest_of(&high, &state));

    let err = parse_ir(&with_theme_contrast("extreme"))
        .expect_err("an unknown contrast token must be rejected");
    assert!(err.to_string().contains("contrast"));
}

#[test]
fn non_bool_high_contrast_and_unknown_role_are_rejected() {
    let bad_hc = SCALE_IR.replace("\"gap\": 4", "\"gap\": 4, \"highContrast\": \"yes\"");
    let err = parse_ir(&bad_hc).expect_err("a non-bool highContrast must be rejected");
    assert!(err.to_string().contains("highContrast"));

    let bad_role = SCALE_IR.replace(
        "\"value\": \"Scale me\"",
        "\"value\": \"Scale me\", \"role\": \"dialog\"",
    );
    let err = parse_ir(&bad_role).expect_err("an unknown role must be rejected");
    assert!(err.to_string().contains("role"));
}

#[test]
fn unknown_node_fields_are_ignored() {
    // The R3 catalog adds an optional `component` field next to `kind`.
    let with_component = SCALE_IR.replace(
        "\"kind\": \"Column\"",
        "\"kind\": \"Column\", \"component\": \"FancyCard\"",
    );
    let base = ir_of(SCALE_IR);
    let state = StateView::from_ir(&base);
    assert_eq!(
        digest_of(&ir_of(&with_component), &state),
        digest_of(&base, &state)
    );
}

// ------------------------------------------------------------ a11y tree

const A11Y_IR: &str = r##"{
  "ir": "0.1", "app": "A11y",
  "state": [
    { "name": "show", "type": "bool", "init": false },
    { "name": "on", "type": "bool", "init": true },
    { "name": "vol", "type": "int", "init": 30 }
  ],
  "tree": { "id": 0, "kind": "Column", "props": { "gap": 2, "padding": 4 },
    "children": [
      { "id": 1, "kind": "Text", "props": { "value": "Hello" }, "children": [], "events": {} },
      { "id": 2, "kind": "Text", "props": { "value": "Big", "size": 28 }, "children": [], "events": {} },
      { "id": 3, "kind": "Button", "props": { "label": "Go" }, "children": [], "events": {} },
      { "id": 4, "kind": "Image", "props": { "src": "logo.png", "label": "Logo" },
        "children": [], "events": {} },
      { "id": 5, "kind": "Checkbox", "props": { "bind": "on", "label": "Accept" },
        "children": [], "events": {} },
      { "id": 6, "kind": "Slider", "props": { "bind": "vol", "min": 0, "max": 100 },
        "children": [], "events": {} },
      { "id": 7, "kind": "Switch", "props": { "bind": "on" }, "children": [], "events": {} },
      { "id": 8, "kind": "Text", "props": { "value": "Custom", "role": "button" },
        "children": [], "events": {} },
      { "id": 9, "kind": "Container", "props": { "width": 40, "height": 30, "title": "Panel" },
        "when": "show",
        "children": [
          { "id": 10, "kind": "Text", "props": { "value": "Hidden" }, "children": [], "events": {} }
        ], "events": {} }
    ], "events": {} },
  "theme": { "mode": "light", "accent": "#4F46E5", "radius": 4 }
}"##;

#[test]
fn a11y_tree_has_pre_order_roles_labels_bounds_and_hidden() {
    let ir = ir_of(A11Y_IR);
    let state = StateView::from_ir(&ir);
    let tree = a11y_tree(&ir, &state, 320, 240);

    assert_eq!(
        tree.iter().map(|n| n.id).collect::<Vec<u32>>(),
        (0..=10).collect::<Vec<u32>>(),
        "the tree must be pre-order and include hidden nodes"
    );
    let roles = [
        Role::None,
        Role::Text,
        Role::Heading,
        Role::Button,
        Role::Image,
        Role::Checkbox,
        Role::Slider,
        Role::Checkbox,
        Role::Button,
        Role::None,
        Role::Text,
    ];
    let labels = [
        "", "Hello", "Big", "Go", "Logo", "Accept", "", "", "Custom", "Panel", "Hidden",
    ];
    for (i, (&role, &label)) in roles.iter().zip(labels.iter()).enumerate() {
        assert_eq!(tree[i].role, role, "role of node {i}");
        assert_eq!(tree[i].label, label, "label of node {i}");
        assert_eq!(tree[i].hidden, i >= 9, "hidden flag of node {i}");
    }

    let laid = layout(&ir, &state, 320, 240);
    for id in 0..=8u32 {
        let r = node(&laid, id).rect;
        assert_eq!(tree[id as usize].bounds, [r.x, r.y, r.w, r.h]);
    }
    let mut expanded = state.clone();
    expanded.set("show", StateValue::Bool(true));
    expanded.set("on", StateValue::Bool(true));
    let expanded_laid = layout(&ir, &expanded, 320, 240);
    for id in 9..=10u32 {
        let r = node(&expanded_laid, id).rect;
        assert_eq!(tree[id as usize].bounds, [r.x, r.y, r.w, r.h]);
        assert_ne!(tree[id as usize].bounds, [0, 0, 0, 0]);
    }
    for n in &tree {
        assert!(n.bounds.iter().all(|v| *v >= 0), "bounds are integers");
    }
    assert_eq!(
        a11y_json(&ir, &state, 320, 240),
        a11y_json(&ir, &state, 320, 240)
    );
}

#[test]
fn cli_writes_byte_identical_a11y_json() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ir_path = dir.join("r3-a11y.opuir.json");
    fs::write(&ir_path, A11Y_IR).unwrap();
    let exe = env!("CARGO_BIN_EXE_opurom-render");
    let ppm = dir.join("r3-a11y.ppm");
    let out1 = dir.join("r3-a11y-1.json");
    let out2 = dir.join("r3-a11y-2.json");
    let run = |json: &PathBuf| -> String {
        let out = Command::new(exe)
            .arg(&ir_path)
            .args(["320", "240"])
            .arg(&ppm)
            .arg("--a11y")
            .arg(json)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    };
    let stdout1 = run(&out1);
    assert_eq!(stdout1, run(&out2));
    assert_eq!(stdout1.lines().count(), 2, "stdout stays digest + dirty");
    assert!(stdout1.starts_with("digest="));
    assert!(stdout1.contains("dirty=1"));

    let bytes1 = fs::read(&out1).unwrap();
    assert_eq!(bytes1, fs::read(&out2).unwrap());
    let ir = ir_of(A11Y_IR);
    let state = StateView::from_ir(&ir);
    assert_eq!(
        String::from_utf8(bytes1.clone()).unwrap(),
        a11y_json(&ir, &state, 320, 240)
    );

    let value: Value = serde_json::from_slice(&bytes1).unwrap();
    let array = value.as_array().unwrap();
    assert_eq!(array.len(), 11);
    for item in array {
        let object = item.as_object().unwrap();
        assert!(object.get("id").unwrap().is_u64());
        assert!(object.get("role").unwrap().is_string());
        assert!(object.get("label").unwrap().is_string());
        assert_eq!(object.get("bounds").unwrap().as_array().unwrap().len(), 4);
        assert!(object
            .get("bounds")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .all(|n| n.is_i64() || n.is_u64()));
        assert!(object.get("hidden").unwrap().is_boolean());
    }

    let missing = Command::new(exe)
        .arg(&ir_path)
        .args(["320", "240"])
        .arg(&ppm)
        .arg("--a11y")
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("--a11y requires"));
}

// -------------------------------------------------------------- regressions

#[test]
fn v0_1_golden_digests_are_unchanged_by_r3() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let exe = env!("CARGO_BIN_EXE_opurom-render");
    let ir = "../opurom-spec/counter.opuir.json";
    let run = |w: &str, h: &str, out: &PathBuf, extra: &[&str]| -> String {
        let output = Command::new(exe)
            .arg(ir)
            .args([w, h])
            .arg(out)
            .args(extra)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).to_string()
    };
    assert!(
        run("320", "240", &dir.join("r3-golden-320.ppm"), &[]).contains("digest=c87bf023a6c01f9d")
    );
    assert!(
        run("800", "600", &dir.join("r3-golden-800.ppm"), &[]).contains("digest=8cc3021fa80749b5")
    );
    assert!(run(
        "800",
        "600",
        &dir.join("r3-golden-7.ppm"),
        &["--state", "count=7"]
    )
    .contains("digest=e7aad51c98d9cf21"));
}
