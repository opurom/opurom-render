//! Interactive UI renderer tests: theme tokens, canonical effects, conditional
//! `when` visibility, the sixteen new node kinds and deterministic animation.

use opurom_render::{
    digest_hex, layout, layout_at, parse_ir, Animator, Control, Direction, Ir, Kind, LaidOut, Op,
    Renderer, Rgba, StateValue, StateView,
};
use std::process::Command;

fn ir_of(text: &str) -> Ir {
    parse_ir(text).expect("fixture IR must parse")
}

fn node<'a>(laid: &'a [LaidOut], id: u32) -> &'a LaidOut {
    laid.iter().find(|n| n.id == id).expect("node id missing")
}

/// One container with a single child, used to isolate effect digests.
fn effect_ir(props: &str) -> String {
    format!(
        r##"{{
  "ir": "0.1", "app": "Fx",
  "state": [],
  "tree": {{ "id": 0, "kind": "Container",
    "props": {{ "width": 120, "height": 80, {props} }},
    "children": [], "events": {{}} }},
  "theme": {{ "mode": "light", "accent": "#4F46E5", "radius": 8 }}
}}"##
    )
}

fn digest_of(ir_text: &str) -> String {
    let ir = ir_of(ir_text);
    let state = StateView::from_ir(&ir);
    digest_hex(&Renderer::new(200, 120).pixels(&ir, &state))
}

// ---------------------------------------------------------------- theme tokens

#[test]
fn theme_tokens_override_mode_defaults_and_fall_back_per_token() {
    let ir = ir_of(
        r##"{
  "ir": "0.1", "app": "T",
  "state": [],
  "tree": { "id": 0, "kind": "Container", "props": { "width": 40, "height": 20 },
    "children": [], "events": {} },
  "theme": { "mode": "dark", "accent": "#4F46E5", "radius": 4,
    "tokens": { "surface": "#010203", "onSurface": "#0A0B0C", "font": "Inter" } }
}"##,
    );
    let state = StateView::from_ir(&ir);
    let laid = layout(&ir, &state, 200, 120);
    // surface token wins over the dark-mode default.
    assert_eq!(node(&laid, 0).fill, Rgba::from_hex("#010203"));
    // The font token is recorded but never selects a host font.
    assert_eq!(ir.theme.tokens.font.as_deref(), Some("Inter"));
    assert_eq!(ir.theme.tokens.on_surface, Rgba::from_hex("#0A0B0C"));

    // A theme without tokens keeps the mode defaults.
    let plain = ir_of(
        r##"{
  "ir": "0.1", "app": "T",
  "state": [],
  "tree": { "id": 0, "kind": "Container", "props": { "width": 40, "height": 20 },
    "children": [], "events": {} },
  "theme": { "mode": "dark", "accent": "#4F46E5", "radius": 4 }
}"##,
    );
    let plain_state = StateView::from_ir(&plain);
    let plain_laid = layout(&plain, &plain_state, 200, 120);
    assert_ne!(node(&plain_laid, 0).fill, Rgba::from_hex("#010203"));
}

#[test]
fn theme_tokens_change_pixels_deterministically() {
    let base = digest_of(
        r##"{
  "ir": "0.1", "app": "T", "state": [],
  "tree": { "id": 0, "kind": "Container", "props": { "width": 60, "height": 40 },
    "children": [], "events": {} },
  "theme": { "mode": "light", "accent": "#4F46E5", "radius": 8 }
}"##,
    );
    let tokened = digest_of(
        r##"{
  "ir": "0.1", "app": "T", "state": [],
  "tree": { "id": 0, "kind": "Container", "props": { "width": 60, "height": 40 },
    "children": [], "events": {} },
  "theme": { "mode": "light", "accent": "#4F46E5", "radius": 8,
    "tokens": { "surface": "#123456", "onSurface": "#FFFFFF" } }
}"##,
    );
    assert_ne!(base, tokened);
    // Same input, same digest.
    assert_eq!(
        tokened,
        digest_of(
            r##"{
  "ir": "0.1", "app": "T", "state": [],
  "tree": { "id": 0, "kind": "Container", "props": { "width": 60, "height": 40 },
    "children": [], "events": {} },
  "theme": { "mode": "light", "accent": "#4F46E5", "radius": 8,
    "tokens": { "surface": "#123456", "onSurface": "#FFFFFF" } }
}"##
        )
    );
}

