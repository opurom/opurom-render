//! Per-node M3 material elevation (ELEVATION_PER_NODE_PROPOSAL.md): the
//! embedded table matches material-tokens.json, authored levels pass a strict
//! six-level whitelist, state deltas use the separate M projector, and the
//! default (no elevation) path stays byte-identical.

use opurom_render::{
    digest_hex, elevation_level, parse_ir, project_state_level, Command, Renderer, StateView,
    ELEVATION_TABLE,
};
use serde_json::Value;

// CI checks out render without the sibling UI repo, so the fixture is the
// source of truth here; a local sibling is checked for drift only.
const TOKENS: &str = "tests/fixtures/material-tokens.json";
const SIBLING_TOKENS: &str = "../opurom-ui/theme/material-tokens.json";
const COUNTER: &str = "../opurom-examples/golden/counter.opuir.json";

fn alpha_of(hex: &str) -> u8 {
    u8::from_str_radix(&hex[1..3], 16).unwrap()
}

#[test]
fn embedded_table_matches_material_tokens_json() {
    let fixture = std::fs::read_to_string(TOKENS).unwrap();
    if let Ok(sibling) = std::fs::read_to_string(SIBLING_TOKENS) {
        assert_eq!(sibling, fixture, "{SIBLING_TOKENS} drifted from fixture");
    }
    let v: Value = serde_json::from_str(&fixture).unwrap();
    let elev = &v["tokens"]["elevation"];
    for level in 0..=6 {
        let Some(layers) = ELEVATION_TABLE[level] else {
            assert!(
                elev.get(format!("md-elev-{level}")).is_none() || level == 0,
                "level {level} absent in table but present in JSON"
            );
            continue;
        };
        let key = format!("md-elev-{level}");
        let node = &elev[&key];
        for (layer, name) in layers.iter().zip(["ambient", "penumbra", "umbra"]) {
            let j = &node[name];
            let off = j["offset"].as_array().unwrap();
            let (dx, dy, blur, spread, alpha) = *layer;
            assert_eq!(dx, off[0].as_i64().unwrap(), "{key}.{name}.offset.x");
            assert_eq!(dy, off[1].as_i64().unwrap(), "{key}.{name}.offset.y");
            assert_eq!(blur, j["blur"].as_i64().unwrap(), "{key}.{name}.blur");
            assert_eq!(spread, j["spread"].as_i64().unwrap(), "{key}.{name}.spread");
            assert_eq!(
                alpha,
                alpha_of(j["color"].as_str().unwrap()),
                "{key}.{name}.color alpha"
            );
        }
    }
    assert!(ELEVATION_TABLE[5].is_none());
    assert!(ELEVATION_TABLE[0].is_none());
}

#[test]
fn legacy_shadow_canonicalizer_is_unchanged() {
    assert_eq!(elevation_level(0), 0);
    assert_eq!(elevation_level(1), 1);
    assert_eq!(elevation_level(4), 4);
    assert_eq!(elevation_level(5), 4);
    assert_eq!(elevation_level(6), 6);
    assert_eq!(elevation_level(7), 6);
    assert_eq!(elevation_level(8), 6);
}

#[test]
fn state_projector_uses_six_level_table() {
    // M(x) = largest legal level <= clamp(round_half_up(x), 0, 6).
    assert_eq!(project_state_level(0), 0);
    assert_eq!(project_state_level(1), 1);
    assert_eq!(project_state_level(3), 3);
    assert_eq!(project_state_level(4), 4);
    assert_eq!(project_state_level(5), 4); // 3+2=5 -> 4
    assert_eq!(project_state_level(6), 6);
    assert_eq!(project_state_level(7), 6); // 1+6=7 -> 6
    assert_eq!(project_state_level(10), 6);
    assert_eq!(project_state_level(-3), 0); // disabled clamp
}

fn box_ir(elevation: Option<i64>) -> String {
    let extra = elevation
        .map(|e| format!(",\"elevation\":{e}"))
        .unwrap_or_default();
    format!(
        r##"{{
        "ir": "0.1.0", "app": "Elev", "state": [],
        "tree": {{ "id": 0, "kind": "Column", "props": {{ "padding": 8 }},
            "children": [ {{ "id": 1, "kind": "Container", "props": {{
                "color": "#102030", "width": 60, "height": 40, "radius": 4{extra} }},
                "children": [], "events": {{}} }} ], "events": {{}} }},
        "theme": {{ "mode": "light", "accent": "#097FEB", "radius": 4 }}
    }}"##
    )
}

fn render(ir_text: &str) -> Vec<u8> {
    let ir = parse_ir(ir_text).unwrap();
    let state = StateView::from_ir(&ir);
    Renderer::new(240, 200).render(&ir, &state).pixels
}

#[test]
fn authored_whitelist_rejects_illegal_levels() {
    for bad in [5, 7, 8, -1, 9] {
        assert!(
            parse_ir(&box_ir(Some(bad))).is_err(),
            "elevation {bad} must fail loud"
        );
    }
    for good in [0, 1, 2, 3, 4, 6] {
        assert!(
            parse_ir(&box_ir(Some(good))).is_ok(),
            "elevation {good} must be accepted"
        );
    }
}

#[test]
fn per_node_elevation_draws_a_material_shadow() {
    let none = render(&box_ir(None));
    let elev = render(&box_ir(Some(3)));
    assert_ne!(
        digest_hex(&none),
        digest_hex(&elev),
        "elevation 3 draws a different shadow than no elevation"
    );
    let l1 = render(&box_ir(Some(1)));
    let l6 = render(&box_ir(Some(6)));
    assert_ne!(digest_hex(&l1), digest_hex(&l6), "levels differ");
    assert_eq!(
        digest_hex(&l1),
        digest_hex(&render(&box_ir(Some(1)))),
        "deterministic"
    );
}

