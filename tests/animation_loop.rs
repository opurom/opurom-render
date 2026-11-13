//! Deterministic animated fixture for the cross-backend multi-frame loop.
//!
//! The fixture uses the two entry-animation curves (`ease` and `spring`) so a
//! bounded sequence of distinct ticks produces distinct frames and distinct
//! display lists. Everything is integer-only and tick-driven: no wall clock.

use opurom_render::{digest_hex, parse_ir, Renderer, StateView};
use std::fs;

/// Ticks used by the GPU animation loop; settled at the last one.
pub const TICKS: [i64; 4] = [0, 96, 192, 4000];

fn fixture() -> String {
    fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/animation-loop.opuir.json"
    ))
    .expect("the animated fixture must exist")
}

fn frames() -> Vec<(String, String)> {
    let ir = parse_ir(&fixture()).unwrap();
    let state = StateView::from_ir(&ir);
    let renderer = Renderer::new(200, 150);
    TICKS
        .iter()
        .map(|&tick| {
            let pixels = renderer.pixels_at(&ir, &state, tick);
            let commands = renderer
                .display_list_at(&ir, &state, tick)
                .to_json()
                .unwrap();
            (digest_hex(&pixels), commands)
        })
        .collect()
}

#[test]
fn animated_fixture_is_valid_and_uses_both_curves() {
    let text = fixture();
    assert!(text.contains("\"animate\": \"ease\""));
    assert!(text.contains("\"animate\": \"spring\""));
    let ir = parse_ir(&text).unwrap();
    assert_eq!(ir.state.len(), 0);
    assert_eq!(ir.tree.children.len(), 2);
}

#[test]
fn every_tick_produces_a_distinct_frame_and_display_list() {
    let sequence = frames();
    assert_eq!(sequence.len(), TICKS.len());
    for (index, (digest, commands)) in sequence.iter().enumerate() {
        for (other, (other_digest, other_commands)) in sequence.iter().enumerate() {
            if index == other {
                continue;
            }
            assert_ne!(digest, other_digest, "frames at two ticks must differ");
            assert_ne!(
                commands, other_commands,
                "display lists at two ticks must differ"
            );
        }
    }
}

#[test]
fn tick_sequences_repeat_byte_for_byte() {
    assert_eq!(frames(), frames());
}

#[test]
fn display_lists_are_non_empty_at_every_tick() {
    for (_, commands) in frames() {
        assert!(commands.starts_with("{\"version\":1"));
        assert!(commands.contains("\"kind\":\"solid\""));
    }
}
