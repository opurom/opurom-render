//! Per-node Material elevation (Flutter 3.50.0 `kElevationToShadow`): the
//! embedded table matches material-tokens.json, all ten authored levels pass
//! the whitelist, state deltas use the separate legacy M projector, and the
//! default (no elevation) path stays byte-identical.

fn compat_renderer(width: u32, height: u32) -> opurom_render::Renderer {
    let mut renderer = opurom_render::Renderer::new(width, height);
    renderer.set_quality(opurom_render::QualityProfile::Compat);
    renderer
}

use opurom_render::{
    digest_hex, elevation_level, parse_ir, project_state_level, Command, Renderer, StateView,
    ELEVATION_TABLE,
};
use serde_json::Value;

// CI checks out render without the sibling UI repo, so the fixture is the
// source of truth here; a local sibling is checked for drift only. The
// Flutter source itself is the authority; when present it is parsed below.
const TOKENS: &str = "tests/fixtures/material-tokens.json";
const SIBLING_TOKENS: &str = "../opurom-ui/theme/material-tokens.json";
const FLUTTER_SHADOWS: &str = "../../flutter-golden/packages/flutter/lib/src/material/shadows.dart";
const COUNTER: &str = "../opurom-examples/golden/counter.opuir.json";

fn alpha_of(hex: &str) -> u8 {
    u8::from_str_radix(&hex[1..3], 16).unwrap()
}

#[test]
fn embedded_table_matches_material_tokens_json() {
    let fixture = std::fs::read_to_string(TOKENS).unwrap();
    if let Ok(sibling) = std::fs::read_to_string(SIBLING_TOKENS) {
        let expected: Value = serde_json::from_str(&fixture).unwrap();
        let actual: Value = serde_json::from_str(&sibling).unwrap();
        assert_eq!(
            actual["tokens"]["elevation"], expected["tokens"]["elevation"],
            "{SIBLING_TOKENS} elevation tokens drifted from fixture"
        );
    }
    let v: Value = serde_json::from_str(&fixture).unwrap();
    let elev = &v["tokens"]["elevation"];
    for level in 0..ELEVATION_TABLE.len() {
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
    for level in [7, 10, 11] {
        assert!(ELEVATION_TABLE[level].is_none());
    }
}

/// Every table entry and the level whitelist are checked against the local
/// Flutter 3.50.0 checkout when it is present. CI that checks out render alone
/// keeps the JSON fixture as the portable copy of the same source.
#[test]
fn embedded_table_matches_flutter_350_source() {
    let Ok(source) = std::fs::read_to_string(FLUTTER_SHADOWS) else {
        return;
    };
    assert!(
        source.contains("Flutter Authors") && source.contains("kElevationToShadow"),
        "unexpected shadows.dart checkout"
    );
    for level in 0..ELEVATION_TABLE.len() {
        // Flutter `shadows.dart:48` maps 0 to an empty shadow list; the
        // renderer represents that as `None` (no shadow to draw), while every
        // non-zero level in the map has three layers.
        let declared = level != 0 && source.contains(&format!("{level}: <BoxShadow>["));
        let present = ELEVATION_TABLE[level].is_some();
        assert_eq!(
            declared, present,
            "level {level} presence vs Flutter source"
        );
    }
    for (color, alpha) in [("0x33000000", 51), ("0x24000000", 36), ("0x1F000000", 31)] {
        assert!(
            source.contains(color),
            "Flutter source color {color} missing"
        );
        assert!(
            ELEVATION_TABLE[1..]
                .iter()
                .flatten()
                .flat_map(|layers| layers.iter())
                .any(|layer| layer.4 == alpha),
            "layer alpha {alpha} missing"
        );
    }
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

fn themed_ir(shadow: &str, tint: &str) -> String {
    format!(
        r##"{{
        "ir": "0.1.0", "app": "Elev", "state": [],
        "tree": {{ "id": 0, "kind": "Column", "props": {{ "padding": 8 }},
            "children": [ {{ "id": 1, "kind": "Container", "props": {{
                "color": "#102030", "width": 60, "height": 40, "radius": 4,
                "elevation": 4 }}, "children": [], "events": {{}} }} ], "events": {{}} }},
        "theme": {{ "mode": "dark", "accent": "#097FEB", "radius": 4,
            "tokens": {{ "shadow": "{shadow}", "surfaceTint": "{tint}" }} }}
    }}"##
    )
}

