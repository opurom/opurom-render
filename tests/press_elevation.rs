//! Press elevation (DEFAULT_UI_GLASS_REFINEMENT.md §12.5, ANIMATION §8): with
//! the elevation switch on, a pressed node with shadow>0 draws one level up;
//! disabled/no-press/shadow-0 and switch-off paths are byte-identical.

use opurom_render::{digest_hex, parse_ir, PressPhase, Renderer, StateView};

const COUNTER: &str = "../opurom-examples/golden/counter.opuir.json";

fn node_ir(shadow: i64, state: Option<&str>) -> String {
    let state_prop = state
        .map(|s| format!(",\"state\":\"{s}\""))
        .unwrap_or_default();
    format!(
        r##"{{
        "ir": "0.1.0", "app": "PressElev", "state": [],
        "tree": {{ "id": 0, "kind": "Column", "props": {{ "gap": 2 }},
            "children": [ {{ "id": 1, "kind": "Button", "props": {{
                "color": "#102030", "width": 60, "height": 40, "radius": 4,
                "shadow": {shadow}{state_prop} }},
                "children": [], "events": {{}} }} ], "events": {{}} }},
        "theme": {{ "mode": "light", "accent": "#097FEB", "radius": 4 }}
    }}"##
    )
}

fn phase() -> PressPhase {
    PressPhase {
        node_id: 1,
        press_start_tick: 0,
        confirm_tick: None,
        touch: None,
    }
}

fn render(
    ir_text: &str,
    elevation: bool,
    press: Option<&PressPhase>,
    incremental: bool,
) -> Vec<u8> {
    let ir = parse_ir(ir_text).unwrap();
    let state = StateView::from_ir(&ir);
    let mut r = Renderer::new(240, 200);
    r.set_elevation_shadows(elevation);
    let frame = if incremental {
        r.render_with_press_incremental(&ir, &state, 0, press)
    } else {
        r.render_with_press(&ir, &state, 0, press)
    };
    frame.pixels
}

#[test]
fn press_raises_the_elevation_level() {
    let ir_text = node_ir(4, None);
    let idle = render(&ir_text, true, None, false);
    let pressed = render(&ir_text, true, Some(&phase()), false);
    assert_ne!(
        digest_hex(&idle),
        digest_hex(&pressed),
        "press with elevation on raises the shadow one level"
    );
    // Without the elevation switch, press changes only the Active fill/inset,
    // not a material shadow; still differs from idle but is a different path.
    let plain_press = render(&ir_text, false, Some(&phase()), false);
    assert_ne!(digest_hex(&pressed), digest_hex(&plain_press));
}

#[test]
fn disabled_node_is_not_elevated_by_press() {
    let ir_text = node_ir(4, Some("disabled"));
    let idle = render(&ir_text, true, None, false);
    let pressed = render(&ir_text, true, Some(&phase()), false);
    assert_eq!(
        digest_hex(&idle),
        digest_hex(&pressed),
        "authored disabled wins: no press elevation"
    );
}

#[test]
fn shadow_zero_and_switch_off_are_no_ops() {
    let no_shadow = node_ir(0, None);
    // shadow 0 has no shadow to raise: elevation on/off under press are equal
    // (both show only the ordinary Active appearance, no material shadow).
    let elev_on = render(&no_shadow, true, Some(&phase()), false);
    let elev_off = render(&no_shadow, false, Some(&phase()), false);
    assert_eq!(
        digest_hex(&elev_on),
        digest_hex(&elev_off),
        "shadow 0: no elevation to raise"
    );

    // Switch off: press equals the ordinary legacy press path with switch off.
    let ir_text = node_ir(4, None);
    let off_press = render(&ir_text, false, Some(&phase()), false);
    let ir = parse_ir(&ir_text).unwrap();
    let state = StateView::from_ir(&ir);
    let mut r = Renderer::new(240, 200);
    let expected = r.render_with_press(&ir, &state, 0, Some(&phase())).pixels;
    assert_eq!(
        digest_hex(&off_press),
        digest_hex(&expected),
        "switch off is default"
    );
}

#[test]
fn level_fallback_stays_defined_under_press() {
    // Strength 5 falls back to level 4; press asks for 6 (min(5+1,6)=6).
    // Strength 8 already at 6; press keeps 6. Both must be deterministic.
    for s in [5, 8] {
        let ir_text = node_ir(s, None);
        let a = render(&ir_text, true, Some(&phase()), false);
        let b = render(&ir_text, true, Some(&phase()), false);
        assert_eq!(
            digest_hex(&a),
            digest_hex(&b),
            "strength {s} press deterministic"
        );
    }
}

#[test]
fn full_and_incremental_paths_agree() {
    for s in [1, 4, 5, 6, 8] {
        let ir_text = node_ir(s, None);
        let full = render(&ir_text, true, Some(&phase()), false);
        let incr = render(&ir_text, true, Some(&phase()), true);
        assert_eq!(
            digest_hex(&full),
            digest_hex(&incr),
            "full == incremental under press elevation at strength {s}"
        );
    }
}

#[test]
fn capture_command_stream_unchanged_with_press_elevation() {
    let ir_text = node_ir(4, None);
    let ir = parse_ir(&ir_text).unwrap();
    let state = StateView::from_ir(&ir);
    let mut off = Renderer::new(240, 200);
    off.set_elevation_shadows(false);
    let mut on = Renderer::new(240, 200);
    on.set_elevation_shadows(true);
    let d_off = off.display_list(&ir, &state);
    let d_on = on.display_list(&ir, &state);
    assert_eq!(
        format!("{:?}", d_off.commands),
        format!("{:?}", d_on.commands),
        "display-list command stream ignores elevation switch"
    );
}

#[test]
fn default_path_keeps_the_two_counter_goldens() {
    let text = std::fs::read_to_string(COUNTER).unwrap();
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