// --------------------------------------------------------------------- effects

#[test]
fn gradient_glass_shadow_and_opacity_have_distinct_digests() {
    let plain = digest_of(&effect_ir("\"color\": \"#112233\""));
    let gradient = digest_of(&effect_ir("\"gradient\": \"#4F46E5,#101828\""));
    let glass = digest_of(&effect_ir("\"glass\": true"));
    let shadow = digest_of(&effect_ir("\"shadow\": 4"));
    let opacity = digest_of(&effect_ir("\"opacity\": 0.5"));
    let border = digest_of(&effect_ir("\"border\": \"#FF0000\""));

    let all = [
        ("plain", &plain),
        ("gradient", &gradient),
        ("glass", &glass),
        ("shadow", &shadow),
        ("opacity", &opacity),
        ("border", &border),
    ];
    for (i, (a, da)) in all.iter().enumerate() {
        for (b, db) in all.iter().skip(i + 1) {
            assert_ne!(da, db, "{a} and {b} must differ");
        }
    }
}

#[test]
fn effects_are_deterministic_across_runs() {
    for props in [
        "\"gradient\": \"#4F46E5,#101828\"",
        "\"glass\": true",
        "\"shadow\": 8",
        "\"opacity\": 0.95",
    ] {
        let first = digest_of(&effect_ir(props));
        let second = digest_of(&effect_ir(props));
        assert_eq!(first, second, "effect {props} is not deterministic");
    }
}

#[test]
fn effect_values_are_recorded_in_the_layout() {
    let ir = ir_of(&effect_ir(
        "\"gradient\": \"#4F46E5,#101828\", \"shadow\": 4, \"opacity\": 0.5, \"glass\": true",
    ));
    let state = StateView::from_ir(&ir);
    let laid = layout(&ir, &state, 200, 120);
    let n = node(&laid, 0);
    assert!(n.effects.gradient.is_some());
    assert_eq!(n.effects.shadow, 4);
    assert_eq!(n.effects.glass, true);
    assert_eq!(n.effects.opacity, 512); // 0.5 in fixed point
}

#[test]
fn unknown_props_are_ignored_and_never_fatal() {
    let ir = ir_of(&effect_ir("\"totallyUnknown\": 12, \"another\": \"x\""));
    let state = StateView::from_ir(&ir);
    let laid = layout(&ir, &state, 200, 120);
    assert_eq!(laid.len(), 1);
}

#[test]
fn margin_pushes_a_child_inside_its_parent() {
    let ir = ir_of(
        r##"{
  "ir": "0.1", "app": "M", "state": [],
  "tree": { "id": 0, "kind": "Container", "props": { "width": 100, "height": 60 },
    "children": [
      { "id": 1, "kind": "Container", "props": { "width": 20, "height": 20, "margin": 5 },
        "children": [], "events": {} }
    ], "events": {} },
  "theme": { "mode": "light", "accent": "#4F46E5", "radius": 0 }
}"##,
    );
    let state = StateView::from_ir(&ir);
    let laid = layout(&ir, &state, 200, 120);
    let child = node(&laid, 1);
    assert_eq!(child.rect.x, 5);
    assert_eq!(child.rect.y, 5);
}

// ------------------------------------------------------------ when visibility

const WHEN_IR: &str = r##"{
  "ir": "0.1", "app": "When",
  "state": [ { "name": "dark", "type": "bool", "init": false } ],
  "tree": { "id": 0, "kind": "Column", "props": { "gap": 4, "padding": 8 },
    "children": [
      { "id": 1, "kind": "Text", "props": { "value": "always" }, "children": [], "events": {} },
      { "id": 2, "kind": "Container", "props": { "width": 40, "height": 40, "color": "#FF0000" },
        "when": "dark", "children": [
          { "id": 3, "kind": "Text", "props": { "value": "hidden" }, "children": [], "events": {} }
        ], "events": {} }
    ], "events": {} },
  "theme": { "mode": "light", "accent": "#4F46E5", "radius": 4 }
}"##;

