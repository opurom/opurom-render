//! Exhaustive Material 3 colour-role parity for the deterministic renderer.
//!
//! Flutter authority: framework 3.50.0 (`ab2912035e`),
//! `packages/flutter/lib/src/material/color_scheme.dart:447-1155`; values are
//! pinned in `opurom-ui/theme/material-tokens.json`. This test checks both
//! modes, local overrides and the legacy golden path without invoking a
//! runtime interpreter.

use opurom_render::{digest_hex, layout, parse_ir, Renderer, Rgba, StateView, ThemeTokens};
use serde_json::json;
use std::fs;

const COUNTER_PATH: &str = "../opurom-examples/golden/counter.opuir.json";

fn ir(role: &str, mode: &str, extra_tokens: serde_json::Value) -> String {
    let mut theme = json!({"mode": mode, "accent": "#097FEB", "radius": 0});
    theme["tokens"] = extra_tokens;
    serde_json::to_string(&json!({
        "ir": "0.1.0", "app": "M3Tokens", "state": [],
        "tree": {"id": 0, "kind": "Text",
            "props": {"value": "x", "color": format!("@md-sys-color-{role}")},
            "children": [], "events": {}},
        "theme": theme
    }))
    .unwrap()
}

fn paint(text: &str) -> Option<Rgba> {
    let ir = parse_ir(text).unwrap();
    let laid = layout(&ir, &StateView::from_ir(&ir), 240, 200);
    laid.first().and_then(|n| n.text_color)
}

#[test]
fn all_49_roles_resolve_light_and_dark_from_the_pinned_manifest() {
    let doc: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/material-tokens.json")).unwrap();
    let colors = &doc["tokens"]["color"];
    let light = colors["light"].as_object().unwrap();
    let dark = colors["dark"].as_object().unwrap();
    assert_eq!(light.len(), 49);
    assert_eq!(light.len(), dark.len());
    for (key, entry) in light {
        let role = key.strip_prefix("md-sys-color-").unwrap();
        assert!(dark.contains_key(key), "{role} must exist in both modes");
        for (mode, expected) in [("light", entry), ("dark", &dark[key])] {
            let s = expected["value"].as_str().unwrap().trim_start_matches('#');
            let want = Rgba::rgb(
                u8::from_str_radix(&s[0..2], 16).unwrap(),
                u8::from_str_radix(&s[2..4], 16).unwrap(),
                u8::from_str_radix(&s[4..6], 16).unwrap(),
            );
            let got = paint(&ir(role, mode, json!({}))).expect(role);
            assert_eq!(got, want, "{role}/{mode}");
        }
    }
}

#[test]
fn an_unknown_role_is_ignored_without_painting() {
    // Unknown roles never bind a paint: the node falls back to theme ink and
    // the hex/default path stays deterministic.
    let parsed = parse_ir(&ir("not-a-role", "light", json!({}))).unwrap();
    assert!(parsed.tree.props.color.is_none());
    assert!(paint(&ir("not-a-role", "light", json!({}))).is_some());
}

#[test]
fn local_token_override_wins_in_both_modes() {
    for mode in ["light", "dark"] {
        let override_color = if mode == "light" {
            "#123456"
        } else {
            "#654321"
        };
        let got = paint(&ir("primary", mode, json!({"primary": override_color}))).unwrap();
        let hex = override_color.trim_start_matches('#');
        let want = Rgba::rgb(
            u8::from_str_radix(&hex[0..2], 16).unwrap(),
            u8::from_str_radix(&hex[2..4], 16).unwrap(),
            u8::from_str_radix(&hex[4..6], 16).unwrap(),
        );
        assert_eq!(got, want, "{mode} override");
    }
}

#[test]
fn override_on_another_role_does_not_change_primary() {
    let a = paint(&ir("primary", "light", json!({}))).unwrap();
    let b = paint(&ir("primary", "light", json!({"secondary": "#00FF00"}))).unwrap();
    assert_eq!(a, b);
}

#[test]
fn local_override_changes_the_rendered_frame_deterministically() {
    let plain = ir("primary", "dark", json!({})).replace(
        "\"value\": \"x\"",
        "\"value\": \"x\", \"width\": 40, \"height\": 20",
    );
    let overridden = ir("primary", "dark", json!({"primary": "#123456"})).replace(
        "\"value\": \"x\"",
        "\"value\": \"x\", \"width\": 40, \"height\": 20",
    );
    let digest = |text: &str| {
        let ir = parse_ir(text).unwrap();
        let state = StateView::from_ir(&ir);
        digest_hex(&Renderer::new(160, 96).render(&ir, &state).pixels)
    };
    assert_ne!(digest(&plain), digest(&overridden));
    assert_eq!(digest(&overridden), digest(&overridden));
    assert_ne!(
        ThemeTokens::default().primary,
        Some(Rgba::rgb(0x12, 0x34, 0x56))
    );
}

#[test]
fn every_full_md_role_override_is_read_by_the_renderer() {
    let doc: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/material-tokens.json")).unwrap();
    let light = doc["tokens"]["color"]["light"].as_object().unwrap();
    let tokens: serde_json::Value = light
        .iter()
        .map(|(key, entry)| (key.clone(), entry["value"].clone()))
        .collect();
    for key in light.keys() {
        let role = key.strip_prefix("md-sys-color-").unwrap();
        let text = ir(role, "light", tokens.clone());
        let parsed = parse_ir(&text).unwrap();
        assert_eq!(
            parsed.theme.tokens.m3.len(),
            light.len(),
            "all imported roles must be retained"
        );
        assert!(
            paint(&text).is_some(),
            "{role}: imported role must still produce a paint"
        );
    }
}

#[test]
fn counter_goldens_remain_byte_identical() {
    let text = fs::read_to_string(COUNTER_PATH).unwrap();
    let ir = parse_ir(&text).unwrap();
    let state = StateView::from_ir(&ir);
    assert_eq!(
        digest_hex(&Renderer::new(320, 240).render(&ir, &state).pixels),
        "8d45f412996a99c6"
    );
    assert_eq!(
        digest_hex(&Renderer::new(800, 600).render(&ir, &state).pixels),
        "318beb1dd337bc4e"
    );
}
