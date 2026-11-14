//! Software effects: prop schema, determinism, distinct pixels, clamping,
//! effect-command export and V0.1 golden regressions.

use opurom_render::{
    digest_hex, layout, parse_ir, Renderer, Rgba, StateValue, StateView, VisualEffectKind,
    EFFECT_MAX_OFFSET, EFFECT_MAX_RADIUS, EFFECT_UNIT,
};
use std::{fs, path::PathBuf, process::Command as ProcessCommand};

const BASE_COMMANDS: &[u8] = include_bytes!("fixtures/counter-commands.json");
const BASE_COMMANDS_FNV: &str = "c72422e40ae53297";

const IR_HEAD: &str = r##"{
  "ir": "0.1", "app": "EffectsProof",
  "state": [ { "name": "n", "type": "int", "init": 3 } ],
  "tree": { "id": 0, "kind": "Column",
    "props": { "gap": 6, "padding": 6, "align": "center" },
    "children": [
      { "id": 1, "kind": "Container",
        "props": { "width": 60, "height": 12, "radius": 2, "color": "#E5464F" },
        "children": [], "events": {} },
      { "id": 2, "kind": "Container",
        "props": { "width": 64, "height": 32, "radius": 6, "color": "#112233""##;

const IR_TAIL: &str = r##" },
        "children": [
          { "id": 3, "kind": "Text",
            "props": { "value": "R{n}", "size": 14, "color": "#FFFFFF" },
            "children": [], "events": {} }
        ], "events": {} }
    ], "events": {} },
  "theme": { "mode": "dark", "accent": "#097FEB", "radius": 4 }
}"##;

const BLUR: &str = r#"[{"kind":"blur","radius":3}]"#;
const SHADOW: &str =
    r##"[{"kind":"shadow","radius":2,"dx":7,"dy":5,"alpha":768,"color":"#000000"}]"##;
const GLOW: &str = r##"[{"kind":"glow","radius":6,"alpha":768,"color":"#FFCC00"}]"##;
const GLASS: &str = r##"[{"kind":"glass","radius":4,"alpha":384,"color":"#FFFFFF"}]"##;
const LIQUID: &str = r##"[{"kind":"liquid","alpha":768,"color":"#FFFFFF"}]"##;
const KINDS: [(&str, &str); 5] = [
    ("blur", BLUR),
    ("shadow", SHADOW),
    ("glow", GLOW),
    ("glass", GLASS),
    ("liquid", LIQUID),
];

fn ir_with_extra(extra: &str) -> String {
    let comma = if extra.trim().is_empty() {
        String::new()
    } else {
        format!(", {extra}")
    };
    format!("{IR_HEAD}{comma}{IR_TAIL}")
}

fn ir_of(effects: Option<&str>) -> String {
    match effects {
        Some(list) => ir_with_extra(&format!("\"effects\":{list}")),
        None => ir_with_extra(""),
    }
}

fn render_pixels(ir_text: &str, count: i64, width: u32, height: u32) -> Vec<u8> {
    let ir = parse_ir(ir_text).expect("effect fixture IR must parse");
    let mut state = StateView::from_ir(&ir);
    state.set("n", StateValue::Int(count));
    Renderer::new(width, height).pixels(&ir, &state)
}

fn effect_pixels(effects: &str) -> Vec<u8> {
    render_pixels(&ir_of(Some(effects)), 3, 96, 64)
}

fn effect_digest(effects: &str) -> String {
    digest_hex(&effect_pixels(effects))
}

fn assert_effect_deterministic(name: &str, effects: &str) {
    let ir = ir_of(Some(effects));
    let first = render_pixels(&ir, 3, 96, 64);
    let second = render_pixels(&ir, 3, 96, 64);
    assert_eq!(first, second, "{name} must be byte-identical");
    assert_ne!(
        first,
        render_pixels(&ir_of(None), 3, 96, 64),
        "{name} must change the fallback pixels"
    );
}

#[test]
fn blur_is_deterministic() {
    assert_effect_deterministic("blur", BLUR);
}

#[test]
fn shadow_is_deterministic() {
    assert_effect_deterministic("shadow", SHADOW);
}

#[test]
fn glow_is_deterministic() {
    assert_effect_deterministic("glow", GLOW);
}