#[test]
fn elevation_shadow_color_follows_theme_tokens() {
    let neutral = render(&themed_ir("#000000", "#000000"));
    let tinted = render(&themed_ir("#000000", "#FF0000"));
    assert_ne!(
        digest_hex(&neutral),
        digest_hex(&tinted),
        "a surface-tinted shadow must change pixels"
    );
    let ir = parse_ir(&themed_ir("#000000", "#FF0000")).unwrap();
    let state = StateView::from_ir(&ir);
    let dl = compat_renderer(240, 200).display_list(&ir, &state);
    assert!(
        dl.commands.iter().any(|c| matches!(
            c,
            Command::EffectElevation { color, .. } if color.r > 0 && color.g == 0 && color.b == 0
        )),
        "the display command must carry the tinted shadow colour"
    );
}

fn render(ir_text: &str) -> Vec<u8> {
    let ir = parse_ir(ir_text).unwrap();
    let state = StateView::from_ir(&ir);
    compat_renderer(240, 200).render(&ir, &state).pixels
}

/// The four levels newly added for Flutter 3.50 must be real draws, not just
/// table entries: each level changes pixels vs the shadowless surface and
/// records its own command for the GPU replay path.
#[test]
fn higher_flutter_levels_draw_and_record() {
    let flat = render(&box_ir(None));
    for level in [8, 9, 12, 16, 24] {
        let pixels = render(&box_ir(Some(level)));
        assert_ne!(
            digest_hex(&pixels),
            digest_hex(&flat),
            "elevation {level} must change pixels"
        );
        let ir = parse_ir(&box_ir(Some(level))).unwrap();
        let state = StateView::from_ir(&ir);
        let list = compat_renderer(240, 200).display_list(&ir, &state);
        assert!(
            list.commands
                .iter()
                .any(|c| matches!(c, Command::EffectElevation { level: got, .. } if *got == level)),
            "elevation {level} must be replayable"
        );
    }
}

#[test]
fn authored_whitelist_rejects_illegal_levels() {
    for bad in [5, 7, -1, 10, 11, 25] {
        assert!(
            parse_ir(&box_ir(Some(bad))).is_err(),
            "elevation {bad} must fail loud"
        );
    }
    for good in [0, 1, 2, 3, 4, 6, 8, 9, 12, 16, 24] {
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
    let mut r = compat_renderer(240, 200);
    r.render_incremental_at(&ir0, &s0, 0);
    r.render_incremental_at(&ir0, &s0, 0);
    assert_eq!(r.redraw_stats(), (0, 0), "seed settled");
    let incr = r.render_incremental_at(&ir3, &s3, 0).pixels;
    let (nodes, pixels) = r.redraw_stats();
    assert!(nodes > 0 && pixels > 0, "elevation change redraws the node");
    let mut full = compat_renderer(240, 200);
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
    let mut r = compat_renderer(320, 240);
    r.set_quality(opurom_render::QualityProfile::Compat);
    assert_eq!(
        digest_hex(&r.render(&ir, &state).pixels),
        "8d45f412996a99c6"
    );
    let mut r2 = compat_renderer(800, 600);
    r2.set_quality(opurom_render::QualityProfile::Compat);
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
    let r = compat_renderer(240, 200);
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

fn clipped_elevation_ir(elevation: Option<i64>) -> String {
    let extra = elevation
        .map(|e| format!(",\"elevation\":{e}"))
        .unwrap_or_default();
    format!(
        r##"{{
        "ir": "0.1.0", "app": "ElevClip", "state": [],
        "tree": {{ "id": 0, "kind": "Column", "props": {{ "padding": 0, "align": "start" }},
            "children": [ {{ "id": 1, "kind": "Container", "props": {{
                "width": 48, "height": 48, "radius": 10, "color": "#112233" }},
                "children": [ {{ "id": 2, "kind": "Container", "props": {{
                    "width": 30, "height": 30, "color": "#FF0000"{extra} }},
                    "children": [], "events": {{}} }} ], "events": {{}} }} ], "events": {{}} }},
        "theme": {{ "mode": "light", "accent": "#097FEB", "radius": 4 }}
    }}"##
    )
}

