//! Determinism gates for animation and press timelines. The renderer keeps a
//! per-instance implicit-animation table; two renderers driven by the same
//! call sequence must produce byte-identical frames, and a display-list query
//! must not mutate that state.

use opurom_render::{digest_hex, parse_ir, PressPhase, Renderer, StateView, SETTLED_MS};
use serde_json::json;

fn compat_renderer(width: u32, height: u32) -> opurom_render::Renderer {
    let mut renderer = opurom_render::Renderer::new(width, height);
    renderer.set_quality(opurom_render::QualityProfile::Compat);
    renderer
}

const ANIMATED: &str = r##"{
  "ir": "0.1.0", "app": "Timing", "state": [],
  "tree": { "id": 0, "kind": "Column", "props": { "gap": 4 },
    "children": [
      { "id": 1, "kind": "Container", "props": { "width": 80, "height": 60, "color": "#102030", "opacity": 0.6,
          "animate": "ease", "duration": 200, "delay": 20 }, "children": [], "events": {} },
      { "id": 2, "kind": "Container", "props": { "width": 60, "height": 40, "color": "#405060",
          "animate": "spring", "duration": 200 }, "children": [], "events": {} }
    ], "events": {} },
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 }
}"##;

fn press_ir() -> (opurom_render::Ir, StateView, PressPhase) {
    let text = std::fs::read_to_string("../opurom-examples/golden/counter.opuir.json").unwrap();
    let ir = parse_ir(&text).unwrap();
    let mut target = 0u32;
    fn find(n: &opurom_render::Node, out: &mut u32) {
        if *out == 0 && !n.events.is_empty() {
            *out = n.id;
        }
        for c in &n.children {
            find(c, out);
        }
    }
    find(&ir.tree, &mut target);
    assert!(target != 0);
    let state = StateView::from_ir(&ir);
    (
        ir,
        state,
        PressPhase {
            node_id: target,
            press_start_tick: 0,
            confirm_tick: None,
            touch: Some((12, 9)),
        },
    )
}

#[test]
fn identical_animation_timelines_have_identical_digests() {
    let ir = parse_ir(ANIMATED).unwrap();
    let state = StateView::from_ir(&ir);
    let ticks = [SETTLED_MS, 0, 40, 120, 260, 500];
    let run = |probe: bool| {
        let mut r = compat_renderer(200, 120);
        let mut digests = Vec::new();
        for &t in &ticks {
            if probe {
                // A display-list query interleaved between frames must be pure.
                let _ = r.display_list_at(&ir, &state, t).to_json().unwrap();
            }
            digests.push(digest_hex(&r.render_at(&ir, &state, t).pixels));
        }
        digests
    };
    assert_eq!(run(false), run(false));
    assert_eq!(run(false), run(true));
}

#[test]
fn interleaved_press_and_idle_frames_are_order_stable() {
    let (ir, state, press) = press_ir();
    let mut timeline = compat_renderer(320, 240);
    let _ = timeline.render_at(&ir, &state, SETTLED_MS);
    for &ms in &[0i64, 75, 225] {
        let _ = timeline.render_with_press_incremental(&ir, &state, ms, Some(&press));
    }
    let released = timeline.render_incremental_at(&ir, &state, SETTLED_MS);

    let mut control = compat_renderer(320, 240);
    let _ = control.render_at(&ir, &state, SETTLED_MS);
    for &ms in &[0i64, 75, 225] {
        let _ = control.render_with_press(&ir, &state, ms, Some(&press));
    }
    let expected = control.render_at(&ir, &state, SETTLED_MS);
    assert_eq!(digest_hex(&released.pixels), digest_hex(&expected.pixels));
}

#[test]
fn malformed_animation_values_do_not_change_the_default_frame() {
    let mut doc: serde_json::Value = serde_json::from_str(ANIMATED).unwrap();
    doc["tree"]["children"][0]["props"]["duration"] = json!(-1);
    let bad = doc.to_string();
    assert!(parse_ir(&bad).is_err());
    let ir = parse_ir(ANIMATED).unwrap();
    let a = compat_renderer(200, 120).render(&ir, &StateView::from_ir(&ir));
    let b = compat_renderer(200, 120).render(&ir, &StateView::from_ir(&ir));
    assert_eq!(digest_hex(&a.pixels), digest_hex(&b.pixels));
}
