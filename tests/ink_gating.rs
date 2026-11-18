//! Ink gating: the mobile hit test can land on a Button's Text child, but
//! Flutter paints the splash on the button's Material box. A press whose IR
//! ancestors contain no button-semantic node must never paint a ripple.

use opurom_render::{a11y_tree, digest_hex, parse_ir, PressPhase, Renderer, StateView};

fn compat_renderer(width: u32, height: u32) -> opurom_render::Renderer {
    let mut renderer = Renderer::new(width, height);
    renderer.set_quality(opurom_render::QualityProfile::Compat);
    renderer
}

fn ir() -> String {
    r##"{
  "ir": "0.1.0", "app": "InkGating", "state": [],
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 4, "surface": "#FFFFFF", "onSurface": "#000000" },
  "tree": { "id": 0, "kind": "Column", "props": { "padding": 8, "gap": 8 }, "children": [
    { "id": 1, "kind": "Button", "props": { "color": "#FFFFFF", "width": 120, "height": 60 },
      "children": [
        { "id": 2, "kind": "Text", "props": { "value": "Press", "color": "#101010", "size": 20 },
          "children": [], "events": { "tap": { "actions": [] } } }
      ], "events": {} },
    { "id": 3, "kind": "Text", "props": { "value": "Plain", "color": "#101010", "size": 20 },
      "children": [], "events": { "tap": { "actions": [] } } },
    { "id": 4, "kind": "Container", "props": { "color": "#EEEEEE", "width": 120, "height": 30 },
      "children": [], "events": { "tap": { "actions": [] } } }
  ], "events": {} }
}"##.to_string()
}

fn frame(node_id: u32, at: i64) -> Vec<u8> {
    let text = ir();
    let ir = parse_ir(&text).unwrap();
    let state = StateView::from_ir(&ir);
    let press = PressPhase {
        node_id,
        press_start_tick: 0,
        confirm_tick: None,
        touch: None,
    };
    compat_renderer(240, 200)
        .render_with_press(&ir, &state, at, Some(&press))
        .pixels
}

fn idle() -> Vec<u8> {
    let text = ir();
    let ir = parse_ir(&text).unwrap();
    let state = StateView::from_ir(&ir);
    compat_renderer(240, 200)
        .render_with_press(&ir, &state, 300, None)
        .pixels
}

// Accent splash over the white button is light blue; grey state layers and
// plain dark text are not. This isolates the ripple disc from press styling.
fn is_ripple(p: &[u8]) -> bool {
    p[2] as i32 > p[0] as i32 + 30 && p[2] as i32 > p[1] as i32 + 15 && p[2] > 120
}

fn ripple_count(f: &[u8]) -> usize {
    f.chunks_exact(4).filter(|p| is_ripple(p)).count()
}

#[test]
fn hit_text_child_paints_ripple_on_the_button_host() {
    let child = frame(2, 300);
    let button = frame(1, 300);
    let (nc, nb) = (ripple_count(&child), ripple_count(&button));
    assert!(nb > 3000, "pressing the Button paints a full ripple");
    assert!(nc > 3000, "hitting the Text child must paint a full ripple");
    // Both paint the same Button-sized disc; the small delta is the child's
    // grey Text state layer drawn over part of it.
    assert!((nc as i64 - nb as i64).abs() < 800);
}

/// Flutter gives GestureDetector no press feedback. A tappable Text or Row
/// with no Button ancestor must stay byte-identical to the idle frame.
#[test]
fn plain_text_and_container_have_no_press_feedback() {
    let base = idle();
    assert_eq!(ripple_count(&base), 0);
    assert_eq!(
        digest_hex(&frame(3, 300)),
        digest_hex(&base),
        "plain tappable Text: no tint and no ripple"
    );
    assert_eq!(
        digest_hex(&frame(4, 300)),
        digest_hex(&base),
        "plain tappable Container: no tint and no ripple"
    );
}

/// Host and ripple must agree: pressing a Button's Text child paints both the
/// Active tint and the disc inside the Button box, never on the child.
#[test]
fn ink_host_tint_and_ripple_stay_inside_the_button_box() {
    let text = ir();
    let ir = parse_ir(&text).unwrap();
    let state = StateView::from_ir(&ir);
    let btn = a11y_tree(&ir, &state, 240, 200)
        .into_iter()
        .find(|n| n.id == 1)
        .unwrap()
        .bounds;
    let (bx, by, bw, bh) = (btn[0], btn[1], btn[2], btn[3]);
    let base = idle();
    let pressed = frame(2, 300);
    let (mut inside, mut outside) = (0, 0);
    for (i, (a, b)) in base
        .chunks_exact(4)
        .zip(pressed.chunks_exact(4))
        .enumerate()
    {
        if a == b {
            continue;
        }
        let (px, py) = ((i % 240) as i64, (i / 240) as i64);
        if bx <= px && px < bx + bw && by <= py && py < by + bh {
            inside += 1;
        } else {
            outside += 1;
        }
    }
    assert!(inside > 1000, "the Button box carries tint and ripple");
    assert_eq!(outside, 0, "no feedback outside the Button box");
}

/// The incremental path must apply the same host-only policy as the full path.
#[test]
fn incremental_plain_press_is_byte_identical_to_idle() {
    let text = ir();
    let ir = parse_ir(&text).unwrap();
    let state = StateView::from_ir(&ir);
    let press = PressPhase {
        node_id: 3,
        press_start_tick: 0,
        confirm_tick: None,
        touch: None,
    };
    let mut r = compat_renderer(240, 200);
    let idle_pixels = r.render_at(&ir, &state, 300).pixels;
    let pressed = r
        .render_with_press_incremental(&ir, &state, 300, Some(&press))
        .pixels;
    assert_eq!(digest_hex(&pressed), digest_hex(&idle_pixels));
}