#[test]
fn glass_is_deterministic() {
    assert_effect_deterministic("glass", GLASS);
}

#[test]
fn liquid_is_deterministic() {
    assert_effect_deterministic("liquid", LIQUID);
}

#[test]
fn five_effects_are_pairwise_distinct_and_differ_from_fallback() {
    let mut samples = vec![(
        "fallback",
        digest_hex(&render_pixels(&ir_of(None), 3, 96, 64)),
    )];
    for (name, effects) in KINDS {
        samples.push((name, effect_digest(effects)));
    }
    for (i, (a, da)) in samples.iter().enumerate() {
        println!("sample {a}={da}");
        for (b, db) in samples.iter().skip(i + 1) {
            assert_ne!(da, db, "{a} and {b} must differ");
        }
    }
}

#[test]
fn sample_effect_digests_are_pinned() {
    for ((name, effects), expected) in KINDS.iter().zip([
        "f49acb8b8d376301",
        "e1737d9e15ca7360",
        "accf7da0beacb2c4",
        "36366364419d42ae",
        "776c9463bc3f33f5",
    ]) {
        assert_eq!(effect_digest(effects), expected, "{name} sample changed");
    }
}

#[test]
fn identical_inputs_follow_state_theme_and_viewport() {
    for (name, effects) in KINDS {
        let ir = ir_of(Some(effects));
        assert_eq!(
            render_pixels(&ir, 7, 96, 64),
            render_pixels(&ir, 7, 96, 64),
            "{name} is not repeatable with the same state and theme"
        );
    }
    let ir = ir_of(Some(GLOW));
    let base = render_pixels(&ir, 3, 96, 64);
    assert_ne!(base, render_pixels(&ir, 7, 96, 64));
    let light = ir.replace("\"mode\": \"dark\"", "\"mode\": \"light\"");
    assert_ne!(base, render_pixels(&light, 3, 96, 64));
    assert_ne!(base, render_pixels(&ir, 3, 128, 80));
}

#[test]
fn effect_schema_is_typed_and_carried_into_layout() {
    let effects = r##"[
      {"kind":"shadow","radius":5,"dx":-9,"dy":11,"alpha":900,"color":"#123456","future":1},
      {"kind":"liquid"}
    ]"##;
    let ir = parse_ir(&ir_of(Some(effects))).unwrap();
    assert!(ir.has_visual_effects());
    assert!(!parse_ir(&ir_of(None)).unwrap().has_visual_effects());
    let fx = &ir.tree.children[1].props.effects;
    assert_eq!(fx.len(), 2);
    assert_eq!(fx[0].kind, VisualEffectKind::Shadow);
    assert_eq!(fx[0].radius, 5);
    assert_eq!(fx[0].dx, -9);
    assert_eq!(fx[0].dy, 11);
    assert_eq!(fx[0].alpha, 900);
    assert_eq!(fx[0].color, Rgba::from_hex("#123456"));
    assert_eq!(fx[1].kind, VisualEffectKind::Liquid);
    assert_eq!(fx[1].radius, 3);
    assert_eq!(fx[1].dx, 1);
    assert_eq!(fx[1].dy, 1);
    assert_eq!(fx[1].alpha, EFFECT_UNIT / 2);
    let laid = layout(&ir, &StateView::from_ir(&ir), 96, 64);
    let node = laid.iter().find(|n| n.id == 2).unwrap();
    assert_eq!(node.visual_effects, *fx);
}

#[test]
fn malformed_effect_props_are_rejected() {
    for extra in [
        "\"effects\": {}",
        "\"effects\": [{\"kind\":\"warp\"}]",
        "\"effects\": [{\"kind\":\"blur\",\"radius\":\"4\"}]",
        "\"effects\": [{\"kind\":\"glow\",\"color\":\"not-a-color\"}]",
        "\"effects\": [{\"radius\":2}]",
    ] {
        let err = parse_ir(&ir_with_extra(extra))
            .expect_err("a malformed effects prop must be rejected")
            .to_string();
        assert!(
            err.contains("effects") || err.contains("kind"),
            "unexpected diagnostic: {err}"
        );
    }
}

