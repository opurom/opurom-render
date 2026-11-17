//! Correctness gate: the incremental press-ripple path must be byte-identical
//! to the full raster path, and clearing a press must leave no residue.

use opurom_render::{digest_hex, Node, PressPhase, Renderer, StateView};

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

    let mut full = Renderer::new(w, h);
    let mut incr = Renderer::new(w, h);
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
    let settled = Renderer::new(w, h).render_at(&ir, &state, 4000);
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
    let mut full = Renderer::new(w, h);
    let mut incr = Renderer::new(w, h);
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
