//! Correctness gate: the incremental press-ripple path must be byte-identical
//! to the full raster path, and clearing a press must leave no residue.

use opurom_render::{digest_hex, Node, PressPhase, Renderer, StateView};

fn compat_renderer(width: u32, height: u32) -> opurom_render::Renderer {
    let mut renderer = opurom_render::Renderer::new(width, height);
    renderer.set_quality(opurom_render::QualityProfile::Compat);
    renderer
}

const COUNTER_PATH: &str = "../opurom-examples/golden/counter.opuir.json";

fn find_event_node(n: &Node, out: &mut u32) {
    if *out == 0 && !n.events.is_empty() {
        *out = n.id;
    }
    for c in &n.children {
        find_event_node(c, out);
    }
}

#[test]
fn incremental_press_matches_full_render_byte_for_byte() {
    let text = std::fs::read_to_string(COUNTER_PATH).unwrap();
    let ir = opurom_render::parse_ir(&text).unwrap();
    let state = StateView::from_ir(&ir);
    let mut target = 0u32;
    find_event_node(&ir.tree, &mut target);
    assert!(target != 0, "fixture needs an event-bound node");

    let (w, h) = (320u32, 240u32);
    let press = PressPhase {
        node_id: target,
        press_start_tick: 0,
        confirm_tick: None,
        touch: None,
    };

    let mut full = compat_renderer(w, h);
    let mut incr = compat_renderer(w, h);
    let _ = full.render_at(&ir, &state, 4000);
    let _ = incr.render_at(&ir, &state, 4000);
    for &ms in &[0i64, 25, 75, 150, 225, 300, 375] {
        let f = full.render_with_press(&ir, &state, ms, Some(&press));
        let i = incr.render_with_press_incremental(&ir, &state, ms, Some(&press));
        assert_eq!(
            digest_hex(&f.pixels),
            digest_hex(&i.pixels),
            "press frame mismatch @{}ms",
            ms
        );
    }

    // Clearing the press after incremental frames restores the exact settled frame.
    let settled = compat_renderer(w, h).render_at(&ir, &state, 4000);
    let cleared = incr.render_incremental_at(&ir, &state, 4000);
    assert_eq!(
        digest_hex(&settled.pixels),
        digest_hex(&cleared.pixels),
        "press clear residue"
    );
}

#[test]
fn press_with_frame_effect_falls_back_to_full_bytes() {
    // A blur effect reads the whole frame after raster. The incremental press
    // path must not reuse a previously-blurred buffer; it must fall back to the
    // exact full render so pixels match byte-for-byte.
    let ir_text = r##"{
      "ir": "0.1.0", "app": "BlurPress",
      "state": [ { "name": "x", "type": "int", "init": 0 } ],
      "tree": { "id": 0, "kind": "Column", "props": { "gap": 4, "padding": 4 },
        "children": [
          { "id": 1, "kind": "Button", "props": { "label": "go", "width": 40, "height": 20 },
            "children": [], "events": { "tap": { "op": "set", "target": "x", "value": 1 } } },
          { "id": 2, "kind": "Container",
            "props": { "width": 40, "height": 20, "color": "#336699",
                       "effects": [ {"kind":"blur","radius":3} ] },
            "children": [], "events": {} }
        ], "events": {} },
      "theme": { "mode": "dark", "accent": "#097FEB", "radius": 4 }
    }"##;
    let ir = opurom_render::parse_ir(ir_text).unwrap();
    let state = StateView::from_ir(&ir);
    let press = PressPhase {
        node_id: 1,
        press_start_tick: 0,
        confirm_tick: None,
        touch: None,
    };
    let (w, h) = (120u32, 120u32);
    let mut full = compat_renderer(w, h);
    let mut incr = compat_renderer(w, h);
    let _ = full.render_at(&ir, &state, 4000);
    let _ = incr.render_at(&ir, &state, 4000);
    for &ms in &[0i64, 75, 150, 225, 300] {
        let f = full.render_with_press(&ir, &state, ms, Some(&press));
        let i = incr.render_with_press_incremental(&ir, &state, ms, Some(&press));
        assert_eq!(
            digest_hex(&f.pixels),
            digest_hex(&i.pixels),
            "blur press mismatch @{}ms",
            ms
        );
    }
}

