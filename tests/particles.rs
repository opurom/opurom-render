//! Deterministic particle fields: opt-in rendering, byte-identical repeats,
//! seed sensitivity, bounded parse diagnostics, clip behaviour and unchanged
//! golden digests.

use opurom_render::{digest_hex, parse_ir, Renderer, StateValue, StateView};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

const BODY: &str = r##"{"ir":"0.1","app":"Particles","state":[],"tree":{"id":0,"kind":"Column","props":{"padding":4},"children":[{"id":1,"kind":"Container","props":{"width":48,"height":32,"color":"#223344","radius":4{EXTRA}},"children":[],"events":{}}],"events":{}},"theme":{"mode":"dark","accent":"#097FEB","radius":4}}"##;

fn ir_with_extra(extra: &str) -> String {
    BODY.replace("{EXTRA}", extra)
}

fn ir_with(particles: Option<&str>) -> String {
    let extra = particles
        .map(|p| format!(",\"particles\":{p}"))
        .unwrap_or_default();
    ir_with_extra(&extra)
}

fn render(ir_text: &str) -> Vec<u8> {
    let ir = parse_ir(ir_text).unwrap();
    Renderer::new(64, 48).pixels(&ir, &StateView::from_ir(&ir))
}

fn commands_json(ir_text: &str) -> Value {
    let ir = parse_ir(ir_text).unwrap();
    let state = StateView::from_ir(&ir);
    serde_json::from_str(
        &Renderer::new(64, 48)
            .display_list(&ir, &state)
            .to_json()
            .unwrap(),
    )
    .unwrap()
}

