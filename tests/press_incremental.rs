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
