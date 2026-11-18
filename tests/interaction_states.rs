//! Eight-state matrix, variant/tone matrix, CLI override and golden regressions.

use opurom_render::{
    digest_hex, layout, parse_ir, Node, NodeState, Renderer, StateValue, StateView, CYAN,
    FOCUS_RING, PRIMARY, SUCCESS,
};
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn compat_renderer(width: u32, height: u32) -> opurom_render::Renderer {
    let mut renderer = opurom_render::Renderer::new(width, height);
    renderer.set_quality(opurom_render::QualityProfile::Compat);
    renderer
}

const W: u32 = 160;
const H: u32 = 96;
const UNIT: i64 = 1024;

const COUNTER_IR: &str = r##"{
  "ir": "0.1.0",
  "app": "Counter",
  "state": [ { "name": "count", "type": "int", "init": 0 } ],
  "tree": {
    "id": 0, "kind": "Column",
    "props": { "gap": 8, "padding": 16, "align": "center" },
    "children": [
      { "id": 1, "kind": "Text", "props": { "value": "Count: {count}", "size": 24 },
        "children": [], "events": {} },
      { "id": 2, "kind": "Row", "props": { "gap": 8 }, "children": [
          { "id": 3, "kind": "Button", "props": { "label": "-" }, "children": [],
            "events": { "tap": { "op": "add", "target": "count", "value": -1 } } },
          { "id": 4, "kind": "Button", "props": { "label": "+" }, "children": [],
            "events": { "tap": { "op": "add", "target": "count", "value": 1 } } }
        ], "events": {} }
    ], "events": {}
  },
  "theme": { "mode": "system", "accent": "#097FEB", "radius": 12 }
}"##;

fn ir_for(kind: &str, extra: Value) -> String {
    let mut props = json!({
        "width": 80,
        "height": 24,
        "label": "Go",
        "placeholder": "hello",
        "value": "a"
    });
    match kind {
        "Input" => props["bind"] = json!("text"),
        "Switch" | "Checkbox" => props["bind"] = json!("on"),
        "Radio" => props["bind"] = json!("choice"),
        "Slider" => props["bind"] = json!("amount"),
        _ => {}
    }
    if let Value::Object(extra) = extra {
        for (key, value) in extra {
            props[key.as_str()] = value;
        }
    }
    serde_json::to_string(&json!({
        "ir": "0.1.0", "app": "States",
        "state": [
            { "name": "on", "type": "bool", "init": true },
            { "name": "text", "type": "string", "init": "hello" },
            { "name": "choice", "type": "string", "init": "a" },
            { "name": "amount", "type": "int", "init": 50 },
            { "name": "count", "type": "int", "init": 0 }
        ],
        "tree": {
            "id": 0, "kind": "Column", "props": { "padding": 8, "align": "start" },
            "children": [
                { "id": 1, "kind": kind, "props": props, "children": [], "events": {} }
            ], "events": {}
        },
        "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 }
    }))
    .unwrap()
}

fn matrix_ir(state: Option<&str>, disabled: Option<bool>) -> String {
    let contracts = [
        ("Button", json!({ "width": 60, "height": 24, "label": "A" })),
        (
            "Checkbox",
            json!({ "width": 24, "height": 24, "bind": "on" }),
        ),
        (
            "Slider",
            json!({ "width": 60, "height": 12, "bind": "amount" }),
        ),
    ];
    let children: Vec<Value> = contracts
        .into_iter()
        .enumerate()
        .map(|(i, (kind, mut props))| {
            if let Some(s) = state {
                props["state"] = json!(s);
            }
            if let Some(d) = disabled {
                props["disabled"] = json!(d);
            }
            json!({
                "id": i + 1, "kind": kind, "props": props,
                "children": [], "events": {}
            })
        })
        .collect();
    serde_json::to_string(&json!({
        "ir": "0.1.0", "app": "Matrix",
        "state": [
            { "name": "on", "type": "bool", "init": true },
            { "name": "amount", "type": "int", "init": 50 },
            { "name": "count", "type": "int", "init": 0 }
        ],
        "tree": {
            "id": 0, "kind": "Column",
            "props": { "padding": 8, "gap": 4, "align": "start" },
            "children": children, "events": {}
        },
        "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 }
    }))
    .unwrap()
}

