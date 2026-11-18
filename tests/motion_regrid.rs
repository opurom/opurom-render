//! Regression: a full 320x240 motion render at several ticks must not panic on
//! an out-of-range clip (the row-template opaque fill used to index past the
//! framebuffer when the effective clip's y-range was empty). Mirrors the mobile
//! `frame_at` baseline that caught the regression.

use opurom_render::{digest_hex, parse_ir, StateView, SETTLED_MS};

fn compat_renderer(width: u32, height: u32) -> opurom_render::Renderer {
    let mut renderer = opurom_render::Renderer::new(width, height);
    renderer.set_quality(opurom_render::QualityProfile::Compat);
    renderer
}

const MOTION_IR: &str = include_str!("fixtures/motion.opuir.json");

#[test]
fn motion_frames_at_320x240_never_index_past_framebuffer() {
    let ir = parse_ir(MOTION_IR).expect("parse motion IR");
    let state = StateView::from_ir(&ir);
    let mut renderer = compat_renderer(320, 240);
    let mut digests = Vec::new();
    for tick in [0i64, 400, SETTLED_MS] {
        let frame = renderer.render_at(&ir, &state, tick);
        assert_eq!(frame.pixels.len(), 320 * 240 * 4);
        assert!(frame.pixels.chunks_exact(4).all(|p| p[3] == 255));
        digests.push(digest_hex(&frame.pixels));
    }
    // Settled frame must match the fixed Settled route; tick 0 and 400 differ.
    assert_ne!(digests[0], digests[1], "clock must advance the animation");
}

#[test]
fn incremental_motion_redraw_stays_byte_identical() {
    let ir = parse_ir(MOTION_IR).expect("parse motion IR");
    let state = StateView::from_ir(&ir);
    let mut full = compat_renderer(320, 240);
    let mut inc = compat_renderer(320, 240);
    for tick in [0i64, 200, 400, SETTLED_MS, 100, SETTLED_MS] {
        let f = full.render_at(&ir, &state, tick);
        let i = inc.render_incremental_at(&ir, &state, tick);
        assert_eq!(
            f.pixels, i.pixels,
            "incremental must match full at tick {tick}"
        );
    }
}
