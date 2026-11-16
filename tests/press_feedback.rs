//! A3 press feedback (ANIMATION_AND_PHYSICS.md §8): caller-owned `PressPhase`
//! drives the Active state and the ripple through `render_with_press`. No phase
//! is byte-identical to `render_at`; authored `disabled` still wins.

use opurom_render::{digest_hex, parse_ir, PressPhase, Renderer, StateView};

const COUNTER_PATH: &str = "../opurom-examples/golden/counter.opuir.json";

fn box_ir(extra_props: &str) -> String {
    format!(
        r##"{{
        "ir": "0.1.0", "app": "Press", "state": [],
        "tree": {{ "id": 0, "kind": "Column", "props": {{ "padding": 8 }},
            "children": [
                {{ "id": 1, "kind": "Container", "props": {{
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
    let mut r = Renderer::new(240, 200);
    r.render_with_press(&ir, &state, elapsed_ms, press).pixels
}

fn phase(start_tick: i64, touch: Option<(i64, i64)>) -> PressPhase {
    PressPhase {
        node_id: 1,
        press_start_tick: start_tick,
        touch,
    }
}

#[test]
fn no_phase_is_byte_identical_to_render_at() {
    let ir_text = box_ir("");
    let ir = parse_ir(&ir_text).unwrap();
    let state = StateView::from_ir(&ir);
    let mut r = Renderer::new(240, 200);
    let plain = r.render(&ir, &state).pixels;
    let ir2 = parse_ir(&ir_text).unwrap();
    let mut r2 = Renderer::new(240, 200);
    let no_phase = r2
        .render_with_press(&ir2, &StateView::from_ir(&ir2), 0, None)
        .pixels;
    assert_eq!(
        digest_hex(&plain),
        digest_hex(&no_phase),
        "render_with_press(None) equals render"
    );
}

#[test]
fn press_activates_the_node_and_disabled_still_wins() {
    let base = box_ir("");
    let pressed = frame(&base, 0, Some(&phase(0, None)));
    let idle = frame(&base, 0, None);
    assert_ne!(
        digest_hex(&pressed),
        digest_hex(&idle),
        "press changes the frame (Active fill + inset)"
    );

    // Authored `state: disabled` beats the caller press: pressed == disabled idle.
    let disabled = box_ir(", \"state\": \"disabled\"");
    let disabled_pressed = frame(&disabled, 0, Some(&phase(0, None)));
    let disabled_idle = frame(&disabled, 0, None);
    assert_eq!(
        digest_hex(&disabled_pressed),
        digest_hex(&disabled_idle),
        "authored disabled wins over the press phase"
    );
}

#[test]
fn ripple_is_time_driven_and_settles_without_a_wall_clock() {
    let ir_text = box_ir("");
    // Mid-press the ripple disc is painted; after the 220 ms default it is
    // fully faded, leaving only the pressed appearance.
    let mid = frame(&ir_text, 110, Some(&phase(0, None)));
    let after = frame(&ir_text, 500, Some(&phase(0, None)));
    assert_ne!(
        digest_hex(&mid),
        digest_hex(&after),
        "ripple visible at 110ms, gone after 220ms"
    );
    // Past the ripple every later tick is identical: only the caller clock
    // drives the disc, and it has settled.
    let later = frame(&ir_text, 900, Some(&phase(0, None)));
    assert_eq!(
        digest_hex(&after),
        digest_hex(&later),
        "settled frames stable"
    );
    // At progress zero the disc has not begun, so a just-pressed frame matches
    // the settled one: both are the pressed box with no visible ripple.
    let just_pressed = frame(&ir_text, 0, Some(&phase(0, None)));
    assert_eq!(
        digest_hex(&just_pressed),
        digest_hex(&after),
        "no disc at progress zero or past settle"
    );
}

#[test]
fn touch_moves_the_ripple_centre() {
    let ir_text = box_ir("");
    let centred = frame(&ir_text, 110, Some(&phase(0, None)));
    let offset = frame(&ir_text, 110, Some(&phase(0, Some((10, 5)))));
    assert_ne!(
        digest_hex(&centred),
        digest_hex(&offset),
        "an explicit touch point shifts the disc off centre"
    );
}

#[test]
fn default_path_keeps_the_two_counter_goldens() {
    let text = std::fs::read_to_string(COUNTER_PATH).unwrap();
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
