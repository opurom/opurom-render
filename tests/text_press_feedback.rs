//! Press feedback follows the ink host, not the hit node. Flutter gives a
//! GestureDetector no pressed visual; only an InkWell/Button reacts. A plain
//! tappable Text with no Button ancestor stays byte-identical under press,
//! including when bitmap text is suppressed for the system-font overlay.

use opurom_render::{digest_hex, parse_ir, PressPhase, Renderer, StateView};

fn compat_renderer(width: u32, height: u32) -> opurom_render::Renderer {
    let mut renderer = opurom_render::Renderer::new(width, height);
    renderer.set_quality(opurom_render::QualityProfile::Compat);
    renderer
}

const THEME: &str = r##""theme": { "mode": "light", "accent": "#097FEB", "radius": 4, "surface": "#FFFFFF", "onSurface": "#000000" }"##;

fn action_ir(tappable: bool, in_button: bool) -> String {
    let event = if tappable {
        r##", "events": { "tap": { "actions": [] } }"##
    } else {
        r##", "events": {}"##
    };
    let text_id = if in_button { 2 } else { 1 };
    let text = format!(
        r##"{{ "id": {text_id}, "kind": "Text", "props": {{ "value": "Action 1", "color": "#6200EE", "fontPx": 42, "size": 42 }}, "children": []{event} }}"##
    );
    let child = if in_button {
        format!(
            r##"{{ "id": 1, "kind": "Button", "props": {{ "width": 160, "height": 80 }}, "children": [{text}], "events": {{}} }}"##
        )
    } else {
        text
    };
    format!(
        r##"{{
  "ir": "0.1.0", "app": "ActionPress", "state": [],
  {THEME},
  "tree": {{ "id": 0, "kind": "Column", "props": {{ "padding": 8 }},
    "children": [{child}], "events": {{}} }}
}}"##
    )
}

fn frame(ir_text: &str, suppress: bool, press: Option<&PressPhase>) -> Vec<u8> {
    let ir = parse_ir(ir_text).expect("action IR must parse");
    let state = StateView::from_ir(&ir);
    let mut renderer = compat_renderer(240, 120);
    renderer.set_suppress_text(suppress);
    renderer.render_with_press(&ir, &state, 300, press).pixels
}

fn phase() -> PressPhase {
    PressPhase {
        node_id: 2,
        press_start_tick: 0,
        confirm_tick: None,
        touch: None,
    }
}

fn changed_pixels(a: &[u8], b: &[u8]) -> usize {
    a.chunks_exact(4)
        .zip(b.chunks_exact(4))
        .filter(|(x, y)| x != y)
        .count()
}

#[test]
fn tappable_text_without_button_host_has_no_press_feedback() {
    let ir = action_ir(true, false);
    let idle = frame(&ir, false, None);
    assert_eq!(
        digest_hex(&frame(&ir, false, Some(&phase()))),
        digest_hex(&idle),
        "a tappable Text is a GestureDetector: no tint, no ripple"
    );
    assert_eq!(
        digest_hex(&frame(&ir, true, Some(&phase()))),
        digest_hex(&frame(&ir, true, None)),
        "system-font overlay mode must not resurrect the press layer"
    );
}

#[test]
fn untappable_text_press_is_a_no_op() {
    let ir = action_ir(false, false);
    assert_eq!(frame(&ir, false, Some(&phase())), frame(&ir, false, None));
}

#[test]
fn button_host_keeps_feedback_when_bitmap_text_is_suppressed() {
    let ir = action_ir(true, true);
    let idle = frame(&ir, true, None);
    let pressed = frame(&ir, true, Some(&phase()));
    assert!(
        changed_pixels(&idle, &pressed) > 100,
        "the Button host paints tint and ripple even with text suppressed"
    );
}

#[test]
fn release_restores_the_idle_frame_byte_for_byte() {
    let ir = action_ir(true, true);
    let ir_parsed = parse_ir(&ir).unwrap();
    let state = StateView::from_ir(&ir_parsed);
    let mut renderer = compat_renderer(240, 120);
    renderer.set_suppress_text(true);
    let idle = renderer
        .render_with_press(&ir_parsed, &state, 300, None)
        .pixels;
    let _ = renderer
        .render_with_press(&ir_parsed, &state, 300, Some(&phase()))
        .pixels;
    let released = renderer
        .render_with_press(&ir_parsed, &state, 300, None)
        .pixels;
    assert_eq!(idle, released, "release must restore the idle bytes");
}

#[test]
fn no_phase_is_byte_identical_to_render() {
    let ir_text = action_ir(true, true);
    let ir = parse_ir(&ir_text).unwrap();
    let state = StateView::from_ir(&ir);
    let plain = compat_renderer(240, 120).render(&ir, &state).pixels;
    assert_eq!(plain, frame(&ir_text, false, None));
}
