//! Lazy sliver window (DYNAMIC_LISTS.md): a `For` inside a `Scroll` clips rows
//! to the viewport plus a fixed overscan, while keeping row geometry exact.

use opurom_render::{layout, parse_ir, Kind, StateValue, StateView};
use serde_json::json;

const N: usize = 10_000;
const ROW_H: i64 = 24;
const CLIP_H: i64 = 240;
const OVERSCAN: usize = 2;

/// Scroll viewport of `CLIP_H` px holding one For of `N` fixed-height rows.
/// `lead` adds a spacer of that height before the Scroll so the clip offset can
/// be moved without changing row geometry.
fn sliver_ir(lead: i64) -> String {
    json!({
        "ir": "0.1.0", "app": "Sliver", "state": [ { "name": "items", "type": "list", "init": [] } ],
        "tree": { "id": 0, "kind": "Column", "props": { "gap": 0, "padding": 0 },
            "children": [ { "id": 1, "kind": "Scroll",
                "props": { "width": 320, "height": CLIP_H },
                "children": [
                    { "id": 2, "kind": "Container",
                        "props": { "width": 320, "height": lead, "color": "#000000" },
                        "children": [], "events": {} },
                    { "id": 3, "kind": "For",
                        "props": { "source": "items", "item": "p", "gap": 0 },
                        "children": [ { "id": 4, "kind": "Text",
                            "props": { "value": "row-{p}", "size": 12, "height": ROW_H },
                            "children": [], "events": {} } ],
                        "events": {} }
                ], "events": {} }
            ], "events": {} },
        "theme": { "mode": "light", "accent": "#097FEB", "radius": 0 }
    })
    .to_string()
}
fn render(ir_text: &str, n: usize) -> Vec<(i64, String)> {
    let ir = parse_ir(ir_text).unwrap();
    let mut state = StateView::from_ir(&ir);
    state.set(
        "items",
        StateValue::List((0..n).map(|i| i.to_string()).collect()),
    );
    layout(&ir, &state, 320, 400)
        .iter()
        .filter(|l| l.kind == Kind::Text)
        .filter_map(|l| {
            l.text
                .as_ref()
                .filter(|t| t.starts_with("row-"))
                .map(|t| (l.rect.y, t.clone()))
        })
        .collect()
}

/// A For laid out with no clip at all: the reference full path (every row).
fn render_no_clip(n: usize) -> Vec<(i64, String)> {
    let text = json!({
        "ir": "0.1.0", "app": "Sliver", "state": [ { "name": "items", "type": "list", "init": [] } ],
        "tree": { "id": 0, "kind": "For",
            "props": { "source": "items", "item": "p", "gap": 0 },
            "children": [ { "id": 1, "kind": "Text",
                "props": { "value": "row-{p}", "size": 12, "height": ROW_H },
                "children": [], "events": {} } ],
            "events": {} },
        "theme": { "mode": "light", "accent": "#097FEB", "radius": 0 }
    })
    .to_string();
    render(&text, n)
}

#[test]
fn ten_thousand_rows_materialize_only_window_plus_overscan() {
    let rows = render(&sliver_ir(0), N);
    // Visible window is ~CLIP_H/ROW_H = 10 rows plus 2 overscan on each side.
    assert!(
        rows.len() <= (CLIP_H / ROW_H + 2 * OVERSCAN as i64) as usize + 1,
        "expected a bounded window, got {} rows",
        rows.len()
    );
    assert!(
        rows.len() < N / 100,
        "node count must be far below full list"
    );
}

#[test]
fn visible_row_y_offsets_match_the_full_path() {
    let full = render_no_clip(12);
    let clipped = render(&sliver_ir(0), 12);
    // Short list fits the clip: every row is materialized at the same y.
    assert_eq!(full, clipped, "no-clip geometry must be byte-identical");
}

#[test]
fn different_clip_offsets_hit_the_correct_window() {
    let at_top = render(&sliver_ir(0), N);
    let lowered = render(&sliver_ir(120), N);
    assert!(!at_top.is_empty() && !lowered.is_empty());
    // Lowering the For start inside the fixed clip shrinks the intersecting
    // window: fewer rows are visible, so the materialized set changes.
    assert!(
        lowered.len() < at_top.len(),
        "clip offset must select a different window ({} vs {})",
        lowered.len(),
        at_top.len()
    );
    assert_eq!(at_top.last().unwrap().1, "row-11");
    assert_eq!(lowered.last().unwrap().1, "row-6");
    // Both windows keep the same y spacing.
    let step = |v: &[(i64, String)]| v[1].0 - v[0].0;
    assert_eq!(step(&at_top), step(&lowered));
}

#[test]
fn no_clip_and_short_lists_are_byte_identical() {
    // With a list shorter than the window, every row is inside; the clipped
    // path must equal the no-clip path exactly (ids, texts, y).
    for n in [0usize, 1, 5, 10] {
        assert_eq!(render_no_clip(n), render(&sliver_ir(0), n), "n={n}");
    }
}

#[test]
fn negative_probe_row_count_really_drops() {
    let full = render_no_clip(N);
    let clipped = render(&sliver_ir(0), N);
    assert_eq!(full.len(), N, "full path materializes every row");
    assert!(
        clipped.len() < full.len() / 100,
        "lazy path must materialize far fewer rows: {} vs {}",
        clipped.len(),
        full.len()
    );
}