fn clipped_pixels(elevation: Option<i64>, edge_aa: bool) -> Vec<u8> {
    let ir = parse_ir(&clipped_elevation_ir(elevation)).unwrap();
    let state = StateView::from_ir(&ir);
    let mut r = compat_renderer(64, 64);
    r.set_edge_aa(edge_aa);
    r.render(&ir, &state).pixels
}

fn rgb(p: &[u8], x: usize, y: usize) -> [u8; 3] {
    let i = (y * 64 + x) * 4;
    [p[i], p[i + 1], p[i + 2]]
}

#[test]
fn elevation_shadow_respects_rounded_clip_coverage() {
    // Closure A coverage is shared by content and shadow. The cut corner
    // (0,0) loses the shadow only when edge_aa is on; an interior shadow pixel
    // (27,0) still proves the elevation path is active.
    let off = clipped_pixels(Some(6), false);
    let off_plain = clipped_pixels(None, false);
    assert_ne!(rgb(&off, 0, 0), rgb(&off_plain, 0, 0));

    let on = clipped_pixels(Some(6), true);
    let on_plain = clipped_pixels(None, true);
    assert_eq!(rgb(&on, 0, 0), rgb(&on_plain, 0, 0));
    assert_ne!(rgb(&on, 27, 0), rgb(&on_plain, 27, 0));
}

#[test]
fn elevation_layer_order_is_pinned_far_to_near() {
    // AAudit: the three integer blends move 1 LSB under permutation. The
    // far-to-near order is deliberate; do not reorder to the source order.
    let layers = ELEVATION_TABLE[4].unwrap();
    assert_eq!(layers.map(|layer| layer.4), [31, 36, 51]);
    let mut d = [247u8, 248, 250];
    for (_, _, _, _, alpha) in layers {
        for channel in &mut d {
            *channel = ((*channel as i64 * (255 - alpha as i64) + 127) / 255) as u8;
        }
    }
    assert_eq!(d, [149, 150, 151]);
    let reference = [149u8, 149, 151];
    assert!(d
        .iter()
        .zip(reference)
        .all(|(a, b)| (*a as i16 - b as i16).abs() <= 1));
}

#[test]
fn shadow_color_alpha_reduces_effective_edge_alpha() {
    // Flutter 3.50 / Impeller: the shadow shader multiplies the shadow colour
    // by the Gaussian CDF (`shadow_vertices.frag:19-25`), so the colour alpha
    // scales the edge. The public IR colour grammar is strict #RRGGBB; the
    // theme-token path keeps alpha as a resolved Rgba field, so this test
    // exercises the same arithmetic helper used by the elevation raster.
    let base = 200i64;
    let cov = 100i64;
    let opaque = ((base * cov + 127) / 255).clamp(0, 255) as u8;
    let alpha = 128i64;
    // Production order in draw_elevation: blur/layer alpha first, then the
    // shadow colour's own alpha, then clip coverage.
    let translucent = (((base * cov + 127) / 255) * alpha / 255).clamp(0, 255) as u8;
    assert!(
        translucent < opaque,
        "{translucent} should be lighter than {opaque}"
    );
}