fn render(kind: &str, extra: Value) -> Vec<u8> {
    let ir = parse_ir(&ir_for(kind, extra)).unwrap();
    let state = StateView::from_ir(&ir);
    compat_renderer(W, H).pixels(&ir, &state)
}

fn digest(kind: &str, extra: Value) -> String {
    digest_hex(&render(kind, extra))
}

fn laid(kind: &str, extra: Value) -> opurom_render::LaidOut {
    let ir = parse_ir(&ir_for(kind, extra)).unwrap();
    let state = StateView::from_ir(&ir);
    layout(&ir, &state, W, H)
        .into_iter()
        .find(|n| n.id == 1)
        .unwrap()
}

fn pixel(pixels: &[u8], x: i64, y: i64) -> [u8; 4] {
    assert!(x >= 0 && y >= 0 && x < W as i64 && y < H as i64);
    let i = (y as usize * W as usize + x as usize) * 4;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

fn no_new_props(n: &Node) -> bool {
    n.props.state == NodeState::Default
        && !n.props.disabled
        && n.props.variant.is_none()
        && n.props.tone.is_none()
        && n.children.iter().all(no_new_props)
}

fn cli(exe: &str, ir: &Path, w: &str, h: &str, out: &Path, extra: &[&str]) -> Output {
    Command::new(exe)
        .arg(ir)
        .args([w, h])
        .arg(out)
        .args(extra)
        .output()
        .unwrap()
}

fn stdout(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn five_states_are_pairwise_distinct_and_deterministic_for_five_controls() {
    for kind in ["Button", "Input", "Checkbox", "Switch", "Slider"] {
        let mut digests = HashSet::new();
        for state in ["default", "hover", "active", "focus", "disabled"] {
            let extra = if state == "default" {
                json!({})
            } else {
                json!({ "state": state })
            };
            let first = render(kind, extra.clone());
            let second = render(kind, extra);
            assert_eq!(first, second, "{kind} {state} must be byte-identical");
            assert!(
                digests.insert(digest_hex(&first)),
                "{kind} {state} repeats an earlier pixel frame"
            );
        }
        assert_eq!(digests.len(), 5, "{kind}");
    }
}

#[test]
fn eight_states_are_pairwise_distinct_and_new_business_states_are_pinned() {
    // Pins generated at 160x96 from ir_for(kind, state); the five legacy
    // defaults were verified byte-identical against the pre-change tree.
    let pins: &[(&str, &[(&str, &str)]); 5] = &[
        (
            "Button",
            &[
                ("default", "609464e4b1333421"),
                ("hover", "20067c304cb654d2"),
                ("active", "92cc5d9376c7b1c3"),
                ("focus", "f3a61407a786a2d1"),
                ("disabled", "9579ea1f9d0a4233"),
                ("loading", "8083a7c7066c81cd"),
                ("success", "db60d54f8da1607d"),
                ("error", "f5dc435b22197fb5"),
            ],
        ),
        (
            "Input",
            &[
                ("default", "0b72c606a801eede"),
                ("hover", "140b7f9d74502380"),
                ("active", "d53ad92ce4471a61"),
                ("focus", "a05fc70c5de45cce"),
                ("disabled", "9c4723ddbe476edf"),
                ("loading", "8083a7c7066c81cd"),
                ("success", "db60d54f8da1607d"),
                ("error", "f5dc435b22197fb5"),
            ],
        ),
        (
            "Checkbox",
            &[
                ("default", "4194aea66cf7eddd"),
                ("hover", "7c298024bc1c206d"),
                ("active", "85d14f26ba3be29c"),
                ("focus", "823e40aa1c370d5d"),
                ("disabled", "4276669cc9ecd778"),
                ("loading", "8083a7c7066c81cd"),
                ("success", "db60d54f8da1607d"),
                ("error", "f5dc435b22197fb5"),
            ],
        ),
        (
            "Switch",
            &[
                ("default", "a0c0cd5502cdc81f"),
                ("hover", "9636c2a31a7f6440"),
                ("active", "1c534a1d2b096b31"),
                ("focus", "d85f24b99cac481f"),
                ("disabled", "31f934d719233d5c"),
                ("loading", "8083a7c7066c81cd"),
                ("success", "db60d54f8da1607d"),
                ("error", "f5dc435b22197fb5"),
            ],
        ),
        (
            "Slider",
            &[
                ("default", "c9f2b75293300452"),
                ("hover", "992da0dbf7d30c7d"),
                ("active", "c49be9f7ca5f3b40"),
                ("focus", "d3867896590b2d72"),
                ("disabled", "08cd0ffeb27fc443"),
                ("loading", "95ac97cf905b6111"),
                ("success", "07deacf2bdde62e9"),
                ("error", "08d5354cb469722d"),
            ],
        ),
    ];
    for (kind, rows) in pins {
        assert_eq!(rows.len(), 8, "{kind} must cover all eight states");
        let mut seen = HashSet::new();
        for (state, pin) in *rows {
            let extra = if *state == "default" {
                json!({})
            } else {
                json!({ "state": state })
            };
            let got = digest(kind, extra.clone());
            assert_eq!(got, *pin, "{kind} {state} golden drift");
            assert_eq!(
                got,
                digest(kind, extra),
                "{kind} {state} must be deterministic"
            );
            assert!(seen.insert(got), "{kind} {state} repeats an earlier state");
        }
        assert_eq!(seen.len(), 8, "{kind} eight-state matrix");
    }
}

#[test]
fn explicit_default_and_absent_state_are_byte_identical() {
    assert_eq!(
        render("Button", json!({})),
        render("Button", json!({ "state": "default" }))
    );
    let ir = parse_ir(&ir_for("Button", json!({}))).unwrap();
    assert_eq!(laid("Button", json!({})).state, NodeState::Default);
    assert!(no_new_props(&ir.tree));
}

#[test]
fn disabled_wins_over_authored_and_caller_hover() {
    assert_eq!(
        NodeState::resolve(Some(NodeState::Hover), NodeState::Focus, true),
        NodeState::Disabled
    );
    let disabled_hover = render("Button", json!({ "state": "hover", "disabled": true }));
    let disabled = render("Button", json!({ "state": "disabled" }));
    let hover = render("Button", json!({ "state": "hover" }));
    assert_eq!(disabled_hover, disabled);
    assert_ne!(disabled_hover, hover);
    assert_eq!(
        laid("Button", json!({ "state": "hover", "disabled": true })).state,
        NodeState::Disabled
    );
}

#[test]
fn focus_ring_is_exactly_two_pixels_outside_the_rect() {
    assert_eq!(FOCUS_RING, 2);
    let r = laid("Input", json!({})).rect;
    let default = render("Input", json!({}));
    let focus = render("Input", json!({ "state": "focus" }));
    let accent = [0x09, 0x7f, 0xeb, 255];
    let mid_y = r.y + r.h / 2;
    for dx in [-2, -1, r.w, r.w + 1] {
        assert_eq!(pixel(&focus, r.x + dx, mid_y), accent);
    }
    assert_eq!(
        pixel(&focus, r.x - 3, mid_y),
        pixel(&default, r.x - 3, mid_y)
    );
    assert_eq!(
        pixel(&focus, r.x + r.w + 2, mid_y),
        pixel(&default, r.x + r.w + 2, mid_y)
    );
    let mid_x = r.x + r.w / 2;
    for dy in [-2, -1, r.h, r.h + 1] {
        assert_eq!(pixel(&focus, mid_x, r.y + dy), accent);
    }
    assert_eq!(
        pixel(&focus, mid_x, r.y - 3),
        pixel(&default, mid_x, r.y - 3)
    );
    assert_eq!(
        pixel(&focus, mid_x, r.y + r.h + 2),
        pixel(&default, mid_x, r.y + r.h + 2)
    );
    assert_eq!(pixel(&focus, r.x, mid_y), pixel(&default, r.x, mid_y));
    assert_eq!(
        pixel(&focus, r.x + r.w - 1, mid_y),
        pixel(&default, r.x + r.w - 1, mid_y)
    );
}

#[test]
fn variants_and_tones_are_pairwise_distinct() {
    let mut variants = HashSet::new();
    for name in ["primary", "secondary", "text", "danger", "ghost", "fab"] {
        assert!(
            variants.insert(digest("Button", json!({ "variant": name }))),
            "variant {name} repeats an earlier frame"
        );
    }
    assert_eq!(variants.len(), 6);

    let mut tones = HashSet::new();
    for name in ["success", "warning", "error", "info", "neutral"] {
        assert!(
            tones.insert(digest(
                "Button",
                json!({ "variant": "ghost", "tone": name, "label": "" })
            )),
            "tone {name} repeats an earlier frame"
        );
    }
    assert_eq!(tones.len(), 5);
}

#[test]
fn fab_is_circular_and_tone_border_dot_use_measured_colour() {
    let fab_props = json!({ "variant": "fab", "width": 40, "height": 40, "label": "" });
    let r = laid("Button", fab_props.clone()).rect;
    let fab = render("Button", fab_props);
    assert_eq!(pixel(&fab, r.x, r.y), [255, 255, 255, 255]);
    assert_eq!(
        pixel(&fab, r.x + r.w / 2, r.y + r.h / 2),
        [CYAN.r, CYAN.g, CYAN.b, 255]
    );

    let tone_props = json!({ "variant": "secondary", "tone": "success", "label": "" });
    let r = laid("Button", tone_props.clone()).rect;
    let toned = render("Button", tone_props);
    assert_eq!(
        pixel(&toned, r.x + r.w / 2, r.y),
        [SUCCESS.r, SUCCESS.g, SUCCESS.b, 255]
    );
    assert_eq!(
        pixel(&toned, r.x + r.w - 4, r.y + 4),
        [SUCCESS.r, SUCCESS.g, SUCCESS.b, 255]
    );
}

#[test]
fn disabled_primary_is_desaturated_with_exact_integer_blend() {
    let props = |state: &str| json!({ "variant": "primary", "state": state, "label": "" });
    let r = laid("Button", props("disabled")).rect;
    let disabled = render("Button", props("disabled"));
    let disabled_color = NodeState::Disabled.color(PRIMARY);
    let alpha = (NodeState::Disabled.opacity(UNIT) * 255 / UNIT) as u8;
    let blend = |src: u8, dst: u8| {
        let a = alpha as u32;
        ((src as u32 * a + dst as u32 * (255 - a)) / 255) as u8
    };
    let expected = [
        blend(disabled_color.r, 255),
        blend(disabled_color.g, 255),
        blend(disabled_color.b, 255),
        255,
    ];
    assert_eq!(pixel(&disabled, r.x + r.w / 2, r.y + r.h / 2), expected);
    assert_ne!(
        digest("Button", props("disabled")),
        digest("Button", props("hover"))
    );
    assert_ne!(
        digest("Button", props("disabled")),
        digest("Button", props("active"))
    );
}

#[test]
fn unknown_state_variant_tone_and_bad_types_are_clear_ir_errors() {
    for (prop, value, expected) in [
        ("state", "pressed", "props.state has unknown value"),
        ("variant", "link", "props.variant has unknown value"),
        ("tone", "caution", "props.tone has unknown value"),
    ] {
        let mut props = Map::new();
        props.insert(prop.into(), json!(value));
        let err = parse_ir(&ir_for("Button", Value::Object(props)))
            .unwrap_err()
            .to_string();
        assert!(err.contains(expected), "{err:?}");
        assert!(err.contains("expected"), "{err:?}");
        if prop == "state" {
            for name in ["loading", "success", "error"] {
                assert!(err.contains(name), "error string must list {name}: {err:?}");
            }
        }
    }
    let err = parse_ir(&ir_for("Button", json!({ "state": 1 })))
        .unwrap_err()
        .to_string();
    assert!(err.contains("props.state must be a string"), "{err:?}");
}

#[test]
fn canonical_golden_digests_are_unchanged_without_new_inputs() {
    let ir = parse_ir(COUNTER_IR).unwrap();
    assert!(no_new_props(&ir.tree));
    let mut state = StateView::from_ir(&ir);
    assert_eq!(
        digest_hex(&compat_renderer(320, 240).pixels(&ir, &state)),
        "8d45f412996a99c6"
    );
    assert_eq!(
        digest_hex(&compat_renderer(800, 600).pixels(&ir, &state)),
        "318beb1dd337bc4e"
    );
    state.set("count", StateValue::Int(7));
    assert_eq!(
        digest_hex(&compat_renderer(800, 600).pixels(&ir, &state)),
        "40073cb95c912d80"
    );
}

#[test]
fn cli_flag_overrides_every_interactive_node_and_is_stable_across_runs() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ir_path = dir.join("interaction-matrix.opuir.json");
    fs::write(&ir_path, matrix_ir(None, None)).unwrap();
    let exe = env!("CARGO_BIN_EXE_opurom-render");
    let expected_ir = parse_ir(&matrix_ir(Some("hover"), None)).unwrap();
    let expected =
        digest_hex(&compat_renderer(W, H).pixels(&expected_ir, &StateView::from_ir(&expected_ir)));
    let first_ppm = dir.join("interaction-matrix-1.ppm");
    let second_ppm = dir.join("interaction-matrix-2.ppm");
    let first = stdout(cli(
        exe,
        &ir_path,
        "160",
        "96",
        &first_ppm,
        &["--interactive-state", "hover", "--quality", "compat"],
    ));
    let second = stdout(cli(
        exe,
        &ir_path,
        "160",
        "96",
        &second_ppm,
        &["--interactive-state", "hover", "--quality", "compat"],
    ));
    assert_eq!(first, second);
    assert_eq!(first, format!("digest={expected}\ndirty=1\n"));
    assert_eq!(first.lines().count(), 2, "stdout shape is unchanged");
    assert_eq!(
        fs::read(&first_ppm).unwrap(),
        fs::read(&second_ppm).unwrap()
    );

    let disabled_path = dir.join("interaction-disabled.opuir.json");
    fs::write(&disabled_path, matrix_ir(None, Some(true))).unwrap();
    let disabled_ir = parse_ir(&matrix_ir(Some("disabled"), Some(true))).unwrap();
    let disabled_expected =
        digest_hex(&compat_renderer(W, H).pixels(&disabled_ir, &StateView::from_ir(&disabled_ir)));
    let disabled = stdout(cli(
        exe,
        &disabled_path,
        "160",
        "96",
        &dir.join("interaction-disabled.ppm"),
        &["--interactive-state", "hover", "--quality", "compat"],
    ));
    assert_eq!(disabled, format!("digest={disabled_expected}\ndirty=1\n"));

    let bad = cli(
        exe,
        &ir_path,
        "160",
        "96",
        &dir.join("interaction-bad.ppm"),
        &["--interactive-state", "pressed"],
    );
    assert_eq!(bad.status.code(), Some(1));
    let bad_stderr = String::from_utf8_lossy(&bad.stderr);
    assert!(bad_stderr.contains("unknown interactive state"));
    assert!(
        bad_stderr.contains("loading, success or error"),
        "{bad_stderr}"
    );
    assert!(!bad_stderr.contains("panicked"));

    // The three new business states are accepted by the CLI contract and mask
    // an authored interaction state through NodeState::resolve.
    for state in ["loading", "success", "error"] {
        let authored_hover = dir.join(format!("interaction-auth-hover-{state}.opuir.json"));
        fs::write(&authored_hover, matrix_ir(Some("hover"), None)).unwrap();
        let expected_ir = parse_ir(&matrix_ir(Some(state), None)).unwrap();
        let expected = format!(
            "digest={}\ndirty=1\n",
            digest_hex(
                &compat_renderer(W, H).pixels(&expected_ir, &StateView::from_ir(&expected_ir))
            )
        );
        let out = stdout(cli(
            exe,
            &authored_hover,
            "160",
            "96",
            &dir.join(format!("interaction-{state}.ppm")),
            &["--interactive-state", state],
        ));
        assert_eq!(out, expected, "CLI business state {state}");
    }
}

#[test]
fn cli_state_variables_and_interactive_state_do_not_collide() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ir_path = dir.join("interaction-counter.opuir.json");
    fs::write(&ir_path, COUNTER_IR).unwrap();
    let exe = env!("CARGO_BIN_EXE_opurom-render");
    let no_flag = stdout(cli(
        exe,
        &ir_path,
        "800",
        "600",
        &dir.join("interaction-counter-base.ppm"),
        &["--quality", "compat"],
    ));
    assert_eq!(no_flag, "digest=318beb1dd337bc4e\ndirty=1\n");
    let state_only = stdout(cli(
        exe,
        &ir_path,
        "800",
        "600",
        &dir.join("interaction-counter-state.ppm"),
        &["--state", "count=7", "--quality", "compat"],
    ));
    assert_eq!(state_only, "digest=40073cb95c912d80\ndirty=1\n");
    let combined = stdout(cli(
        exe,
        &ir_path,
        "800",
        "600",
        &dir.join("interaction-counter-combined.ppm"),
        &[
            "--state",
            "count=7",
            "--interactive-state",
            "focus",
            "--quality",
            "compat",
        ],
    ));
    assert_eq!(combined.lines().count(), 2);
    assert_ne!(combined, state_only);
    assert!(combined.starts_with("digest="));
    assert!(combined.ends_with("dirty=1\n"));
}
