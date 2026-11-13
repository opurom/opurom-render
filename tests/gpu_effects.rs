//! GPU effect-command export: all five effect types, integer parameters, draw
//! order, bounds, effect-free invariance and the committed proof fixture.

use opurom_render::{digest_hex, parse_ir, DisplayList, Renderer, Rgba, StateView};
use serde_json::Value;
use std::{fs, path::PathBuf, process::Command};

const FIXTURE_IR: &str = include_str!("fixtures/r9_effect.opuir.json");
const BASE_COMMANDS: &[u8] = include_bytes!("fixtures/r6_counter_commands.json");

fn ir_with(effects: &str) -> String {
    format!(
        r##"{{"ir":"0.1","app":"CommandsProof","state":[],"tree":{{"id":0,"kind":"Column","props":{{"gap":6,"padding":6}},"children":[{{"id":1,"kind":"Container","props":{{"width":64,"height":32,"radius":6,"color":"#112233","effects":{effects}}},"children":[],"events":{{}}}}],"events":{{}}}},"theme":{{"mode":"dark","accent":"#097FEB","radius":4}}}}"##
    )
}

fn dump(ir_path: &PathBuf, width: &str, height: &str, label: &str) -> (String, PathBuf) {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ppm = dir.join(format!("r9-{label}.ppm"));
    let commands = dir.join(format!("r9-{label}.commands.json"));
    let out = Command::new(env!("CARGO_BIN_EXE_opurom-render"))
        .arg(ir_path)
        .args([width, height])
        .arg(&ppm)
        .args(["--commands"])
        .arg(&commands)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{label}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).lines().count(),
        2,
        "{label}: --commands must not add stdout lines"
    );
    (fs::read_to_string(&commands).unwrap(), ppm)
}

const ALL_EFFECTS: &str = r##"[
  {"kind":"blur","radius":2},
  {"kind":"shadow","radius":2,"dx":5,"dy":-4,"alpha":768,"color":"#123456"},
  {"kind":"glow","radius":3,"alpha":640,"color":"#FFCC00"},
  {"kind":"glass","radius":4,"alpha":384,"color":"#FFFFFF"},
  {"kind":"liquid","alpha":700,"color":"#00FF00"}
]"##;

#[test]
fn all_five_effect_command_types_carry_integer_parameters() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ir_path = dir.join("r9-all.opuir.json");
    fs::write(&ir_path, ir_with(ALL_EFFECTS)).unwrap();
    let (json, _) = dump(&ir_path, "96", "64", "all");
    let doc: Value = serde_json::from_str(&json).unwrap();
    let commands = doc["commands"].as_array().unwrap();
    let by_kind = |kind: &str| -> Vec<&Value> {
        commands
            .iter()
            .filter(|c| c["kind"].as_str() == Some(kind))
            .collect()
    };
    for kind in [
        "effect_backdrop",
        "effect_mask",
        "effect_shadow",
        "effect_glow",
        "effect_glass",
        "effect_liquid",
        "effect_blur",
    ] {
        assert_eq!(by_kind(kind).len(), 1, "{kind} must appear exactly once");
    }
    let shadow = by_kind("effect_shadow")[0];
    assert_eq!(shadow["radius"].as_i64(), Some(2));
    assert_eq!(shadow["dx"].as_i64(), Some(5));
    assert_eq!(shadow["dy"].as_i64(), Some(-4));
    assert_eq!(shadow["alpha"].as_i64(), Some(768));
    assert_eq!(shadow["color"][0].as_i64(), Some(18));
    assert_eq!(shadow["color"][3].as_i64(), Some(255));
    let glow = by_kind("effect_glow")[0];
    assert_eq!(glow["radius"].as_i64(), Some(3));
    let glass = by_kind("effect_glass")[0];
    assert_eq!(glass["radius"].as_i64(), Some(4));
    assert_eq!(glass["shape_radius"].as_i64(), Some(6));
    let liquid = by_kind("effect_liquid")[0];
    assert_eq!(liquid["denom"].as_i64(), Some(94));
    let blur = by_kind("effect_blur")[0];
    assert_eq!(blur["radius"].as_i64(), Some(2));
    let mask = by_kind("effect_mask")[0];
    assert_eq!(mask["shape"].as_i64(), Some(1));
    let bounds_w = mask["bounds"][2].as_i64().unwrap();
    let bounds_h = mask["bounds"][3].as_i64().unwrap();
    assert!((64..=96).contains(&bounds_w), "bounds must pad the node");
    assert!((32..=64).contains(&bounds_h), "bounds must pad the node");
}

#[test]
fn effect_commands_follow_the_software_draw_order() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ir_path = dir.join("r9-order.opuir.json");
    fs::write(&ir_path, ir_with(ALL_EFFECTS)).unwrap();
    let (json, _) = dump(&ir_path, "96", "64", "order");
    let doc: Value = serde_json::from_str(&json).unwrap();
    let kinds: Vec<&str> = doc["commands"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["kind"].as_str())
        .filter(|k| k.starts_with("effect_") || *k == "solid" || *k == "rounded")
        .collect();
    let backdrop = kinds.iter().position(|k| *k == "effect_backdrop").unwrap();
    let mask = kinds.iter().position(|k| *k == "effect_mask").unwrap();
    let blur = kinds.iter().position(|k| *k == "effect_blur").unwrap();
    assert!(
        kinds[backdrop + 1..mask]
            .iter()
            .any(|k| *k == "solid" || *k == "rounded"),
        "the node's own draw commands must sit between the two snapshots"
    );
    assert!(mask < blur, "local effects must precede the frame blur");
    assert_eq!(kinds.last(), Some(&"effect_blur"));
    let shadow = kinds.iter().position(|k| *k == "effect_shadow").unwrap();
    let glow = kinds.iter().position(|k| *k == "effect_glow").unwrap();
    let glass = kinds.iter().position(|k| *k == "effect_glass").unwrap();
    let liquid = kinds.iter().position(|k| *k == "effect_liquid").unwrap();
    assert!(shadow < glow && glow < glass && glass < liquid);
}