fn particles_command(doc: &Value) -> Option<&Value> {
    doc["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["kind"].as_str() == Some("particles"))
}

const DEFAULT_PARTICLES: &str = r#"{"count":40,"seed":7}"#;

#[test]
fn particle_frames_and_commands_are_byte_identical_across_renders() {
    let ir_text = ir_with(Some(DEFAULT_PARTICLES));
    let ir = parse_ir(&ir_text).unwrap();
    let state = StateView::from_ir(&ir);
    let first = Renderer::new(64, 48).pixels(&ir, &state);
    let second = Renderer::new(64, 48).pixels(&ir, &state);
    assert_eq!(first, second, "identical IR must render identical bytes");

    let first_json = Renderer::new(64, 48)
        .display_list(&ir, &state)
        .to_json()
        .unwrap();
    let second_json = Renderer::new(64, 48)
        .display_list(&ir, &state)
        .to_json()
        .unwrap();
    assert_eq!(first_json, second_json);

    let doc: Value = serde_json::from_str(&first_json).unwrap();
    let command = particles_command(&doc).expect("particles command must be recorded");
    assert_eq!(command["count"].as_i64(), Some(40));
    assert_eq!(command["seed"].as_i64(), Some(7));
    assert_eq!(command["radius"].as_i64(), Some(2), "documented default");
    assert_eq!(command["color"][3].as_i64(), Some(255), "opaque alpha");
    assert_eq!(command["rect"][2].as_i64(), Some(48), "node width");
    assert_eq!(command["rect"][3].as_i64(), Some(32), "node height");
}

#[test]
fn different_seeds_yield_different_frames() {
    let seed_a = render(&ir_with(Some(r#"{"count":40,"seed":1}"#)));
    let seed_b = render(&ir_with(Some(r#"{"count":40,"seed":2}"#)));
    assert_eq!(seed_a.len(), seed_b.len());
    assert_ne!(seed_a, seed_b, "the seed must select the dot positions");

    let json_a = commands_json(&ir_with(Some(r#"{"count":40,"seed":1}"#)));
    let json_b = commands_json(&ir_with(Some(r#"{"count":40,"seed":2}"#)));
    assert_ne!(
        particles_command(&json_a).unwrap(),
        particles_command(&json_b).unwrap()
    );
}

#[test]
fn count_zero_and_alpha_zero_match_a_node_without_particles() {
    let plain = render(&ir_with(None));
    let plain_json = commands_json(&ir_with(None));
    for prop in [r#"{"count":0,"seed":7}"#, r#"{"count":40,"alpha":0}"#] {
        let ir_text = ir_with(Some(prop));
        assert_eq!(render(&ir_text), plain, "prop {prop} must paint nothing");
        assert_eq!(
            commands_json(&ir_text),
            plain_json,
            "prop {prop} must record no particles command"
        );
    }
    assert!(particles_command(&plain_json).is_none());
}

#[test]
fn particle_pixels_stay_inside_the_recorded_rect() {
    let ir_text = ir_with(Some(r#"{"count":60,"seed":11,"radius":3}"#));
    let plain = render(&ir_with(None));
    let scattered = render(&ir_text);
    let doc = commands_json(&ir_text);
    let command = particles_command(&doc).unwrap();
    let rect: Vec<i64> = (0..4usize)
        .map(|k| command["rect"][k].as_i64().unwrap())
        .collect();
    for y in 0..48i64 {
        for x in 0..64i64 {
            let i = (y as usize * 64 + x as usize) * 4;
            if plain[i..i + 4] != scattered[i..i + 4] {
                assert!(
                    x >= rect[0] && y >= rect[1] && x < rect[0] + rect[2] && y < rect[1] + rect[3],
                    "particle pixel ({x},{y}) leaked outside the rect {rect:?}"
                );
            }
        }
    }
    assert_ne!(plain, scattered, "the field must be visible");
}

#[test]
fn particles_draw_between_the_effect_backdrop_and_mask_snapshots() {
    let with_effects = ir_with_extra(
        r##","particles":{"count":24,"seed":3},"effects":[{"kind":"glow","radius":2,"alpha":512}]"##,
    );
    let doc = commands_json(&with_effects);
    let kinds: Vec<&str> = doc["commands"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["kind"].as_str())
        .collect();
    let backdrop = kinds.iter().position(|k| *k == "effect_backdrop").unwrap();
    let particles = kinds.iter().position(|k| *k == "particles").unwrap();
    let mask = kinds.iter().position(|k| *k == "effect_mask").unwrap();
    let glow = kinds.iter().position(|k| *k == "effect_glow").unwrap();
    assert!(
        backdrop < particles && particles < mask && mask < glow,
        "unexpected draw order: {kinds:?}"
    );
    assert_eq!(render(&with_effects), render(&with_effects));
    assert_ne!(render(&with_effects), render(&ir_with(None)));
}

#[test]
fn malformed_particle_props_are_rejected_with_clear_diagnostics() {
    let cases: &[(&str, &str)] = &[
        (r#"{"seed":1}"#, "missing required field"),
        (r#"{"count":513}"#, "count 513 must be 0..=512"),
        (r#"{"count":-1}"#, "count -1 must be 0..=512"),
        (r#"{"count":"many"}"#, "must be an integer"),
        (r#"{"count":4,"seed":-1}"#, "seed -1 must be 0..=4294967295"),
        (
            r#"{"count":4,"seed":4294967296}"#,
            "seed 4294967296 must be 0..=4294967295",
        ),
        (r#"{"count":4,"radius":33}"#, "radius 33 must be 0..=32"),
        (r#"{"count":4,"radius":-2}"#, "radius -2 must be 0..=32"),
        (r#"{"count":4,"alpha":1025}"#, "alpha 1025 must be 0..=1024"),
        (r#"{"count":4,"alpha":1.5}"#, "must be an integer"),
        (r##"{"count":4,"color":"#GGGGGG"}"##, "must be #RRGGBB"),
        ("7", "must be a JSON object"),
    ];
    for (prop, expected) in cases {
        let error = parse_ir(&ir_with(Some(prop))).expect_err(prop);
        assert!(
            error.to_string().contains(expected),
            "prop {prop}: {} must mention {expected:?}",
            error
        );
    }
    // Boundary values parse and render.
    render(&ir_with(Some(r#"{"count":0,"seed":0}"#)));
    render(&ir_with(Some(
        r#"{"count":512,"seed":4294967295,"radius":32,"alpha":1024}"#,
    )));
}

#[test]
fn golden_counter_digests_are_unchanged_by_the_particles_layer() {
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

    // The CLI surface is unchanged as well.
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ppm = dir.join("particles-golden.ppm");
    let output = ProcessCommand::new(env!("CARGO_BIN_EXE_opurom-render"))
        .arg("../opurom-spec/counter.opuir.json")
        .args(["800", "600"])
        .arg(&ppm)
        .args(["--state", "count=7"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "digest=40073cb95c912d80\ndirty=1\n"
    );
    let ppm_bytes = fs::read(&ppm).unwrap();
    assert!(ppm_bytes.starts_with(b"P6\n800 600\n255\n"));
    assert_eq!(ppm_bytes.len(), 800 * 600 * 3 + 15);
}