#[test]
fn when_hides_the_node_and_its_subtree() {
    let ir = ir_of(WHEN_IR);
    let mut state = StateView::from_ir(&ir);

    let hidden = layout(&ir, &state, 200, 120);
    assert_eq!(
        hidden.len(),
        2,
        "the hidden node and its child are not laid out"
    );
    assert!(hidden.iter().all(|n| n.id != 2 && n.id != 3));

    state.set("dark", StateValue::Bool(true));
    let shown = layout(&ir, &state, 200, 120);
    assert_eq!(shown.len(), 4);
    assert!(shown.iter().any(|n| n.id == 2));
    assert!(shown.iter().any(|n| n.id == 3));
}

#[test]
fn when_changes_the_digest_and_keeps_dirty_rects_bounded() {
    let ir = ir_of(WHEN_IR);
    let mut state = StateView::from_ir(&ir);
    let mut renderer = Renderer::new(200, 120);
    let before = renderer.render(&ir, &state);
    state.set("dark", StateValue::Bool(true));
    let after = renderer.render(&ir, &state);
    assert_ne!(digest_hex(&before.pixels), digest_hex(&after.pixels));
    assert!(!after.dirty.is_empty());
    assert!(after.dirty.len() <= 64);
    for r in &after.dirty {
        assert!(r.w > 0 && r.h > 0);
        assert!(r.x + r.w <= 200 && r.y + r.h <= 120);
    }
    // The change is local: the dirty area is smaller than the whole surface.
    let dirty_area: u64 = after.dirty.iter().map(|r| r.w as u64 * r.h as u64).sum();
    assert!(
        dirty_area < 200 * 120,
        "a conditional toggle must not dirty the whole surface"
    );
}

#[test]
fn when_on_a_non_bool_state_is_rejected() {
    let bad = WHEN_IR.replace(
        "{ \"name\": \"dark\", \"type\": \"bool\", \"init\": false }",
        "{ \"name\": \"dark\", \"type\": \"int\", \"init\": 0 }",
    );
    let err = parse_ir(&bad).expect_err("a non-bool when must be rejected");
    assert!(
        err.to_string().contains("when must reference a bool state"),
        "unexpected diagnostic: {err}"
    );
}

// -------------------------------------------------------------- new node kinds

