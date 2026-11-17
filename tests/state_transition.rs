//! State-paint transitions: three-frame evidence, determinism and continuity.

use opurom_render::{digest_hex, parse_ir, NodeState, Renderer, StateView};

const W: u32 = 160;
const H: u32 = 96;
const START_MS: i64 = 400;
const MID_MS: i64 = 500;
const SETTLE_MS_LOCAL: i64 = 600;

fn ir(state: &str, animated: bool) -> String {
    let mut props = serde_json::json!({
        "width": 80, "height": 24, "label": "Go", "state": state
    });
    if animated {
        props["animate"] = serde_json::json!("ease");
        props["duration"] = serde_json::json!(200);
    }
    serde_json::to_string(&serde_json::json!({
        "ir":"0.1.0","app":"Transition","state":[],
        "tree":{"id":0,"kind":"Column","props":{"padding":8,"align":"start"},
            "children":[{"id":1,"kind":"Button","props":props,"children":[],"events":{}}],
            "events":{}},
        "theme":{"mode":"light","accent":"#097FEB","radius":4}
    }))
    .unwrap()
}

fn frame(renderer: &mut Renderer, ir: &opurom_render::Ir, tick: i64) -> String {
    digest_hex(&renderer.render_at(ir, &StateView::from_ir(ir), tick).pixels)
}

#[test]
fn three_frame_transitions_are_pinned_and_mid_differs_from_both_ends() {
    // Pins from the settled start at 400 ms, mid at 500 ms and settle at
    // 600 ms, with the authored ease/200 ms schedule (no new duration value).
    for (from, to, start, mid, end) in [
        (
            "default",
            "loading",
            "609464e4b1333421",
            "6a31840ac10b039d",
            "8083a7c7066c81cd",
        ),
        (
            "hover",
            "success",
            "20067c304cb654d2",
            "6e3aac3b16295589",
            "db60d54f8da1607d",
        ),
        (
            "disabled",
            "error",
            "9579ea1f9d0a4233",
            "aec59b35b537eedf",
            "f5dc435b22197fb5",
        ),
        (
            "error",
            "disabled",
            "f5dc435b22197fb5",
            "9117c23847b34f35",
            "9579ea1f9d0a4233",
        ),
    ] {
        let mut render_ir = parse_ir(&ir(from, true)).unwrap();
        let mut renderer = Renderer::new(W, H);
        let base = frame(&mut renderer, &render_ir, START_MS);
        assert_eq!(base, start, "{from} start pin");
        render_ir.tree.children[0].props.state = NodeState::parse(to).unwrap();

        let first = frame(&mut renderer, &render_ir, START_MS);
        let middle = frame(&mut renderer, &render_ir, MID_MS);
        let settled = frame(&mut renderer, &render_ir, SETTLE_MS_LOCAL);
        assert_eq!(first, start, "{from}->{to} t=0 must keep old paint");
        assert_eq!(middle, mid, "{from}->{to} mid pin");
        assert_eq!(settled, end, "{from}->{to} settle pin");
        assert_ne!(middle, first, "{from}->{to} mid equals start");
        assert_ne!(middle, settled, "{from}->{to} mid equals end");

        let mut again = Renderer::new(W, H);
        let mut repeat_ir = parse_ir(&ir(from, true)).unwrap();
        assert_eq!(frame(&mut again, &repeat_ir, START_MS), start);
        repeat_ir.tree.children[0].props.state = NodeState::parse(to).unwrap();
        assert_eq!(frame(&mut again, &repeat_ir, START_MS), first);
        assert_eq!(frame(&mut again, &repeat_ir, MID_MS), middle);
        assert_eq!(frame(&mut again, &repeat_ir, SETTLE_MS_LOCAL), settled);
    }
}

#[test]
fn sixty_and_one_twenty_hz_gradients_agree_at_shared_timestamps() {
    let mut coarse_ir = parse_ir(&ir("default", true)).unwrap();
    let mut coarse = Renderer::new(W, H);
    let base = frame(&mut coarse, &coarse_ir, START_MS);
    coarse_ir.tree.children[0].props.state = NodeState::Loading;
    let coarse_frames = [
        frame(&mut coarse, &coarse_ir, START_MS),
        frame(&mut coarse, &coarse_ir, MID_MS),
        frame(&mut coarse, &coarse_ir, SETTLE_MS_LOCAL),
    ];

    let mut fine_ir = parse_ir(&ir("default", true)).unwrap();
    let mut fine = Renderer::new(W, H);
    assert_eq!(frame(&mut fine, &fine_ir, START_MS), base);
    fine_ir.tree.children[0].props.state = NodeState::Loading;
    let at_400 = frame(&mut fine, &fine_ir, START_MS);
    let at_450 = frame(&mut fine, &fine_ir, 450);
    let at_500 = frame(&mut fine, &fine_ir, MID_MS);
    let at_550 = frame(&mut fine, &fine_ir, 550);
    let at_600 = frame(&mut fine, &fine_ir, SETTLE_MS_LOCAL);

    assert_eq!(coarse_frames[0], at_400);
    assert_eq!(coarse_frames[1], at_500);
    assert_eq!(coarse_frames[2], at_600);
    assert_ne!(at_450, at_400);
    assert_ne!(at_450, at_500);
    assert_ne!(at_550, at_500);
    assert_ne!(at_550, at_600);
}

#[test]
fn mid_flight_state_change_continues_from_current_blend() {
    let mut render_ir = parse_ir(&ir("default", true)).unwrap();
    let mut renderer = Renderer::new(W, H);
    frame(&mut renderer, &render_ir, START_MS);

    render_ir.tree.children[0].props.state = NodeState::Loading;
    frame(&mut renderer, &render_ir, START_MS);
    let blended = frame(&mut renderer, &render_ir, MID_MS);

    // A second state change at the same absolute tick must not jump: the
    // frame immediately after the switch equals the current blended frame.
    render_ir.tree.children[0].props.state = NodeState::Success;
    assert_eq!(frame(&mut renderer, &render_ir, MID_MS), blended);
    let after_mid = frame(&mut renderer, &render_ir, 550);
    let settled = frame(&mut renderer, &render_ir, 700);
    assert_ne!(after_mid, blended);
    assert_eq!(settled, "db60d54f8da1607d");
}

#[test]
fn no_animate_keeps_state_paint_discrete() {
    let mut render_ir = parse_ir(&ir("default", false)).unwrap();
    let mut renderer = Renderer::new(W, H);
    let base = frame(&mut renderer, &render_ir, START_MS);
    assert_eq!(base, "609464e4b1333421");
    render_ir.tree.children[0].props.state = NodeState::Loading;
    frame(&mut renderer, &render_ir, START_MS);
    assert_eq!(
        frame(&mut renderer, &render_ir, START_MS),
        "8083a7c7066c81cd"
    );
}

#[test]
fn focus_ring_geometry_is_instant_during_a_state_transition() {
    let mut render_ir = parse_ir(&ir("focus", true)).unwrap();
    let mut renderer = Renderer::new(W, H);
    let focus = frame(&mut renderer, &render_ir, START_MS);

    // The outer ring is driven by the resolved state, not the blended paint.
    render_ir.tree.children[0].props.state = NodeState::Loading;
    let ring_gone = frame(&mut renderer, &render_ir, START_MS);
    assert_ne!(ring_gone, focus);
    assert_eq!(ring_gone, "609464e4b1333421");
}
