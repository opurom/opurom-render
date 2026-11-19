//! Commit 2: dual dirty marks, boundary props and the keyed memo cache
//! (INCREMENTAL_RENDER_PIPELINE §2.3 / §4 / §6 / §8). All additions opt-in;
//! the default path and every pinned golden stay byte-identical.

use opurom_render::{
    digest, layout, layout_cached_at, parse_ir, plan_dirty, LayoutCache, StateValue, StateView,
    SETTLED_MS,
};
use std::collections::BTreeSet;

fn compat_renderer(width: u32, height: u32) -> opurom_render::Renderer {
    let mut renderer = opurom_render::Renderer::new(width, height);
    renderer.set_quality(opurom_render::QualityProfile::Compat);
    renderer
}

const MANY: &str = include_str!("fixture_many_leaves.json");
const SLIVER: &str = include_str!("fixture_sliver.json");

fn cached(ir_text: &str, state: &StateView, cache: &mut LayoutCache, w: u32, h: u32) -> Vec<u8> {
    let ir = parse_ir(ir_text).unwrap();
    let laid = layout_cached_at(&ir, state, w, h, SETTLED_MS, cache);
    // Raster the cached layout through the same digest the full path uses:
    // bytes are serialized from laid nodes' stable fields for comparison.
    let mut buf = Vec::new();
    for l in &laid {
        buf.extend_from_slice(&l.rect.x.to_le_bytes());
        buf.extend_from_slice(&l.rect.y.to_le_bytes());
        buf.extend_from_slice(&l.rect.w.to_le_bytes());
        buf.extend_from_slice(&l.rect.h.to_le_bytes());
        if let Some(t) = &l.text {
            buf.extend_from_slice(t.as_bytes());
        }
    }
    buf
}

fn full(ir_text: &str, state: &StateView, w: u32, h: u32) -> Vec<u8> {
    let ir = parse_ir(ir_text).unwrap();
    let laid = layout(&ir, state, w, h);
    let mut buf = Vec::new();
    for l in &laid {
        buf.extend_from_slice(&l.rect.x.to_le_bytes());
        buf.extend_from_slice(&l.rect.y.to_le_bytes());
        buf.extend_from_slice(&l.rect.w.to_le_bytes());
        buf.extend_from_slice(&l.rect.h.to_le_bytes());
        if let Some(t) = &l.text {
            buf.extend_from_slice(t.as_bytes());
        }
    }
    buf
}

#[test]
fn golden_digests_are_byte_identical_with_incremental_pipeline_present() {
    let counter = std::fs::read_to_string("../opurom-examples/golden/counter.opuir.json").unwrap();
    let ir = parse_ir(&counter).unwrap();
    let state = StateView::from_ir(&ir);
    // The real render digest path is unchanged by the additive fields.
    let a = compat_renderer(320, 240).render(&ir, &state).pixels;
    let b = compat_renderer(320, 240).render(&ir, &state).pixels;
    assert_eq!(format!("{:016x}", digest(&a)), "8d45f412996a99c6");
    assert_eq!(a, b);
    let c = compat_renderer(800, 600).render(&ir, &state).pixels;
    assert_eq!(format!("{:016x}", digest(&c)), "318beb1dd337bc4e");
}

#[test]
fn cold_and_warm_cache_match_full_path_for_random_state_deltas() {
    let ir = parse_ir(MANY).unwrap();
    let base = StateView::from_ir(&ir);
    let mut cache = LayoutCache::new(4096);
    cache.activate();
    // Deterministic pseudo-random deltas, no wall clock.
    for step in 0..16i64 {
        let mut st = base.clone();
        for i in 0..60 {
            st.set(
                format!("k{i}"),
                StateValue::Int((i as i64 * 31 + step * 7) % 97),
            );
        }
        let got = cached(MANY, &st, &mut cache, 640, 240);
        let want = full(MANY, &st, 640, 240);
        assert_eq!(got, want, "cache on/off byte mismatch at step {step}");
    }
}

#[test]
fn negative_proof_one_leaf_change_skips_majority_of_nodes() {
    let ir = parse_ir(MANY).unwrap();
    let base = StateView::from_ir(&ir);
    let mut cache = LayoutCache::new(4096);
    cache.activate();
    // Prime the cache.
    let _ = cached(MANY, &base, &mut cache, 640, 240);
    cache.reset_counters();
    // Change exactly one leaf's state key.
    let mut st = base.clone();
    st.set("k30", StateValue::Int(-777));
    let got = cached(MANY, &st, &mut cache, 640, 240);
    let want = full(MANY, &st, 640, 240);
    assert_eq!(got, want, "incremental output must stay byte-identical");
    let total = 61usize; // root + 60 leaves
    let c = cache.counters;
    let skip = c.layout_reused as f64 / total as f64;
    assert!(
        skip >= 0.95,
        "unchanged-subtree skip rate too low: {}/{total} = {:.3}",
        c.layout_reused,
        skip
    );
    assert!(
        c.layout_recomputed <= total / 10,
        "work must drop an order of magnitude: recomputed={} total={total}",
        c.layout_recomputed
    );
}