#[test]
fn radius_is_clamped_for_all_effects() {
    let large = r#"[
      {"kind":"blur","radius":999999},
      {"kind":"shadow","radius":999999,"dx":6,"dy":6,"alpha":768},
      {"kind":"glow","radius":999999,"alpha":768},
      {"kind":"glass","radius":999999,"alpha":384},
      {"kind":"liquid","radius":999999,"alpha":768}
    ]"#;
    let capped = large.replace("999999", &EFFECT_MAX_RADIUS.to_string());
    assert_eq!(
        effect_pixels(large),
        effect_pixels(&capped),
        "large radii must clamp to the documented maximum"
    );
    let negative = large.replace("999999", "-999999");
    let zero = large.replace("999999", "0");
    assert_eq!(
        effect_pixels(&negative),
        effect_pixels(&zero),
        "negative radii must clamp to zero"
    );
}

#[test]
fn extreme_effect_integers_clamp_without_overflow() {
    let max = i64::MAX.to_string();
    let min = i64::MIN.to_string();
    let extreme = format!(
        r#"[
          {{"kind":"blur","radius":{max}}},
          {{"kind":"shadow","radius":{max},"dx":{max},"dy":{min},"alpha":{max}}},
          {{"kind":"glow","radius":{max},"alpha":{max}}},
          {{"kind":"glass","radius":{max},"alpha":{max}}},
          {{"kind":"liquid","radius":{max},"alpha":{max}}}
        ]"#
    );
    let capped = format!(
        r#"[
          {{"kind":"blur","radius":{EFFECT_MAX_RADIUS}}},
          {{"kind":"shadow","radius":{EFFECT_MAX_RADIUS},"dx":{EFFECT_MAX_OFFSET},"dy":-{EFFECT_MAX_OFFSET},"alpha":{EFFECT_UNIT}}},
          {{"kind":"glow","radius":{EFFECT_MAX_RADIUS},"alpha":{EFFECT_UNIT}}},
          {{"kind":"glass","radius":{EFFECT_MAX_RADIUS},"alpha":{EFFECT_UNIT}}},
          {{"kind":"liquid","radius":{EFFECT_MAX_RADIUS},"alpha":{EFFECT_UNIT}}}
        ]"#
    );
    assert_eq!(effect_pixels(&extreme), effect_pixels(&capped));

    let minimum = format!(
        r#"[
          {{"kind":"blur","radius":{min}}},
          {{"kind":"shadow","radius":{min},"dx":{min},"dy":{min},"alpha":{min}}},
          {{"kind":"glow","radius":{min},"alpha":{min}}},
          {{"kind":"glass","radius":{min},"alpha":{min}}},
          {{"kind":"liquid","radius":{min},"alpha":{min}}}
        ]"#
    );
    let lower = r#"[
      {"kind":"blur","radius":0},
      {"kind":"shadow","radius":0,"dx":-64,"dy":-64,"alpha":0},
      {"kind":"glow","radius":0,"alpha":0},
      {"kind":"glass","radius":0,"alpha":0},
      {"kind":"liquid","radius":0,"alpha":0}
    ]"#;
    assert_eq!(effect_pixels(&minimum), effect_pixels(lower));

    let ir = parse_ir(&ir_of(Some(&extreme))).unwrap();
    let fx = &ir.tree.children[1].props.effects;
    assert_eq!(fx[1].radius, EFFECT_MAX_RADIUS);
    assert_eq!(fx[1].dx, EFFECT_MAX_OFFSET);
    assert_eq!(fx[1].dy, -EFFECT_MAX_OFFSET);
    assert_eq!(fx[1].alpha, EFFECT_UNIT);

    let tiny = render_pixels(&ir_of(Some(&extreme)), 3, 16, 12);
    assert_eq!(tiny.len(), 16 * 12 * 4);
    assert!(tiny.chunks_exact(4).all(|p| p[3] == 255));
}

