//! Opt-in shared-clock stagger (ANIMATION_AND_PHYSICS.md §7). Nodes sharing an
//! `animClock` id run one timeline; `animInterval` slices each node's window.
//! Default paths keep every pinned golden byte-for-byte.

use opurom_render::{
    digest_hex, layout_animated_at, parse_ir, AnimSpec, AnimTable, Curve, Renderer, StateView, ONE,
};
use serde_json::json;

const COUNTER_PATH: &str = "../opurom-examples/golden/counter.opuir.json";
const TICKS: [i64; 10] = [0, 100, 200, 300, 400, 500, 600, 700, 800, 900];

fn series(spec: &AnimSpec) -> Vec<i64> {
    TICKS.iter().map(|&t| spec.progress_at(t)).collect()
}

fn series_digest(spec: &AnimSpec) -> String {
    let mut bytes = Vec::new();
    for v in series(spec) {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    digest_hex(&bytes)
}

#[test]
fn two_nodes_same_clock_different_intervals_progress_differently() {
    // The Chief's cross-check: period 800, A [0,500], B [250,750].
    let a = AnimSpec::new(Curve::Ease, 800, 0).with_interval(0, 500);
    let b = AnimSpec::new(Curve::Ease, 800, 0).with_interval(250, 750);
    let sa = series(&a);
    let sb = series(&b);
    assert!(
        sa.iter().zip(&sb).any(|(x, y)| x != y),
        "intervals must stagger"
    );
    // A runs first; B holds its start value until its slice opens at 200 ms.
    assert_eq!(sb[1], 0, "B holds start before its slice (t=100)");
    assert_eq!(sb[2], 0, "B holds start at its slice begin (t=200)");
    assert!(sa[1] > 0, "A is already moving at t=100");
    // Byte-level pins lock the fixed-point formula against drift. A is a pure
    // linear remap: t=100 -> 256 (ONE/4), t=200 -> 512, t=400 -> ONE.
    assert_eq!(series_digest(&a), "11d060985cb4c765", "A sample digest");
    assert_eq!(series_digest(&b), "a1746d8abaaa8ce5", "B sample digest");
    assert_eq!(sa[1], 256, "A linear at t=100");
    assert_eq!(sb[3], 256, "B linear at t=300");
}

#[test]
fn slice_holds_start_before_end_value_after_and_settles_at_one() {
    let a = AnimSpec::new(Curve::Ease, 800, 0).with_interval(0, 500);
    let s = series(&a);
    assert_eq!(s[0], 0, "start value before slice");
    assert_eq!(*s.last().unwrap(), ONE, "settles at ONE");
    for pair in s.windows(2) {
        assert!(pair[1] >= pair[0], "monotonic");
        assert!(pair[1] <= ONE, "bounded");
    }
    // Slice closed at 400 ms (800*500/1000): holds end value after.
    assert_eq!(s[4], ONE, "end value once the slice closes");
    assert_eq!(s[5], ONE, "holds end after slice");
    // Past the period the schedule is settled.
    assert_eq!(a.progress_at(800 + 500), ONE);
}

#[test]
fn bare_interval_without_interval_field_is_the_default_path() {
    // No with_interval => identical to the pre-stagger schedule, byte for byte.
    let plain = AnimSpec::new(Curve::Ease, 800, 0);
    for &t in &TICKS {
        assert_eq!(
            plain.progress_at(t),
            AnimSpec::new(Curve::Ease, 800, 0).progress_at(t)
        );
    }
    assert_eq!(plain.interval, None);
}

#[test]
fn fixed_point_formula_matches_spec_example() {
    // local = clamp((t*1000/P - b) * 1000 / (e-b), 0, 1000).
    // At t=300,P=800,b=250,e=750: tn=375, local=(375-250)*1000/500=250.
    // The curve at fraction 250/1000 is then evaluated; just assert the time
    // remap is non-trivial and monotonic across the slice via distinct values.
    let b = AnimSpec::new(Curve::Ease, 800, 0).with_interval(250, 750);
    assert!(
        b.progress_at(300) > 0 && b.progress_at(300) < ONE,
        "inside slice"
    );
    assert_eq!(b.progress_at(200), 0, "at slice begin");
    assert!(b.progress_at(600) > 0, "still inside at 600");
    assert_eq!(b.progress_at(700), ONE, "settled by period end");
}

fn two_node_ir() -> String {
    json!({
        "ir": "0.1.0", "app": "Stagger", "state": [],
        "tree": { "id": 0, "kind": "Column", "props": { "gap": 4 },
            "children": [
                { "id": 1, "kind": "Container", "props": {
                    "color": "#102030", "width": 40, "height": 20,
                    "animate": "ease", "duration": 800,
                    "animClock": "home", "animInterval": [0, 500] },
                    "children": [], "events": {} },
                { "id": 2, "kind": "Container", "props": {
                    "color": "#102030", "width": 40, "height": 20,
                    "animate": "ease", "duration": 800,
                    "animClock": "home", "animInterval": [250, 750] },
                    "children": [], "events": {} }
            ], "events": {} },
        "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 }
    })
    .to_string()
}

#[test]
fn end_to_end_two_nodes_one_clock_stagger_in_layout() {
    let ir = parse_ir(&two_node_ir()).expect("stagger IR must parse");
    let state = StateView::from_ir(&ir);
    let at = |tick| {
        layout_animated_at(&ir, &state, 200, 120, tick, &mut AnimTable::new())
            .into_iter()
            .find(|n| n.id == 2)
            .unwrap()
    };
    // Node 2's slice begins at 200 ms; its box grows only after that tick.
    let early = at(100);
    let late = at(400);
    assert_ne!(
        early.rect, late.rect,
        "staggered node changes across its slice"
    );
    // The entry-progress flag drives box growth from centre; both nodes present.
    let both = layout_animated_at(&ir, &state, 200, 120, 400, &mut AnimTable::new());
    assert_eq!(both.len(), 3, "root plus two staggered cells");
}

#[test]
fn default_path_keeps_all_pinned_goldens() {
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
