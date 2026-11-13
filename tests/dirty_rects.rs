use opurom_render::{parse_ir, Renderer, StateValue, StateView};

const WHEN_IR: &str = include_str!("fixtures/when.opuir.json");

#[test]
fn identical_state_changes_emit_identical_ordered_dirty_lists() {
    let ir = parse_ir(WHEN_IR).unwrap();
    let mut state = StateView::from_ir(&ir);
    let mut renderer = Renderer::new(320, 240);
    renderer.render(&ir, &state);
    state.set("dark", StateValue::Bool(true));
    let first = renderer.render(&ir, &state);
    state.set("dark", StateValue::Bool(false));
    renderer.render(&ir, &state);
    state.set("dark", StateValue::Bool(true));
    let second = renderer.render(&ir, &state);
    assert!(!first.dirty.is_empty());
    assert_eq!(
        first, second,
        "identical state changes must emit identical frames"
    );
    assert!(first.dirty.len() <= 64);
    assert!(
        first
            .dirty
            .windows(2)
            .all(|p| (p[0].y, p[0].x) <= (p[1].y, p[1].x)),
        "dirty rects must be ordered by (y, x): {:?}",
        first.dirty
    );
}

#[test]
fn a_resize_diff_is_canonically_ordered_and_bounded() {
    let ir = parse_ir(WHEN_IR).unwrap();
    let mut state = StateView::from_ir(&ir);
    let mut renderer = Renderer::new(200, 120);
    renderer.render(&ir, &state);
    state.set("dark", StateValue::Bool(true));
    renderer.resize(240, 160).unwrap();
    let frame = renderer.render(&ir, &state);
    assert!(!frame.dirty.is_empty());
    assert!(frame.dirty.len() <= 64);
    assert!(
        frame
            .dirty
            .windows(2)
            .all(|p| (p[0].y, p[0].x) <= (p[1].y, p[1].x)),
        "dirty rects must be ordered by (y, x): {:?}",
        frame.dirty
    );
    for r in &frame.dirty {
        assert!(r.w > 0 && r.h > 0 && r.x + r.w <= 240 && r.y + r.h <= 160);
    }
}
