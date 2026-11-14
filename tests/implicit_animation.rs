//! Implicit-animation renderer tests.
//!
//! The renderer interpolates a node's `radius`, `padding`, `shadow`, `color`
//! and `opacity` from its previous targets to its current ones over the
//! `animate` schedule (`duration` + `delay`), advancing one tick at a time.
//! Every value is deterministic integer-only fixed point, and a settled frame
//! is byte-identical to a non-animated render of the same props.

use opurom_render::{
    animate_progress, layout_animated_at, layout_at, lerp, lerp_rgba, parse_ir, AnimSpec,
    AnimTable, AnimValue, Curve, Renderer, Rgba, StateView, ONE, SETTLED_MS,
};

/// A `Container` with an animated prop set. The root `Column` is inert so the
/// animated node is always `laid[1]`.
fn container_ir(props: &str) -> String {
    let tmpl = r##"{
  "ir": "0.1", "app": "ImplicitAnim", "state": [],
  "tree": { "id": 0, "kind": "Column", "props": { "gap": 4 },
    "children": [
      { "id": 1, "kind": "Container", "props": { @PROPS@ },
        "children": [], "events": {} }
    ], "events": {} },
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 }
}"##;
    tmpl.replace("@PROPS@", props)
}

fn parse(props: &str) -> opurom_render::Ir {
    parse_ir(&container_ir(props)).unwrap()
}

fn laid_animated(ir: &opurom_render::Ir, tick: i64) -> Vec<opurom_render::LaidOut> {
    layout_animated_at(
        ir,
        &StateView::from_ir(ir),
        200,
        120,
        tick,
        &mut AnimTable::new(),
    )
}

fn laid_plain(ir: &opurom_render::Ir, tick: i64) -> Vec<opurom_render::LaidOut> {
    layout_at(ir, &StateView::from_ir(ir), 200, 120, tick)
}

