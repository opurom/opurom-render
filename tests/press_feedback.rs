//! A3 press feedback (ANIMATION_AND_PHYSICS.md §8): caller-owned `PressPhase`
//! drives the Active state and the InkRipple on the nearest Button host. No
//! phase is byte-identical to `render_at`; authored `disabled` still wins.

use opurom_render::{digest_hex, parse_ir, PressPhase, Renderer, StateView};

fn compat_renderer(width: u32, height: u32) -> Renderer {
    let mut renderer = Renderer::new(width, height);
    renderer.set_quality(opurom_render::QualityProfile::Compat);
    renderer
}

const COUNTER_PATH: &str = "../opurom-examples/golden/counter.opuir.json";

fn button_ir(extra_props: &str) -> String {
    format!(
        r##"{{
        "ir": "0.1.0", "app": "Press", "state": [],
        "tree": {{ "id": 0, "kind": "Column", "props": {{ "padding": 8 }},
            "children": [
                {{ "id": 1, "kind": "Button", "props": {{
                    "color": "#102030", "width": 80, "height": 40, "radius": 8 {extra_props} }},
                    "children": [], "events": {{}} }}
            ], "events": {{}} }},
        "theme": {{ "mode": "light", "accent": "#097FEB", "radius": 4 }}
    }}"##
    )
}

fn frame(ir_text: &str, elapsed_ms: i64, press: Option<&PressPhase>) -> Vec<u8> {
    let ir = parse_ir(ir_text).expect("IR must parse");
    let state = StateView::from_ir(&ir);
    let mut r = compat_renderer(240, 200);
    r.render_with_press(&ir, &state, elapsed_ms, press).pixels
}

fn phase(start_tick: i64, confirm: Option<i64>, touch: Option<(i64, i64)>) -> PressPhase {
    PressPhase {
        node_id: 1,
        press_start_tick: start_tick,
        confirm_tick: confirm,
        touch,
    }
}

#[test]
fn no_phase_is_byte_identical_to_render_at() {
    let ir_text = button_ir("");
    let ir = parse_ir(&ir_text).unwrap();
    let state = StateView::from_ir(&ir);
    let mut r = compat_renderer(240, 200);
    let plain = r.render(&ir, &state).pixels;
    let ir2 = parse_ir(&ir_text).unwrap();
    let no_phase = r2_frame(&ir2, &state);
    assert_eq!(digest_hex(&plain), digest_hex(&no_phase));
}

fn r2_frame(ir: &opurom_render::Ir, state: &StateView) -> Vec<u8> {
    compat_renderer(240, 200)
        .render_with_press(ir, state, 0, None)
        .pixels
}

#[test]
fn press_activates_the_node_and_disabled_still_wins() {
    let base = button_ir("");
    let pressed = frame(&base, 0, Some(&phase(0, None, None)));
    let idle = frame(&base, 0, None);
    assert_ne!(digest_hex(&pressed), digest_hex(&idle));
    let disabled = button_ir(", \"state\": \"disabled\"");
    assert_eq!(
        digest_hex(&frame(&disabled, 0, Some(&phase(0, None, None)))),
        digest_hex(&frame(&disabled, 0, None))
    );
}

#[test]
fn held_ripple_keeps_growing_and_never_fades() {
    let ir = button_ir("");
    let early = frame(&ir, 75, Some(&phase(0, None, None)));
    let mid = frame(&ir, 400, Some(&phase(0, None, None)));
    let late = frame(&ir, 1200, Some(&phase(0, None, None)));
    assert_ne!(
        digest_hex(&early),
        digest_hex(&mid),
        "radius grows while held"
    );
    assert_ne!(
        digest_hex(&mid),
        digest_hex(&late),
        "1 s expansion continues"
    );
    // Past one second Flutter holds the full radius at full alpha: stable.
    assert_eq!(
        digest_hex(&late),
        digest_hex(&frame(&ir, 2000, Some(&phase(0, None, None))))
    );
}

#[test]
fn confirmed_ripple_accelerates_then_fades_and_disappears() {
    let ir = button_ir("");
    let held = frame(&ir, 300, Some(&phase(0, None, None)));
    let just_released = frame(&ir, 300, Some(&phase(0, Some(300), None)));
    assert_eq!(
        digest_hex(&held),
        digest_hex(&just_released),
        "release continues from the held radius"
    );
    let at_525 = frame(&ir, 525, Some(&phase(0, Some(300), None)));
    assert_ne!(digest_hex(&held), digest_hex(&at_525), "radius accelerates");
    let at_600 = frame(&ir, 600, Some(&phase(0, Some(300), None)));
    assert_ne!(
        digest_hex(&at_525),
        digest_hex(&at_600),
        "alpha fades at 225 ms"
    );
    let gone = frame(&ir, 700, Some(&phase(0, Some(300), None)));
    let idle_pressed = frame(&ir, 700, Some(&phase(0, Some(300), None)));
    assert_eq!(digest_hex(&gone), digest_hex(&idle_pressed));
}

#[test]
fn touch_moves_the_ripple_centre() {
    let ir = button_ir("");
    let centred = frame(&ir, 110, Some(&phase(0, None, None)));
    let offset = frame(&ir, 110, Some(&phase(0, None, Some((10, 5)))));
    assert_ne!(digest_hex(&centred), digest_hex(&offset));
}

#[test]
fn default_path_keeps_the_two_counter_goldens() {
    let text = std::fs::read_to_string(COUNTER_PATH).unwrap();
    let ir = parse_ir(&text).unwrap();
    let state = StateView::from_ir(&ir);
    assert_eq!(
        digest_hex(&compat_renderer(320, 240).render(&ir, &state).pixels),
        "8d45f412996a99c6"
    );
    assert_eq!(
        digest_hex(&compat_renderer(800, 600).render(&ir, &state).pixels),
        "318beb1dd337bc4e"
    );
}