const CLIP_HEAD: &str = r##"{
  "ir": "0.1", "app": "ClipProof", "state": [],
  "tree": { "id": 0, "kind": "Column", "props": { "gap": 0, "padding": 0 },
    "children": [
      { "id": 1, "kind": "Scroll", "props": { "width": 80, "height": 20 },
        "children": [
          { "id": 2, "kind": "Container",
            "props": { "width": 60, "height": 20, "color": "#112233""##;

const CLIP_TAIL: &str = r##" }, "children": [], "events": {} }
        ], "events": {} },
      { "id": 3, "kind": "Container",
        "props": { "width": 80, "height": 20, "color": "#445566" },
        "children": [], "events": {} }
    ], "events": {} },
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 0 }
}"##;

fn clip_ir(effects: Option<&str>) -> String {
    let extra = effects
        .map(|list| format!(", \"effects\":{list}"))
        .unwrap_or_default();
    format!("{CLIP_HEAD}{extra}{CLIP_TAIL}")
}

#[test]
fn effect_bounds_are_clipped_to_viewport_and_parent_clip() {
    let effects = r##"[
      {"kind":"shadow","radius":10,"dx":64,"dy":64,"alpha":900},
      {"kind":"glow","radius":10,"alpha":900,"color":"#FFCC00"}
    ]"##;
    let fallback_ir = parse_ir(&clip_ir(None)).unwrap();
    let effect_ir = parse_ir(&clip_ir(Some(effects))).unwrap();
    let state = StateView::from_ir(&fallback_ir);
    let fallback = Renderer::new(80, 60).pixels(&fallback_ir, &state);
    let changed = Renderer::new(80, 60).pixels(&effect_ir, &state);
    let row_bytes = 80usize * 4;
    assert_ne!(
        &fallback[..row_bytes * 20],
        &changed[..row_bytes * 20],
        "the effect must be visible inside its parent clip"
    );
    for y in 20..60 {
        let a = &fallback[y * row_bytes..(y + 1) * row_bytes];
        let b = &changed[y * row_bytes..(y + 1) * row_bytes];
        assert_eq!(a, b, "effect leaked outside parent clip at row {y}");
    }
}

#[test]
fn no_effect_goldens_and_commands_are_byte_identical() {
    let counter = fs::read_to_string("../opurom-spec/counter.opuir.json").unwrap();
    let ir = parse_ir(&counter).unwrap();
    let state = StateView::from_ir(&ir);
    assert_eq!(
        digest_hex(&Renderer::new(320, 240).render(&ir, &state).pixels),
        "8d45f412996a99c6"
    );
    assert_eq!(
        digest_hex(&Renderer::new(800, 600).render(&ir, &state).pixels),
        "318beb1dd337bc4e"
    );
    let mut count_7 = state.clone();
    count_7.set("count", StateValue::Int(7));
    assert_eq!(
        digest_hex(&Renderer::new(800, 600).render(&ir, &count_7).pixels),
        "40073cb95c912d80"
    );
    assert!(!ir.has_visual_effects());

    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ppm = dir.join("r8-golden.ppm");
    let commands = dir.join("r8-golden.commands.json");
    let output = ProcessCommand::new(env!("CARGO_BIN_EXE_opurom-render"))
        .arg("../opurom-spec/counter.opuir.json")
        .args(["320", "240"])
        .arg(&ppm)
        .args(["--commands", commands.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "digest=8d45f412996a99c6\ndirty=1\n"
    );
    let bytes = fs::read(&commands).unwrap();
    assert_eq!(bytes.as_slice(), BASE_COMMANDS);
    assert_eq!(digest_hex(&bytes), BASE_COMMANDS_FNV);
}

#[test]
fn cli_commands_accepts_visual_effects() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ir_path = dir.join("r8-command-effect.opuir.json");
    let ppm = dir.join("r8-command-effect.ppm");
    let commands = dir.join("r8-command-effect.commands.json");
    fs::write(&ir_path, ir_of(Some(BLUR))).unwrap();
    let _ = fs::remove_file(&commands);

    let accepted = ProcessCommand::new(env!("CARGO_BIN_EXE_opurom-render"))
        .arg(&ir_path)
        .args(["96", "64"])
        .arg(&ppm)
        .args(["--commands", commands.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    assert!(String::from_utf8_lossy(&accepted.stdout).starts_with("digest="));
    let json = fs::read_to_string(&commands).unwrap();
    assert!(
        json.contains("\"kind\":\"effect_blur\",\"radius\":3"),
        "blur command missing: {json}"
    );
}