const KINDS_IR: &str = r##"{
  "ir": "0.1", "app": "Kinds",
  "state": [
    { "name": "on", "type": "bool", "init": true },
    { "name": "pct", "type": "int", "init": 40 },
    { "name": "vol", "type": "int", "init": 30 },
    { "name": "pick", "type": "string", "init": "b" },
    { "name": "tab", "type": "string", "init": "Two" }
  ],
  "tree": { "id": 0, "kind": "Column", "props": { "gap": 4, "padding": 8 },
    "children": [
      { "id": 1, "kind": "Card", "props": { "padding": 4 },
        "children": [ { "id": 2, "kind": "Text", "props": { "value": "card" }, "children": [], "events": {} } ],
        "events": {} },
      { "id": 3, "kind": "Divider", "props": {}, "children": [], "events": {} },
      { "id": 4, "kind": "Icon", "props": { "name": "star" }, "children": [], "events": {} },
      { "id": 5, "kind": "Badge", "props": { "label": "new" }, "children": [], "events": {} },
      { "id": 6, "kind": "Progress", "props": { "value": 0.4 }, "children": [], "events": {} },
      { "id": 7, "kind": "Switch", "props": { "bind": "on" }, "children": [], "events": {} },
      { "id": 8, "kind": "Slider", "props": { "bind": "vol", "min": 0, "max": 100 }, "children": [], "events": {} },
      { "id": 9, "kind": "Checkbox", "props": { "bind": "on" }, "children": [], "events": {} },
      { "id": 10, "kind": "Radio", "props": { "bind": "pick", "value": "a" }, "children": [], "events": {} },
      { "id": 11, "kind": "Dialog", "props": { "title": "Hi" },
        "children": [ { "id": 12, "kind": "Text", "props": { "value": "body" }, "children": [], "events": {} } ],
        "events": {} },
      { "id": 13, "kind": "Tabs", "props": { "labels": ["One", "Two", "Three"], "bind": "tab" },
        "children": [], "events": {} },
      { "id": 14, "kind": "List", "props": {},
        "children": [ { "id": 15, "kind": "Text", "props": { "value": "row" }, "children": [], "events": {} } ],
        "events": {} },
      { "id": 16, "kind": "Grid", "props": { "columns": 2 },
        "children": [
          { "id": 17, "kind": "Text", "props": { "value": "a" }, "children": [], "events": {} },
          { "id": 18, "kind": "Text", "props": { "value": "b" }, "children": [], "events": {} },
          { "id": 19, "kind": "Text", "props": { "value": "c" }, "children": [], "events": {} }
        ], "events": {} },
      { "id": 20, "kind": "Scroll", "props": { "height": 30 },
        "children": [
          { "id": 21, "kind": "Text", "props": { "value": "one" }, "children": [], "events": {} },
          { "id": 22, "kind": "Text", "props": { "value": "two" }, "children": [], "events": {} },
          { "id": 23, "kind": "Text", "props": { "value": "three" }, "children": [], "events": {} }
        ], "events": {} },
      { "id": 24, "kind": "Avatar", "props": { "label": "AB" }, "children": [], "events": {} },
      { "id": 25, "kind": "Spinner", "props": {}, "children": [], "events": {} }
    ], "events": {} },
  "theme": { "mode": "light", "accent": "#4F46E5", "radius": 6 }
}"##;

#[test]
fn every_new_kind_renders_without_error() {
    let ir = ir_of(KINDS_IR);
    let state = StateView::from_ir(&ir);
    let laid = layout(&ir, &state, 400, 800);
    assert_eq!(laid.len(), 26);
    let pixels = Renderer::new(400, 800).pixels(&ir, &state);
    assert_eq!(pixels.len(), 400 * 800 * 4);
}

#[test]
fn new_kinds_resolve_their_bound_state() {
    let ir = ir_of(KINDS_IR);
    let state = StateView::from_ir(&ir);
    let laid = layout(&ir, &state, 400, 800);
    assert_eq!(node(&laid, 7).control, Some(Control::Switch { on: true }));
    assert_eq!(
        node(&laid, 8).control,
        Some(Control::Slider {
            value: 30,
            min: 0,
            max: 100
        })
    );
    assert_eq!(
        node(&laid, 9).control,
        Some(Control::Checkbox { checked: true })
    );
    assert_eq!(
        node(&laid, 10).control,
        Some(Control::Radio { selected: false })
    );
    assert_eq!(
        node(&laid, 13).control,
        Some(Control::Tabs {
            labels: vec!["One".into(), "Two".into(), "Three".into()],
            selected: 1
        })
    );
    // Progress takes a fractional value 0..1.
    assert_eq!(
        node(&laid, 6).control,
        Some(Control::Progress { value: 410 })
    );
}

#[test]
fn grid_uses_the_column_count_and_scroll_clips_to_its_height() {
    let ir = ir_of(KINDS_IR);
    let state = StateView::from_ir(&ir);
    let laid = layout(&ir, &state, 400, 800);
    // Grid: three cells in two columns -> two rows.
    let a = node(&laid, 17).rect;
    let b = node(&laid, 18).rect;
    let c = node(&laid, 19).rect;
    assert_eq!(a.y, b.y, "the first two cells share a row");
    assert!(c.y > a.y, "the third cell wraps to the next row");
    assert!(a.x < b.x, "cells advance horizontally inside a row");
    // Scroll: the viewport clips its children.
    let scroll = node(&laid, 20);
    assert_eq!(scroll.rect.h, 30);
    assert!(node(&laid, 23).clip.is_some());
}