#[test]
fn effect_free_stream_is_byte_identical_to_r6_and_unchanged_by_an_empty_array() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let plain = dir.join("r9-plain.opuir.json");
    let empty = dir.join("r9-empty.opuir.json");
    let without = ir_with("[]").replace(",\"effects\":[]", "");
    fs::write(&plain, &without).unwrap();
    fs::write(&empty, ir_with("[]")).unwrap();
    let (a, _) = dump(&plain, "96", "64", "plain");
    let (b, _) = dump(&empty, "96", "64", "empty");
    assert_eq!(a, b);
    assert!(!a.contains("effect_"));

    let counter = fs::read_to_string("../opurom-spec/counter.opuir.json").unwrap();
    let ir = parse_ir(&counter).unwrap();
    let state = StateView::from_ir(&ir);
    assert_eq!(
        Renderer::new(320, 240)
            .display_list(&ir, &state)
            .to_json()
            .unwrap()
            .as_bytes(),
        BASE_COMMANDS
    );
}

#[test]
fn commands_do_not_change_the_effect_ppm_or_stdout() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ir_path = dir.join("r9-effect-ppm.opuir.json");
    fs::write(&ir_path, ir_with(ALL_EFFECTS)).unwrap();
    let exe = env!("CARGO_BIN_EXE_opurom-render");
    // Unique names: dump(..., "plain") writes r9-plain.ppm from a different IR,
    // and parallel tests would otherwise race on the shared temp directory.
    let plain_ppm = dir.join("r9-effect-plain.ppm");
    let command_ppm = dir.join("r9-effect-command.ppm");
    let commands = dir.join("r9-effect-ppm.commands.json");
    let plain = Command::new(exe)
        .arg(&ir_path)
        .args(["96", "64"])
        .arg(&plain_ppm)
        .output()
        .unwrap();
    let with = Command::new(exe)
        .arg(&ir_path)
        .args(["96", "64"])
        .arg(&command_ppm)
        .args(["--commands"])
        .arg(&commands)
        .output()
        .unwrap();
    assert!(plain.status.success() && with.status.success());
    assert_eq!(plain.stdout, with.stdout);
    assert_eq!(
        fs::read(&plain_ppm).unwrap(),
        fs::read(&command_ppm).unwrap()
    );
}

#[test]
fn effect_bounds_are_clipped_to_the_viewport() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ir_path = dir.join("r9-bounds.opuir.json");
    fs::write(
        &ir_path,
        ir_with(r#"[{"kind":"shadow","radius":32,"dx":64,"dy":64,"alpha":1024}]"#),
    )
    .unwrap();
    let (json, _) = dump(&ir_path, "32", "24", "bounds");
    let doc: Value = serde_json::from_str(&json).unwrap();
    let shadow = doc["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["kind"].as_str() == Some("effect_shadow"))
        .unwrap();
    let b = &shadow["bounds"];
    assert_eq!(b[0].as_i64(), Some(0));
    assert_eq!(b[1].as_i64(), Some(0));
    assert!(b[2].as_i64().unwrap() <= 32);
    assert!(b[3].as_i64().unwrap() <= 24);
}

#[test]
fn committed_fixture_emits_all_five_effects_and_is_deterministic() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    assert!(parse_ir(FIXTURE_IR).unwrap().has_visual_effects());
    let ir_path = dir.join("r9-fixture.opuir.json");
    fs::write(&ir_path, FIXTURE_IR).unwrap();
    let (first, ppm) = dump(&ir_path, "96", "64", "fixture-a");
    let (second, _) = dump(&ir_path, "96", "64", "fixture-b");
    assert_eq!(first, second);
    for kind in [
        "effect_blur",
        "effect_shadow",
        "effect_glow",
        "effect_glass",
        "effect_liquid",
    ] {
        assert!(
            first.contains(&format!("\"kind\":\"{kind}\"")),
            "{kind} missing"
        );
    }
    assert_eq!(digest_hex(&fs::read(&ppm).unwrap()).len(), 16);
}

#[test]
fn empty_display_list_and_malformed_effect_commands_are_bounded() {
    let empty = DisplayList {
        width: 2,
        height: 1,
        background: Rgba::rgb(1, 2, 3),
        commands: Vec::new(),
    };
    let json = empty.to_json().unwrap();
    assert!(json.contains("\"commands\":[\n]}\n"));

    // The GPU parser rejects these; the renderer never emits them.
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ir_path = dir.join("r9-malformed.opuir.json");
    fs::write(&ir_path, ir_with(r#"[{"kind":"warp"}]"#)).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_opurom-render"))
        .arg(&ir_path)
        .args(["96", "64"])
        .arg(dir.join("r9-malformed.ppm"))
        .args(["--commands"])
        .arg(dir.join("r9-malformed.commands.json"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("effects"));
}
