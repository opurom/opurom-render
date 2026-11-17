//! State-driven per-node elevation (ELEVATION_PER_NODE_PROPOSAL.md §3.4):
//! authored elevation + Flutter state deltas (disabled 0, pressed +6,
//! hovered/focused +2) projected through M; idle/default stays byte-identical.

use opurom_render::{digest_hex, parse_ir, NodeState, PressPhase, Renderer, StateView};

const COUNTER: &str = "../opurom-examples/golden/counter.opuir.json";

fn node_ir(elevation: i64, state: Option<&str>) -> String {
    let state_prop = state
        .map(|s| format!(",\"state\":\"{s}\""))
        .unwrap_or_default();
    format!(
        r##"{{
        "ir": "0.1.0", "app": "StateElev", "state": [],
        "tree": {{ "id": 0, "kind": "Column", "props": {{ "gap": 2 }},
            "children": [ {{ "id": 1, "kind": "Button", "props": {{
                "color": "#102030", "width": 60, "height": 40, "radius": 4,
                "elevation": {elevation}{state_prop} }},
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

fn render_press(ir_text: &str, press: Option<&PressPhase>) -> Vec<u8> {
    let ir = parse_ir(ir_text).unwrap();
    let state = StateView::from_ir(&ir);
    Renderer::new(240, 200)
        .render_with_press(&ir, &state, 0, press)
        .pixels
}

#[test]
fn press_raises_authored_elevation() {
    // Exact state->level mapping (Chief's pinned cases), independent of the
    // fill/tint pixels a press also changes.
    assert_eq!(NodeState::Default.elevation(1), 1);
    assert_eq!(NodeState::Active.elevation(1), 6); // 1+6=7 -> M=6
    assert_eq!(NodeState::Active.elevation(3), 6); // 3+6=9 -> M=6
    assert_eq!(NodeState::Disabled.elevation(3), 0);
    // A node with no declared elevation stays shadowless in every state.
    for s in [
        NodeState::Default,
        NodeState::Hover,
        NodeState::Active,
        NodeState::Focus,
        NodeState::Disabled,
    ] {
        assert_eq!(s.elevation(0), 0, "no authored elevation stays 0");
    }
}

#[test]
fn disabled_node_has_no_elevation() {
    // Authored disabled wins: M(0)=0 regardless of press. Both frames equal.
    let ir_text = node_ir(3, Some("disabled"));
    let idle = render_press(&ir_text, None);
    let pressed = render_press(&ir_text, Some(&phase()));
    assert_eq!(
        digest_hex(&idle),
        digest_hex(&pressed),
        "authored disabled suppresses state elevation"
    );
}

#[test]
fn hover_and_active_project_to_legal_levels() {
    // hovered/focused -> M(e+2); active/pressed -> M(e+6).
    assert_eq!(NodeState::Hover.elevation(1), 3); // 1+2=3
    assert_eq!(NodeState::Focus.elevation(1), 3);
    assert_eq!(NodeState::Hover.elevation(3), 4); // 3+2=5 -> M=4
    assert_eq!(NodeState::Active.elevation(1), 6); // 1+6=7 -> M=6
                                                   // Business states keep the authored base level.
    assert_eq!(NodeState::Loading.elevation(2), 2);
    assert_eq!(NodeState::Success.elevation(4), 4);
    assert_eq!(NodeState::Error.elevation(6), 6);
}

#[test]
fn state_elevation_is_deterministic_and_incremental_agrees() {
    let ir_text = node_ir(2, None);
    let a = render_press(&ir_text, Some(&phase()));
    let b = render_press(&ir_text, Some(&phase()));
    assert_eq!(digest_hex(&a), digest_hex(&b), "deterministic");

    let ir = parse_ir(&ir_text).unwrap();
    let state = StateView::from_ir(&ir);
    let incr = Renderer::new(240, 200)
        .render_with_press_incremental(&ir, &state, 0, Some(&phase()))
        .pixels;
    assert_eq!(
        digest_hex(&a),
        digest_hex(&incr),
        "full == incremental under state elevation"
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