#[test]
fn animate_none_and_absent_are_byte_identical() {
    let none = parse(r##""width": 60, "height": 40, "color": "#102030", "animate": "none""##);
    let absent = parse(r##""width": 60, "height": 40, "color": "#102030""##);
    let a = Renderer::new(200, 120).render(&none, &StateView::from_ir(&none));
    let b = Renderer::new(200, 120).render(&absent, &StateView::from_ir(&absent));
    assert_eq!(
        opurom_render::digest_hex(&a.pixels),
        opurom_render::digest_hex(&b.pixels)
    );
}

#[test]
fn settled_animation_is_byte_identical_to_non_animated() {
    // The animated node and the plain node share every prop except `animate`.
    // At the settled tick the interpolated values are exactly the targets, so
    // the frames must match byte for byte.
    let animated = parse(
        r##""width": 60, "height": 40, "color": "#102030", "radius": 12, "padding": 8, "shadow": 3, "opacity": 0.8, "animate": "spring""##,
    );
    let mut plain = parse(
        r##""width": 60, "height": 40, "color": "#102030", "radius": 12, "padding": 8, "shadow": 3, "opacity": 0.8"##,
    );
    plain.tree.children[0].props.animate = Curve::None;

    let frame_anim = Renderer::new(200, 120).render(&animated, &StateView::from_ir(&animated));
    let frame_plain = Renderer::new(200, 120).render(&plain, &StateView::from_ir(&plain));
    assert_eq!(
        opurom_render::digest_hex(&frame_anim.pixels),
        opurom_render::digest_hex(&frame_plain.pixels)
    );
    // The laid-out metrics are equal too, not just the pixels.
    assert_eq!(
        laid_animated(&animated, SETTLED_MS),
        laid_plain(&plain, SETTLED_MS)
    );
}

#[test]
fn layout_metrics_do_not_change_at_rest() {
    // Direct assertion: a settled animated layout equals the same layout with
    // `animate` removed, field for field.
    let animated = parse(
        r##""width": 60, "height": 40, "color": "#102030", "radius": 12, "padding": 8, "shadow": 3, "opacity": 0.8, "animate": "ease", "duration": 200"##,
    );
    let mut plain = animated.clone();
    plain.tree.children[0].props.animate = Curve::None;
    let anim = laid_animated(&animated, SETTLED_MS);
    let base = laid_plain(&plain, SETTLED_MS);
    assert_eq!(anim, base);
    // And the settled node carries exactly the declared target values.
    let node = &anim[1];
    assert_eq!(node.radius, 12);
    assert_eq!(node.effects.shadow, 3);
    assert_eq!(node.effects.opacity, 819); // 0.8 * 1024
    assert_eq!(node.fill, Some(Rgba::rgb(0x10, 0x20, 0x30)));
}

#[test]
fn color_interpolates_between_targets_and_settles() {
    let renderer = Renderer::new(200, 120);
    let mut ir = parse(
        r##""width": 60, "height": 40, "color": "#102030", "animate": "ease", "duration": 200"##,
    );
    let state = StateView::from_ir(&ir);
    // Settle the entry so the next change restarts from the previous target.
    renderer.compute_layout_at(&ir, &state, SETTLED_MS);

    // Change the colour to a different one at tick 1000.
    ir.tree.children[0].props.color = Some(Rgba::rgb(0x40, 0x50, 0x60));
    let at_start = renderer.compute_layout_at(&ir, &state, 1000);
    let at_mid = renderer.compute_layout_at(&ir, &state, 1100);
    let at_end = renderer.compute_layout_at(&ir, &state, 1200);

    assert_eq!(at_start[1].fill, Some(Rgba::rgb(0x10, 0x20, 0x30)));
    let mid = at_mid[1].fill.unwrap();
    // The mid-point colour lies strictly between the two endpoints.
    assert!(mid.r > 0x10 && mid.r < 0x40, "mid r {}", mid.r);
    assert!(mid.g > 0x20 && mid.g < 0x50, "mid g {}", mid.g);
    assert!(mid.b > 0x30 && mid.b < 0x60, "mid b {}", mid.b);
    // At the settle tick the colour is exactly the new target.
    assert_eq!(at_end[1].fill, Some(Rgba::rgb(0x40, 0x50, 0x60)));
}

#[test]
fn opacity_interpolates_between_targets_and_settles() {
    let renderer = Renderer::new(200, 120);
    let mut ir = parse(
        r##""width": 60, "height": 40, "color": "#102030", "opacity": 0.5, "animate": "ease", "duration": 200"##,
    );
    let state = StateView::from_ir(&ir);
    renderer.compute_layout_at(&ir, &state, SETTLED_MS);

    ir.tree.children[0].props.opacity = Some(ONE);
    let at_start = renderer.compute_layout_at(&ir, &state, 1000);
    let at_mid = renderer.compute_layout_at(&ir, &state, 1100);
    let at_end = renderer.compute_layout_at(&ir, &state, 1200);

    assert_eq!(at_start[1].effects.opacity, 512); // 0.5 * 1024
    let mid = at_mid[1].effects.opacity;
    assert!(mid > 512 && mid < ONE, "mid opacity {mid}");
    assert_eq!(at_end[1].effects.opacity, ONE);
}

#[test]
fn radius_and_shadow_interpolate_and_settle() {
    let renderer = Renderer::new(200, 120);
    let mut ir = parse(
        r##""width": 60, "height": 40, "color": "#102030", "radius": 4, "shadow": 1, "animate": "spring", "duration": 200"##,
    );
    let state = StateView::from_ir(&ir);
    renderer.compute_layout_at(&ir, &state, SETTLED_MS);

    ir.tree.children[0].props.radius = Some(20);
    ir.tree.children[0].props.shadow = Some(6);
    let at_start = renderer.compute_layout_at(&ir, &state, 1000);
    let at_mid = renderer.compute_layout_at(&ir, &state, 1100);
    let at_end = renderer.compute_layout_at(&ir, &state, 1200);

    assert_eq!(at_start[1].radius, 4);
    assert_eq!(at_start[1].effects.shadow, 1);
    let mid_radius = at_mid[1].radius;
    let mid_shadow = at_mid[1].effects.shadow;
    assert!(mid_radius > 4 && mid_radius < 20, "mid radius {mid_radius}");
    assert!(mid_shadow > 1 && mid_shadow < 6, "mid shadow {mid_shadow}");
    assert_eq!(at_end[1].radius, 20);
    assert_eq!(at_end[1].effects.shadow, 6);
}

#[test]
fn padding_interpolation_moves_the_child_and_settles() {
    // A Container with padding and one child; animating the padding moves the
    // child's rect and settles on the declared value.
    let renderer = Renderer::new(200, 120);
    let mut ir = parse_ir(
        r##"{
      "ir": "0.1", "app": "Pad", "state": [],
      "tree": { "id": 0, "kind": "Column", "props": { "gap": 4 },
        "children": [
          { "id": 1, "kind": "Container", "props": { "width": 80, "height": 60, "color": "#102030", "padding": 4, "animate": "ease", "duration": 200 },
            "children": [
              { "id": 2, "kind": "Container", "props": { "width": 30, "height": 20, "color": "#405060" }, "children": [], "events": {} }
            ], "events": {} }
        ], "events": {} },
      "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 }
    }"##,
    )
    .unwrap();
    let state = StateView::from_ir(&ir);
    renderer.compute_layout_at(&ir, &state, SETTLED_MS);

    let before = renderer.compute_layout_at(&ir, &state, 1000);
    ir.tree.children[0].props.padding = 16;
    let at_start = renderer.compute_layout_at(&ir, &state, 1000);
    let at_end = renderer.compute_layout_at(&ir, &state, 1200);

    let child_before = laid_plain(&ir, SETTLED_MS)[2].rect;
    // The change starts from the previous target padding, so the child starts
    // where it was before the change.
    assert_eq!(at_start[2].rect, before[2].rect);
    assert_ne!(at_start[2].rect, child_before);
    // Settled, the child sits at the padding-16 position.
    assert_eq!(at_end[2].rect, child_before);
}

#[test]
fn duration_and_delay_ordering_holds_then_runs_then_settles() {
    let renderer = Renderer::new(200, 120);
    let mut ir = parse(
        r##""width": 60, "height": 40, "color": "#102030", "opacity": 0.25, "animate": "ease", "duration": 100, "delay": 50"##,
    );
    let state = StateView::from_ir(&ir);
    renderer.compute_layout_at(&ir, &state, SETTLED_MS);

    ir.tree.children[0].props.opacity = Some(ONE);
    // The change is detected at tick 1000: the delay holds the old value until
    // 1050, the curve runs until 1150, and the node settles exactly there.
    renderer.compute_layout_at(&ir, &state, 1000);
    let held = renderer.compute_layout_at(&ir, &state, 1049);
    let at_delay = renderer.compute_layout_at(&ir, &state, 1050);
    let running = renderer.compute_layout_at(&ir, &state, 1100);
    let settled = renderer.compute_layout_at(&ir, &state, 1150);

    assert_eq!(held[1].effects.opacity, 256); // 0.25 * 1024
    assert_eq!(at_delay[1].effects.opacity, 256);
    assert!(running[1].effects.opacity > 256 && running[1].effects.opacity < ONE);
    assert_eq!(settled[1].effects.opacity, ONE);

    // The pure schedule agrees with the renderer at the same ticks.
    assert_eq!(animate_progress(Curve::Ease, 0, 100, 50), 0);
    assert_eq!(animate_progress(Curve::Ease, 49, 100, 50), 0);
    assert_eq!(animate_progress(Curve::Ease, 50, 100, 50), 0);
    assert_ne!(animate_progress(Curve::Ease, 100, 100, 50), 0);
    assert_eq!(animate_progress(Curve::Ease, 150, 100, 50), ONE);
}

#[test]
fn settled_frame_matches_non_animated_after_a_change() {
    let mut renderer = Renderer::new(200, 120);
    let mut ir = parse(
        r##""width": 60, "height": 40, "color": "#102030", "radius": 8, "animate": "ease", "duration": 200"##,
    );
    let state = StateView::from_ir(&ir);
    // Settle the entry, then change the colour and radius. Detect the change at
    // tick 1000 so the 200 ms animation settles by tick 1200.
    renderer.compute_layout_at(&ir, &state, SETTLED_MS);
    ir.tree.children[0].props.color = Some(Rgba::rgb(0x40, 0x50, 0x60));
    ir.tree.children[0].props.radius = Some(16);
    renderer.compute_layout_at(&ir, &state, 1000);

    let animated_frame = renderer.render_at(&ir, &state, 1200);

    let mut plain = ir.clone();
    plain.tree.children[0].props.animate = Curve::None;
    let plain_frame = Renderer::new(200, 120).render(&plain, &StateView::from_ir(&plain));

    assert_eq!(
        opurom_render::digest_hex(&animated_frame.pixels),
        opurom_render::digest_hex(&plain_frame.pixels)
    );
}

#[test]
fn same_ir_and_tick_is_byte_identical() {
    let ir = parse(
        r##""width": 60, "height": 40, "color": "#102030", "radius": 8, "opacity": 0.6, "animate": "spring", "duration": 200, "delay": 20"##,
    );
    let state = StateView::from_ir(&ir);
    for tick in [0i64, 30, 96, 200, 1000, SETTLED_MS] {
        let a = Renderer::new(200, 120).pixels_at(&ir, &state, tick);
        let b = Renderer::new(200, 120).pixels_at(&ir, &state, tick);
        assert_eq!(
            opurom_render::digest_hex(&a),
            opurom_render::digest_hex(&b),
            "tick {tick} must be deterministic"
        );
    }
}

#[test]
fn interpolation_math_matches_the_half_up_rule() {
    // The renderer's colour blend is the shared `lerp_rgba`: mid-point of a
    // pure channel is the half-up value.
    assert_eq!(
        lerp_rgba(Rgba::rgb(0, 0, 0), Rgba::rgb(100, 0, 0), ONE / 2).r,
        50
    );
    assert_eq!(lerp(0, 100, ONE / 2), 50);
    assert_eq!(lerp(100, 0, ONE / 2), 50);
    // An opacity schedule settles to exactly the target at duration + delay.
    let spec = AnimSpec::new(Curve::Ease, 200, 50);
    assert_eq!(spec.progress_at(49), 0);
    assert_eq!(spec.progress_at(50), 0);
    assert_ne!(spec.progress_at(150), 0);
    assert_eq!(spec.progress_at(250), ONE);
    assert_eq!(spec.progress_at(10_000), ONE);
    // `AnimValue` drives both scalar and colour slots.
    let mixed = AnimValue::lerp(AnimValue::scalar(0), AnimValue::scalar(10), ONE / 2);
    assert_eq!(mixed, AnimValue::scalar(5));
}

#[test]
fn animated_container_entry_grows_then_settles() {
    // The entry animation is the first-layout case: the box grows from its
    // centre and settles to the full target rect.
    let ir = parse(
        r##""width": 60, "height": 40, "color": "#102030", "animate": "ease", "duration": 200"##,
    );
    let at_zero = laid_animated(&ir, 0);
    let at_mid = laid_animated(&ir, 100);
    let at_end = laid_animated(&ir, 200);

    assert_eq!(at_zero[1].rect.w, 0);
    assert_eq!(at_zero[1].rect.h, 0);
    assert!(at_mid[1].rect.w > 0 && at_mid[1].rect.w < 60);
    assert_eq!(at_end[1].rect.w, 60);
    assert_eq!(at_end[1].rect.h, 40);
    // The settled entry equals the plain layout.
    let mut plain = ir.clone();
    plain.tree.children[0].props.animate = Curve::None;
    assert_eq!(laid_animated(&ir, 200), laid_plain(&plain, 200));
}