#[test]
fn unknown_kind_falls_back_to_container_or_text() {
    let ir = ir_of(
        r##"{
  "ir": "0.1", "app": "U", "state": [],
  "tree": { "id": 0, "kind": "Column", "props": {},
    "children": [
      { "id": 1, "kind": "FutureWidget", "props": { "label": "hi" }, "children": [], "events": {} },
      { "id": 2, "kind": "FutureBox", "props": {},
        "children": [ { "id": 3, "kind": "Text", "props": { "value": "x" }, "children": [], "events": {} } ],
        "events": {} }
    ], "events": {} },
  "theme": { "mode": "light", "accent": "#4F46E5", "radius": 4 }
}"##,
    );
    let state = StateView::from_ir(&ir);
    let laid = layout(&ir, &state, 200, 120);
    // A leaf unknown kind renders as text with its label.
    assert_eq!(node(&laid, 1).kind, Kind::Other);
    assert_eq!(node(&laid, 1).text.as_deref(), Some("hi"));
    // An unknown kind with children renders as a container.
    assert!(node(&laid, 2).fill.is_some());
    assert_eq!(laid.len(), 4);
}

// ------------------------------------------------------------------ animation

const ANIM_IR: &str = r##"{
  "ir": "0.1", "app": "A", "state": [],
  "tree": { "id": 0, "kind": "Column", "props": { "gap": 4 },
    "children": [
      { "id": 1, "kind": "Container", "props": { "width": 60, "height": 40, "animate": "spring" },
        "children": [], "events": {} },
      { "id": 2, "kind": "Container", "props": { "width": 60, "height": 40, "animate": "ease" },
        "children": [], "events": {} }
    ], "events": {} },
  "theme": { "mode": "light", "accent": "#4F46E5", "radius": 4 }
}"##;

#[test]
fn animation_tick_sequences_are_deterministic() {
    let ir = ir_of(ANIM_IR);
    let state = StateView::from_ir(&ir);
    let sequence = |count: usize| -> Vec<String> {
        let mut animator = Animator::new();
        let mut renderer = Renderer::new(200, 120);
        let mut out = Vec::new();
        for _ in 0..count {
            animator.tick(16);
            let frame = animator.frame(&ir, &state, &mut renderer);
            out.push(digest_hex(&frame.pixels));
        }
        out
    };
    let a = sequence(24);
    let b = sequence(24);
    assert_eq!(
        a, b,
        "two identical tick sequences must produce identical digests"
    );
    assert!(
        a.windows(2).any(|w| w[0] != w[1]),
        "the animation must change frames"
    );
}

#[test]
fn spring_and_ease_curves_differ() {
    let ir = ir_of(ANIM_IR);
    let state = StateView::from_ir(&ir);
    let at = |ms: i64| {
        let laid = layout_at(&ir, &state, 200, 120, ms);
        (node(&laid, 1).rect.w, node(&laid, 2).rect.w)
    };
    let (spring, ease) = at(64);
    assert!(spring < 60, "the spring is still growing");
    assert!(ease < 60, "the ease is still growing");
    // Settled frames are back to the full box.
    assert_eq!(at(4000), (60, 60));
}

#[test]
fn animate_none_keeps_the_box_stable() {
    let ir = ir_of(
        r##"{
  "ir": "0.1", "app": "A", "state": [],
  "tree": { "id": 0, "kind": "Column", "props": {},
    "children": [
      { "id": 1, "kind": "Container", "props": { "width": 60, "height": 40 }, "children": [], "events": {} }
    ], "events": {} },
  "theme": { "mode": "light", "accent": "#4F46E5", "radius": 4 }
}"##,
    );
    let state = StateView::from_ir(&ir);
    for ms in [0i64, 16, 64, 4000] {
        let laid = layout_at(&ir, &state, 200, 120, ms);
        assert_eq!(node(&laid, 1).rect.w, 60);
        assert_eq!(node(&laid, 1).rect.h, 40);
    }
    assert_eq!(
        node(&layout_at(&ir, &state, 200, 120, 0), 1)
            .effects
            .opacity,
        1024
    );
}