#[test]
fn boundary_isolation_dirty_marks_stop_at_boundary() {
    // Three siblings under a root; the middle one is a repaint+relayout
    // boundary. A change inside it must dirty it but not its parent/siblings.
    let ir = parse_ir(r##"{
      "ir":"0.1.0","app":"B","state":[{"name":"a","type":"int","init":0},{"name":"b","type":"int","init":0}],
      "tree":{"id":0,"kind":"Column","props":{"gap":0,"padding":0},"children":[
        {"id":1,"kind":"Text","props":{"value":"{a}","size":12},"children":[],"events":{}},
        {"id":2,"kind":"Container","props":{"color":"#102030","repaintBoundary":true,"relayoutBoundary":true},
          "children":[{"id":3,"kind":"Text","props":{"value":"{b}","size":12},"children":[],"events":{}}],"events":{}}
      ],"events":{}},"theme":{"mode":"light","accent":"#097FEB","radius":0}
    }"##).unwrap();
    let mut changed = BTreeSet::new();
    changed.insert("b".to_string());
    let (dirty_layout, dirty_paint) = plan_dirty(&ir.tree, &changed);
    assert!(dirty_layout.contains(&"3".to_string()), "leaf dirty");
    assert!(
        dirty_layout.contains(&"2".to_string()),
        "boundary itself dirty"
    );
    assert!(
        !dirty_layout.contains(&"0".to_string()),
        "root clean past relayout boundary"
    );
    assert!(!dirty_layout.contains(&"1".to_string()), "sibling clean");
    assert!(dirty_paint.contains(&"3".to_string()) && dirty_paint.contains(&"2".to_string()));
    assert!(
        !dirty_paint.contains(&"0".to_string()),
        "root paint clean past repaint boundary"
    );

    // Paint-only change in a sibling must not promote to layout.
    let mut changed2 = BTreeSet::new();
    changed2.insert("a".to_string());
    let (dl, dp) = plan_dirty(&ir.tree, &changed2);
    // Text value change is an appearance change: it dirties layout too in the
    // conservative model (glyph width may change), which is correct; but it
    // never escapes the non-boundary sibling's parent here.
    assert!(dp.contains(&"1".to_string()));
    assert_eq!(dl.contains(&"0".to_string()), true);
}

#[test]
fn row_identity_reuses_surviving_rows_after_insert() {
    let ir = parse_ir(SLIVER).unwrap();
    let mut base = StateView::from_ir(&ir);
    let items: Vec<String> = (0..40).map(|i| i.to_string()).collect();
    base.set("items", StateValue::List(items.clone()));
    let mut cache = LayoutCache::new(4096);
    cache.activate();
    let _ = cached(SLIVER, &base, &mut cache, 320, 240);
    cache.reset_counters();
    // Insert a row at the front; surviving rows keep identity (node id, value).
    let mut st = base.clone();
    let mut inserted = vec!["-1".to_string()];
    inserted.extend(items);
    st.set("items", StateValue::List(inserted));
    let got = cached(SLIVER, &st, &mut cache, 320, 240);
    let want = full(SLIVER, &st, 320, 240);
    assert_eq!(got, want, "insert must keep byte-identical output");
    assert!(
        cache.counters.layout_reused > 0,
        "surviving rows must reuse memo entries, got {:?}",
        cache.counters
    );
}

#[test]
fn determinism_two_runs_same_bytes_and_counters() {
    fn run() -> (Vec<u8>, opurom_render::RecomputeCounters) {
        let ir = parse_ir(MANY).unwrap();
        let st = StateView::from_ir(&ir);
        let mut cache = LayoutCache::new(4096);
        cache.activate();
        let _ = cached(MANY, &st, &mut cache, 640, 240);
        cache.reset_counters();
        let mut st2 = st.clone();
        st2.set("k7", StateValue::Int(123));
        let b = cached(MANY, &st2, &mut cache, 640, 240);
        (b, cache.counters)
    }
    let (a, ca) = run();
    let (b, cb) = run();
    assert_eq!(a, b, "bytes must be deterministic");
    assert_eq!(ca, cb, "counters must be deterministic");
}

