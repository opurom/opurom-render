//! A `glass: true` surface is a liquid-glass lamina on every backend: the
//! software framebuffer refracts the backdrop and the display list carries the
//! matching GPU effect commands. IR that declares no glass stays unchanged.

use opurom_render::{digest_hex, parse_ir, Command, Renderer, Rgba, StateView, ThemeMode};

const BASE: &str = r##"{
  "ir": "0.1", "app": "GlassBridge",
  "state": [],
  "theme": { "mode": "light", "accent": "#4F46E5", "radius": 14,
             "tokens": { "stage": "#EDF0F7" } },
  "tree": { "id": 0, "kind": "Column", "props": { "gap": 10, "padding": 12 },
    "children": [
      { "id": 1, "kind": "Container",
        "props": { "width": 160, "height": 40, "color": "#FF5500" },
        "children": [], "events": {} },
      { "id": 2, "kind": "Card",
        "props": { "width": 120, "height": 60, "radius": 16, "color": "#FFFFFF" GLASS },
        "children": [], "events": {} }
    ], "events": {} }
}"##;

fn ir(glass: bool) -> String {
    BASE.replace(" GLASS", if glass { ", \"glass\": true" } else { "" })
}

fn frame(glass: bool) -> Vec<u8> {
    let ir = parse_ir(&ir(glass)).unwrap();
    let state = StateView::from_ir(&ir);
    Renderer::new(200, 140).pixels(&ir, &state)
}

fn command_kinds(glass: bool) -> Vec<&'static str> {
    let ir = parse_ir(&ir(glass)).unwrap();
    let state = StateView::from_ir(&ir);
    let list = Renderer::new(200, 140).display_list(&ir, &state);
    list.commands
        .iter()
        .filter_map(|c| match c {
            Command::EffectBackdrop => Some("effect_backdrop"),
            Command::EffectGlass { .. } => Some("effect_glass"),
            Command::EffectLiquid { .. } => Some("effect_liquid"),
            Command::EffectShadow { .. }
            | Command::EffectGlow { .. }
            | Command::EffectBlur { .. } => Some("other_effect"),
            _ => None,
        })
        .collect()
}

#[test]
fn glass_boolean_changes_pixels_and_emits_native_glass_commands() {
    assert_ne!(
        digest_hex(&frame(true)),
        digest_hex(&frame(false)),
        "glass must refract the backdrop in the software framebuffer"
    );
    let kinds = command_kinds(true);
    assert_eq!(
        kinds.iter().filter(|k| **k == "effect_glass").count(),
        1,
        "one glass lamina command"
    );
    assert_eq!(
        kinds.iter().filter(|k| **k == "effect_liquid").count(),
        1,
        "one specular edge command"
    );
    assert!(
        kinds.iter().any(|k| *k == "effect_backdrop"),
        "the backdrop snapshot must precede the lamina"
    );
}

#[test]
fn no_glass_emits_no_effect_commands() {
    assert!(
        command_kinds(false).is_empty(),
        "an effect-free IR must keep the byte-identical command stream"
    );
}

#[test]
fn stage_token_is_painted_as_the_page_backdrop() {
    let ir = parse_ir(&ir(false)).unwrap();
    assert_eq!(ir.theme.mode, ThemeMode::Light);
    assert_eq!(ir.theme.tokens.stage, Some(Rgba::rgb(0xed, 0xf0, 0xf7)));
    let px = frame(false);
    assert_eq!(
        &px[0..3],
        &[0xed, 0xf0, 0xf7],
        "corner pixel is the stage tint"
    );
}