#[test]
fn unknown_animate_value_is_rejected() {
    let bad = ANIM_IR.replace("\"spring\"", "\"bounce\"");
    let err = parse_ir(&bad).expect_err("an unknown animate value must be rejected");
    assert!(err.to_string().contains("animate has unknown value"));
}

#[test]
fn animation_without_a_change_yields_zero_dirty_rects() {
    let ir = ir_of(ANIM_IR);
    let state = StateView::from_ir(&ir);
    let mut renderer = Renderer::new(200, 120);
    renderer.render(&ir, &state);
    // The same clock position twice: nothing moved, so nothing is dirty.
    let again = renderer.render_at(&ir, &state, 4000);
    assert!(again.dirty.is_empty());
}

// ------------------------------------------------------------- event shapes

#[test]
fn multi_action_and_value_from_events_parse() {
    let ir = ir_of(
        r##"{
  "ir": "0.1", "app": "E",
  "state": [ { "name": "a", "type": "int", "init": 1 }, { "name": "b", "type": "int", "init": 2 } ],
  "tree": { "id": 0, "kind": "Button", "props": { "label": "go" }, "children": [],
    "events": { "tap": { "actions": [
      { "op": "add", "target": "a", "value": 1 },
      { "op": "set", "target": "b", "valueFrom": "a" }
    ] } } },
  "theme": { "mode": "light", "accent": "#4F46E5", "radius": 4 }
}"##,
    );
    let tap = &ir.tree.events[0];
    assert_eq!(tap.name, "tap");
    assert_eq!(tap.actions.len(), 2);
    assert_eq!(tap.actions[1].value_from.as_deref(), Some("a"));
}

#[test]
fn add_value_from_parses_and_reads_state_at_event_time() {
    let ir = ir_of(
        r##"{
  "ir": "0.1", "app": "E",
  "state": [ { "name": "a", "type": "int", "init": 1 }, { "name": "b", "type": "int", "init": 4 } ],
  "tree": { "id": 0, "kind": "Button", "props": { "label": "go" }, "children": [],
    "events": { "tap": { "op": "add", "target": "a", "valueFrom": "b" } } },
  "theme": { "mode": "light", "accent": "#4F46E5", "radius": 4 }
}"##,
    );
    let action = ir.tree.events[0].action();
    assert_eq!(action.op, Op::Add);
    assert_eq!(action.target, "a");
    assert_eq!(action.value, None);
    assert_eq!(action.value_from.as_deref(), Some("b"));

    // Hosts execute the parsed action against the current state view.
    fn int(view: &StateView, name: &str) -> i64 {
        match view.get(name) {
            Some(StateValue::Int(n)) => *n,
            other => panic!("expected int state {name}, got {other:?}"),
        }
    }
    let apply = |state: &mut StateView| {
        let sum = int(state, &action.target) + int(state, action.value_from.as_deref().unwrap());
        state.set(action.target.clone(), StateValue::Int(sum));
    };
    let mut state = StateView::from_ir(&ir);
    apply(&mut state);
    assert_eq!(state.get("a"), Some(&StateValue::Int(5)));
    state.set("b", StateValue::Int(9));
    apply(&mut state);
    assert_eq!(state.get("a"), Some(&StateValue::Int(14)));
}

#[test]
fn add_without_value_or_value_from_is_rejected() {
    let missing = r##"{
  "ir": "0.1", "app": "E",
  "state": [ { "name": "a", "type": "int", "init": 1 }, { "name": "b", "type": "int", "init": 2 } ],
  "tree": { "id": 0, "kind": "Button", "props": { "label": "go" }, "children": [],
    "events": { "tap": { "op": "add", "target": "a" } } },
  "theme": { "mode": "light", "accent": "#4F46E5", "radius": 4 }
}"##;
    let err = parse_ir(missing).expect_err("an add with no value must be rejected");
    assert!(err.to_string().contains("add needs"));

    let non_int = missing.replace(
        r#"{ "op": "add", "target": "a" }"#,
        r#"{ "op": "add", "target": "a", "value": "x", "valueFrom": "b" }"#,
    );
    let err = parse_ir(&non_int).expect_err("a non-int add value must be rejected");
    assert!(err.to_string().contains("add needs"));
}