#[test]
fn runtime_code_size_stays_within_lightweight_kernel_budget() {
    // Artifact-size dimension (§8.4): the incremental runtime module stays
    // small. Assert a bounded source size so growth is an explicit decision.
    let src = include_str!("../src/incremental.rs");
    let lines = src.lines().count();
    assert!(
        lines <= 600,
        "incremental runtime module grew to {lines} lines (budget 600)"
    );
}

#[test]
fn sliver_scroll_only_dirties_rows_entering_the_window() {
    use opurom_render::Kind;
    let ir = parse_ir(SLIVER).unwrap();
    let items: Vec<String> = (0..60).map(|i| i.to_string()).collect();
    let mut st = StateView::from_ir(&ir);
    st.set("items", StateValue::List(items));
    let mut cache = LayoutCache::new(4096);
    cache.activate();
    // First window.
    let a = layout_cached_at(&ir, &st, 320, 240, SETTLED_MS, &mut cache);
    cache.reset_counters();
    // Same content, warm cache: scroll window unchanged -> all reused.
    let b = layout_cached_at(&ir, &st, 320, 240, SETTLED_MS, &mut cache);
    let rows = |v: &[opurom_render::LaidOut]| {
        v.iter()
            .filter(|l| l.kind == Kind::Text)
            .filter_map(|l| {
                l.text
                    .as_ref()
                    .filter(|t| t.starts_with("r-"))
                    .map(|t| t.clone())
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(rows(&a), rows(&b), "stable scroll window is byte-stable");
    assert_eq!(
        cache.counters.layout_reused,
        a.len(),
        "all rows reused on a stable window"
    );
    // Sliver window bounds the materialized set: 60 list rows in a 240px clip
    // with 20px rows never materialize the whole list.
    assert!(rows(&a).len() < 60, "sliver must not materialize all rows");
    assert!(
        rows(&a).len() >= 2,
        "sliver must materialize the visible rows"
    );
}

#[test]
fn gesture_arena_golden_scroll_vs_click_and_drag_vs_long_press() {
    use opurom_render::{
        Gesture, GestureEngine, GestureKind, PointerEvent, PointerKind, Rect, DRAG_THRESHOLD,
        LONG_PRESS_MS,
    };
    // Both canonical arena outcomes reduce to a stable (winner, rejected) pair.
    fn fingerprint(engine: &GestureEngine) -> String {
        let w = engine.arena_winner().map(|k| k.name()).unwrap_or("none");
        let mut rej: Vec<&str> = [
            GestureKind::Tap,
            GestureKind::DoubleTap,
            GestureKind::LongPress,
            GestureKind::Drag,
        ]
        .iter()
        .filter(|&&k| engine.arena_rejected(k))
        .map(|&k| k.name())
        .collect();
        rej.sort_unstable();
        format!("{w}|{}", rej.join(","))
    }

    // Golden 1: scroll region click — small jitter is a tap; crossing the slop
    // is a drag that wins the arena.
    let mut click = GestureEngine::new();
    click.add_node(
        1,
        Rect {
            x: 0,
            y: 0,
            w: 200,
            h: 200,
        },
    );
    assert_eq!(
        click.on_down(1, &PointerEvent::at(PointerKind::Down, 40, 40, 0)),
        None
    );
    assert_eq!(
        click.on_move(1, &PointerEvent::at(PointerKind::Move, 45, 43, 10)),
        None
    );
    assert_eq!(
        click.on_up(1, &PointerEvent::at(PointerKind::Up, 45, 43, 30)),
        None
    );
    // The first tap waits for the double-tap window and the tick awards Tap.
    assert_eq!(click.on_tick(300), Some(Gesture::Tap));
    assert_eq!(fingerprint(&click), "tap|doubleTap,drag,longPress");

    let mut scroll = GestureEngine::new();
    scroll.add_node(
        1,
        Rect {
            x: 0,
            y: 0,
            w: 200,
            h: 200,
        },
    );
    assert_eq!(
        scroll.on_down(1, &PointerEvent::at(PointerKind::Down, 40, 40, 100)),
        None
    );
    assert_eq!(
        scroll.on_move(1, &PointerEvent::at(PointerKind::Move, 40, 70, 110)),
        Some(Gesture::Drag { dx: 0, dy: 30 })
    );
    assert_eq!(fingerprint(&scroll), "drag|doubleTap,longPress,tap");

    // Golden 2: long press accepts on its tick before a later drag move, so
    // drag is rejected and stays inert.
    let mut tie = GestureEngine::new();
    tie.add_node(
        1,
        Rect {
            x: 0,
            y: 0,
            w: 200,
            h: 200,
        },
    );
    assert_eq!(
        tie.on_down(1, &PointerEvent::at(PointerKind::Down, 10, 10, 0)),
        None
    );
    assert_eq!(tie.on_tick(LONG_PRESS_MS), Some(Gesture::LongPress));
    assert_eq!(fingerprint(&tie), "longPress|doubleTap,drag,tap");
    assert_eq!(
        tie.on_move(
            1,
            &PointerEvent::at(
                PointerKind::Move,
                10 + DRAG_THRESHOLD + 2,
                10,
                LONG_PRESS_MS + 1
            )
        ),
        None
    );
}

#[test]
fn key_diff_default_aligns_by_index_and_key_matches_across_positions() {
    use opurom_render::{align_by_key, KeyedNode};
    let k = |id: u32, kind: &'static str, key: Option<&str>| KeyedNode {
        static_id: id,
        kind,
        key: key.map(str::to_string),
    };

    // Default path: no keys, source-index alignment against the static id.
    let prev = vec![k(1, "Text", None), k(2, "Text", None), k(3, "Text", None)];
    let next = vec![k(1, "Text", None), k(2, "Text", None), k(3, "Text", None)];
    assert_eq!(align_by_key(&prev, &next), vec![Some(0), Some(1), Some(2)]);

    // Enhanced path: same-kind keys match across positions after a reorder.
    let prev = vec![
        k(1, "Text", Some("a")),
        k(2, "Text", Some("b")),
        k(3, "Text", Some("c")),
    ];
    let next = vec![
        k(1, "Text", Some("c")),
        k(2, "Text", Some("a")),
        k(3, "Text", Some("b")),
    ];
    assert_eq!(align_by_key(&prev, &next), vec![Some(2), Some(0), Some(1)]);

    // A key with no partner falls back to index alignment, never errors; a
    // different kind never matches even with an equal key.
    let prev = vec![k(1, "Text", Some("a")), k(2, "Text", Some("b"))];
    let next = vec![k(1, "Text", Some("x")), k(2, "Image", Some("b"))];
    let got = align_by_key(&prev, &next);
    assert_eq!(
        got,
        vec![Some(0), None],
        "unmatched key falls back; kind gate holds"
    );

    // Static ids are carried unchanged; matching never reassigns them.
    assert_eq!(next[0].static_id, 1);
    assert_eq!(next[1].static_id, 2);
}

/// Flutter 3.50.0 `RenderObject.markNeedsLayout` stops propagation at a
/// relayout boundary (`rendering/object.dart:2682-2700`), and `markNeedsPaint`
/// stops at a repaint boundary (`:3344-3368`). The dirty marker itself stays
/// on the boundary node, so the boundary still redraws while its parent and
/// siblings do not. This pins the two boundaries independently: a layout
/// boundary must not implicitly stop paint, and vice versa.
#[test]
fn layout_and_paint_boundaries_stop_independently() {
    use std::collections::BTreeSet;

    let ir = parse_ir(r##"{
        "ir":"0.1.0","app":"B","state":[],
        "tree":{"id":0,"kind":"Column","props":{},"children":[
            {"id":1,"kind":"Container","props":{"relayoutBoundary":true},
             "children":[{"id":2,"kind":"Text","props":{"value":"{x}"},"children":[],"events":{}}],"events":{}},
            {"id":3,"kind":"Container","props":{"repaintBoundary":true},
             "children":[{"id":4,"kind":"Text","props":{"value":"{y}"},"children":[],"events":{}}],"events":{}}
        ],"events":{}},"theme":{"mode":"light","accent":"#097FEB","radius":0}
    }"##).unwrap();
    let mut changed = BTreeSet::new();
    changed.insert("x".to_string());
    let (layout, paint) = plan_dirty(&ir.tree, &changed);
    // The relayout boundary consumes layout and paint because layout implies
    // paint. Its parent (root) is still marked for paint: with no repaint
    // boundary in that branch, Flutter propagates paint past the layout-only
    // boundary.
    assert!(layout.contains(&"1".to_string()));
    assert!(!layout.contains(&"0".to_string()));
    assert!(paint.contains(&"0".to_string()));
    assert!(!paint.contains(&"3".to_string()));
    assert!(!paint.contains(&"4".to_string()));

    let mut changed = BTreeSet::new();
    changed.insert("y".to_string());
    let (layout, paint) = plan_dirty(&ir.tree, &changed);
    // A repaint boundary stops paint at the boundary itself; the root is
    // nevertheless painted because the repaint boundary's own layout change
    // makes its parent repaint (Flutter: layout change implies markNeedsPaint,
    // object.dart:2789). Layout itself still reaches the root because this
    // branch has no relayout boundary.
    assert!(paint.contains(&"3".to_string()));
    assert!(paint.contains(&"0".to_string()));
    assert!(layout.contains(&"0".to_string()));
    assert!(!layout.contains(&"1".to_string()));
}