#[test]
fn elevation_wins_over_legacy_shadow() {
    // A node declaring both elevation and shadow uses the material path.
    let text = r##"{
        "ir": "0.1.0", "app": "Elev", "state": [],
        "tree": { "id": 0, "kind": "Column", "props": { "padding": 8 },
            "children": [ { "id": 1, "kind": "Container", "props": {
                "color": "#102030", "width": 60, "height": 40, "radius": 4,
                "shadow": 6, "elevation": 2 },
                "children": [], "events": {} } ], "events": {} },
        "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 }
    }"##;
    let both = render(text);
    let only_shadow = render(&text.replace(", \"elevation\": 2", ""));
    assert_ne!(
        digest_hex(&both),
        digest_hex(&only_shadow),
        "elevation>0 wins over legacy shadow strength"
    );
}

#[test]
fn capture_records_elevation_only_when_set() {
    let ir = parse_ir(&box_ir(Some(4))).unwrap();
    let state = StateView::from_ir(&ir);
    let r = Renderer::new(240, 200);
    let dl = r.display_list(&ir, &state);
    assert!(
        dl.commands
            .iter()
            .any(|c| matches!(c, Command::EffectElevation { level: 4, .. })),
        "capture records the resolved elevation level"
    );
    let ir0 = parse_ir(&box_ir(None)).unwrap();
    let s0 = StateView::from_ir(&ir0);
    let dl0 = r.display_list(&ir0, &s0);
    assert!(
        !dl0.commands
            .iter()
            .any(|c| matches!(c, Command::EffectElevation { .. })),
        "no elevation command for a default node"
    );
}

#[test]
fn authored_elevation_change_invalidates_incremental_cache() {
    // The appearance hash must include authored elevation, otherwise a cached
    // shadowless node would be reused after elevation is added.
    let ir0 = parse_ir(&box_ir(None)).unwrap();
    let s0 = StateView::from_ir(&ir0);
    let ir3 = parse_ir(&box_ir(Some(3))).unwrap();
    let s3 = StateView::from_ir(&ir3);
    let mut r = Renderer::new(240, 200);
    r.render_incremental_at(&ir0, &s0, 0);
    r.render_incremental_at(&ir0, &s0, 0);
    assert_eq!(r.redraw_stats(), (0, 0), "seed settled");
    let incr = r.render_incremental_at(&ir3, &s3, 0).pixels;
    let (nodes, pixels) = r.redraw_stats();
    assert!(nodes > 0 && pixels > 0, "elevation change redraws the node");
    let mut full = Renderer::new(240, 200);
    let expected = full.render(&ir3, &s3).pixels;
    assert_eq!(
        digest_hex(&incr),
        digest_hex(&expected),
        "incremental equals full after elevation change"
    );
}

#[test]
fn default_path_keeps_the_two_counter_goldens() {
    let text = std::fs::read_to_string(COUNTER).unwrap();
    let ir = parse_ir(&text).unwrap();
    let state = StateView::from_ir(&ir);
    let mut r = Renderer::new(320, 240);
    assert_eq!(
        digest_hex(&r.render(&ir, &state).pixels),
        "8d45f412996a99c6"
    );
    let mut r2 = Renderer::new(800, 600);
    assert_eq!(
        digest_hex(&r2.render(&ir, &state).pixels),
        "318beb1dd337bc4e"
    );
}

#[test]
fn elevation_command_carries_the_effective_clip() {
    // gpu-dev ABI: the GPU replay expands `level` itself, so it must receive the
    // same clip the software `draw_elevation` masks with
    // (`canvas.effective_clip()`), otherwise a clipped node cannot be
    // byte-identical. Wire format is `"clip":[x,y,w,h]`.
    let ir_text = r##"{
        "ir": "0.1.0", "app": "Elev", "state": [],
        "tree": { "id": 0, "kind": "Column", "props": { "padding": 0 },
            "children": [ { "id": 1, "kind": "Scroll", "props": {
                "width": 80, "height": 40 },
                "children": [ { "id": 2, "kind": "Container", "props": {
                    "color": "#102030", "width": 60, "height": 40, "radius": 4,
                    "elevation": 4 },
                    "children": [], "events": {} } ], "events": {} } ], "events": {} },
        "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 }
    }"##;
    let ir = parse_ir(ir_text).unwrap();
    let state = StateView::from_ir(&ir);
    let r = Renderer::new(240, 200);
    let json = r.display_list(&ir, &state).to_json().unwrap();
    let v: Value = serde_json::from_str(&json).unwrap();
    let cmd = v["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["kind"] == "effect_elevation")
        .expect("clipped node still records its elevation command");
    assert_eq!(cmd["level"], 4);
    let clip = cmd["clip"]
        .as_array()
        .expect("clip must be an [x,y,w,h] array");
    let clip: Vec<i64> = clip.iter().map(|n| n.as_i64().unwrap()).collect();
    // The scroll viewport clip is narrower than the 240x200 canvas: the
    // command must carry the trimmed rect, not the full canvas.
    assert_eq!(clip, vec![0, 0, 80, 40], "effective clip, not the canvas");
    assert_eq!(clip[2] < 240, true, "non-full clip is actually exercised");

    // The software path is deterministic for the same clipped elevation.
    let a = render(ir_text);
    let b = render(ir_text);
    assert_eq!(a, b, "clipped elevation is byte-reproducible");
}