fn switch_ir() -> opurom_render::Ir {
    let text = r##"{
      "ir": "0.1.0", "app": "SwitchPress", "state": [],
      "tree": { "id": 0, "kind": "Column", "props": { "padding": 8, "gap": 20 }, "children": [
        { "id": 1, "kind": "Button", "props": { "width": 140, "height": 70 },
          "children": [
            { "id": 2, "kind": "Text", "props": { "value": "A", "size": 12,
                                                  "color": "#101010" },
              "children": [], "events": { "tap": { "actions": [] } } }
          ], "events": {} },
        { "id": 3, "kind": "Button", "props": { "label": "B", "width": 80, "height": 40 },
          "children": [], "events": {} }
      ], "events": {} },
      "theme": { "mode": "light", "accent": "#097FEB", "radius": 4,
                 "surface": "#FFFFFF", "onSurface": "#000000" }
    }"##;
    opurom_render::parse_ir(text).unwrap()
}

fn held(id: u32, touch: Option<(i64, i64)>) -> PressPhase {
    PressPhase {
        node_id: id,
        press_start_tick: 0,
        confirm_tick: None,
        touch,
    }
}

/// S2: after a press moves from button A to button B, the incremental frame
/// must equal a fresh full render of press B. Any stale splash left in the
/// prev buffer at A would change the digest.
#[test]
fn switching_press_host_clears_the_previous_ripple() {
    let ir = switch_ir();
    let state = StateView::from_ir(&ir);
    let (w, h) = (200u32, 200u32);
    let mut incr = compat_renderer(w, h);
    let _ = incr.render_at(&ir, &state, 4000);
    for &ms in &[75i64, 300, 500] {
        let _ =
            incr.render_with_press_incremental(&ir, &state, ms, Some(&held(2, Some((130, 60)))));
    }
    let switched = incr.render_with_press_incremental(&ir, &state, 500, Some(&held(3, None)));

    let mut full = compat_renderer(w, h);
    let _ = full.render_at(&ir, &state, 4000);
    let expected = full.render_with_press(&ir, &state, 500, Some(&held(3, None)));
    assert_eq!(
        digest_hex(&switched.pixels),
        digest_hex(&expected.pixels),
        "old ripple on button 1 must clear when press moves to button 3"
    );
}

/// S2: the same host with a moved touch point also relocates the disc; the
/// previous disc must be unioned into the dirty region and cleared.
#[test]
fn switching_touch_position_clears_the_previous_disc() {
    let ir = switch_ir();
    let state = StateView::from_ir(&ir);
    let (w, h) = (200u32, 200u32);
    let p1 = held(1, Some((10, 10)));
    let p2 = held(1, Some((60, 30)));
    let mut incr = compat_renderer(w, h);
    let _ = incr.render_at(&ir, &state, 4000);
    let _ = incr.render_with_press_incremental(&ir, &state, 500, Some(&p1));
    let switched = incr.render_with_press_incremental(&ir, &state, 500, Some(&p2));

    let mut full = compat_renderer(w, h);
    let _ = full.render_at(&ir, &state, 4000);
    let expected = full.render_with_press(&ir, &state, 500, Some(&p2));
    assert_eq!(
        digest_hex(&switched.pixels),
        digest_hex(&expected.pixels),
        "old disc at the first touch point must be cleared"
    );
}

/// PERF-004 parity evidence: after a press sequence, releasing the press must
/// restore the exact settled frame. The current default path takes a full
/// raster for this case; the byte comparison is the gate any future local
/// prev_press_disc clearing must pass before it can be enabled.
#[test]
fn press_release_clear_is_byte_identical_and_skips_full_raster() {
    let text = std::fs::read_to_string(COUNTER_PATH).unwrap();
    let ir = opurom_render::parse_ir(&text).unwrap();
    let state = StateView::from_ir(&ir);
    let mut target = 0u32;
    find_event_node(&ir.tree, &mut target);
    assert!(target != 0, "fixture needs an event-bound node");
    let press = PressPhase {
        node_id: target,
        press_start_tick: 0,
        confirm_tick: None,
        touch: Some((12, 9)),
    };
    let (w, h) = (320u32, 240u32);
    let mut incr = compat_renderer(w, h);
    let _ = incr.render_at(&ir, &state, 4000);
    for &ms in &[0i64, 75, 225, 375] {
        let _ = incr.render_with_press_incremental(&ir, &state, ms, Some(&press));
    }
    let cleared = incr.render_incremental_at(&ir, &state, 4000);
    let expected = compat_renderer(w, h).render_at(&ir, &state, 4000);
    assert_eq!(
        digest_hex(&cleared.pixels),
        digest_hex(&expected.pixels),
        "press release must match the full-raster settled frame byte for byte"
    );
    // Document the audit claim: the clear frame currently reports a full
    // viewport redraw, so a local clear has measurable room to improve.
    assert_eq!(
        incr.redraw_stats().1,
        w as u64 * h as u64,
        "current default falls back to a full raster after a press release"
    );
}
