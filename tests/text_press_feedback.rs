//! Pressed state layer for tappable Text nodes. A Text has no authored fill;
//! the low-alpha layer makes Action-style nodes visibly react to press even
//! when bitmap glyphs are suppressed for the system-font display layer.
//! Untappable Text and the no-press path stay byte-identical.

use opurom_render::{digest_hex, parse_ir, PressPhase, Renderer, StateView};

fn action_ir(tappable: bool) -> String {
    let event = if tappable {
        r##", "events": { "tap": { "actions": [] } }"##
    } else {
        r##", "events": {}"##
    };
    format!(
        r##"{{
  "ir": "0.1.0", "app": "ActionPress", "state": [],
  "theme": {{ "mode": "light", "accent": "#097FEB", "radius": 4, "surface": "#FFFFFF", "onSurface": "#000000" }},
  "tree": {{ "id": 0, "kind": "Column", "props": {{ "padding": 8 }},
    "children": [
      {{ "id": 1, "kind": "Text", "props": {{
          "value": "Action 1", "color": "#6200EE", "fontPx": 42, "size": 42
        }}, "children": []{event} }}
    ], "events": {{}} }}
}}"##
    )
}

fn frame(ir_text: &str, suppress: bool, press: Option<&PressPhase>) -> Vec<u8> {
    let ir = parse_ir(ir_text).expect("action IR must parse");
    let state = StateView::from_ir(&ir);
    let mut renderer = Renderer::new(240, 80);
    renderer.set_suppress_text(suppress);
    renderer.render_with_press(&ir, &state, 0, press).pixels
}

fn phase() -> PressPhase {
    PressPhase {
        node_id: 1,
        press_start_tick: 0,
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
fn tap_text_gains_a_visible_press_layer() {
    let ir = action_ir(true);
    let idle = frame(&ir, false, None);
    let pressed = frame(&ir, false, Some(&phase()));
    let changed = changed_pixels(&idle, &pressed);
    assert!(
        changed > 100,
        "a pressed Action must change a visible area, got {changed} pixels"
    );
    assert_eq!(digest_hex(&idle), digest_hex(&frame(&ir, false, None)));
    assert_eq!(
        digest_hex(&pressed),
        digest_hex(&frame(&ir, false, Some(&phase())))
    );
}

#[test]
fn press_layer_works_with_suppressed_bitmap_text() {
    // System-font overlay mode skips draw_text; the state layer is not glyph
    // ink, so the pressed node still paints feedback for the host to overlay.
    let ir = action_ir(true);
    let idle = frame(&ir, true, None);
    let pressed = frame(&ir, true, Some(&phase()));
    assert!(
        changed_pixels(&idle, &pressed) > 100,
        "suppressed text must not suppress the press layer"
    );
}

#[test]
fn release_restores_the_idle_frame_byte_for_byte() {
    let ir_text = action_ir(true);
    let ir = parse_ir(&ir_text).unwrap();
    let state = StateView::from_ir(&ir);
    let mut renderer = Renderer::new(240, 80);
    renderer.set_suppress_text(true);
    let idle = renderer.render_with_press(&ir, &state, 0, None).pixels;
    let _ = renderer
        .render_with_press(&ir, &state, 0, Some(&phase()))
        .pixels;
    let released = renderer.render_with_press(&ir, &state, 0, None).pixels;
    assert_eq!(idle, released, "release must restore the idle bytes");
}

#[test]
fn untappable_text_keeps_its_historical_press_bytes() {
    // No tap prop and no tap event: the caller may still mark it Active, but
    // the new state layer must not paint. This digest pins the old behaviour.
    let ir = action_ir(false);
    let idle = frame(&ir, false, None);
    let pressed = frame(&ir, false, Some(&phase()));
    assert_eq!(
        digest_hex(&pressed),
        "d30a72934e2710e5",
        "untappable Text press output must not change"
    );
    assert_ne!(digest_hex(&idle), digest_hex(&pressed));
}

#[test]
fn no_phase_is_byte_identical_to_render() {
    let ir_text = action_ir(true);
    let ir = parse_ir(&ir_text).unwrap();
    let state = StateView::from_ir(&ir);
    let plain = Renderer::new(240, 80).render(&ir, &state).pixels;
    assert_eq!(plain, frame(&ir_text, false, None));
}