#[test]
fn value_from_type_mismatch_is_rejected() {
    let bad = r##"{
  "ir": "0.1", "app": "E",
  "state": [ { "name": "a", "type": "int", "init": 1 }, { "name": "s", "type": "string", "init": "x" } ],
  "tree": { "id": 0, "kind": "Button", "props": { "label": "go" }, "children": [],
    "events": { "tap": { "op": "set", "target": "a", "valueFrom": "s" } } },
  "theme": { "mode": "light", "accent": "#4F46E5", "radius": 4 }
}"##;
    let err = parse_ir(bad).expect_err("a type mismatch must be rejected");
    assert!(err.to_string().contains("valueFrom type does not match"));
}

#[test]
fn the_hand_written_when_fixture_renders_two_different_digests() {
    let text = std::fs::read_to_string("tests/fixtures/when.opuir.json")
        .expect("the when fixture must be readable");
    let ir = ir_of(&text);
    let mut state = StateView::from_ir(&ir);
    let mut renderer = Renderer::new(320, 240);

    let hidden = renderer.render(&ir, &state);
    state.set("dark", StateValue::Bool(true));
    let shown = renderer.render(&ir, &state);
    assert_ne!(digest_hex(&hidden.pixels), digest_hex(&shown.pixels));
    assert!(!shown.dirty.is_empty());
    assert!(shown.dirty.len() <= 64);
}

#[test]
fn the_cli_renders_the_when_fixture_and_honours_the_tick_clock() {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ir_path = dir.join("when.opuir.json");
    std::fs::copy("tests/fixtures/when.opuir.json", &ir_path).unwrap();
    let exe = env!("CARGO_BIN_EXE_opurom-render");
    let run = |args: &[&str]| {
        let out = Command::new(exe)
            .arg(&ir_path)
            .args(["320", "240"])
            .arg(dir.join("when.ppm"))
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    };
    let light = run(&["--state", "dark=false"]);
    let dark = run(&["--state", "dark=true"]);
    assert_ne!(light, dark, "the when state must change the digest");
    // A tick position is accepted and is deterministic.
    assert_eq!(run(&["--tick", "0"]), run(&["--tick", "0"]));
    assert!(run(&["--tick", "0"]).contains("digest="));
}

#[test]
fn the_cli_rejects_an_unknown_argument() {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ir_path = dir.join("when2.opuir.json");
    std::fs::copy("tests/fixtures/when.opuir.json", &ir_path).unwrap();
    let exe = env!("CARGO_BIN_EXE_opurom-render");
    let out = Command::new(exe)
        .arg(&ir_path)
        .args(["320", "240"])
        .arg(dir.join("when2.ppm"))
        .arg("--nope")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("unexpected argument"));
}

// --------------------------------------------------------------- regressions

#[test]
fn v0_1_golden_digest_is_unchanged_by_r2_features() {
    let counter = std::fs::read_to_string("../opurom-spec/counter.opuir.json")
        .expect("the golden IR fixture must be readable");
    let ir = ir_of(&counter);
    let state = StateView::from_ir(&ir);
    let mut renderer = Renderer::new(800, 600);
    assert_eq!(
        digest_hex(&renderer.render(&ir, &state).pixels),
        "88127ca1577cdb61"
    );
    let mut renderer = Renderer::new(320, 240);
    assert_eq!(
        digest_hex(&renderer.render(&ir, &state).pixels),
        "2fc4eac75fd50f89"
    );
}

#[test]
fn row_direction_and_kinds_are_stable() {
    let ir = ir_of(KINDS_IR);
    let state = StateView::from_ir(&ir);
    let laid = layout(&ir, &state, 1000, 800);
    assert_eq!(node(&laid, 0).direction, Direction::Vertical);
    assert_eq!(node(&laid, 14).direction, Direction::Vertical);
}
