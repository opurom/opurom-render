//! A3/M4 ripple primitive (ANIMATION_AND_PHYSICS.md §8): integer fixed-point
//! ripple progress/radius/alpha and a clip-aware Canvas draw. The radius and
//! alpha follow Flutter's InkRipple timing (fade-in / ease expansion / delayed
//! fade-out). Opt-in only; no default path calls it, so goldens stay identical.

use opurom_render::{
    digest_hex, raster::Canvas, ripple_alpha, ripple_progress, ripple_radius, Rect, Rgba, ONE,
    RIPPLE_DURATION_MS, RIPPLE_FADE_IN_MS, RIPPLE_FADE_OUT_START_MS, RIPPLE_PEAK_ALPHA,
    RIPPLE_RADIUS_MS, RIPPLE_START_PERCENT, RIPPLE_TOTAL_MS,
};

const COUNTER_PATH: &str = "../opurom-examples/golden/counter.opuir.json";

#[test]
fn progress_has_exact_endpoints_and_is_monotonic() {
    let d = RIPPLE_DURATION_MS;
    assert_eq!(ripple_progress(-5, d), 0, "before press holds zero");
    assert_eq!(ripple_progress(0, d), 0, "at press zero");
    assert_eq!(ripple_progress(d, d), ONE, "at duration settles to ONE");
    assert_eq!(ripple_progress(d + 500, d), ONE, "past duration holds ONE");
    let mut prev = 0;
    for t in 0..=d {
        let p = ripple_progress(t, d);
        assert!(p >= prev && p <= ONE, "monotonic bounded at t={t}");
        prev = p;
    }
}

#[test]
fn ripple_follows_flutter_ink_timing() {
    // Radius starts at 30% of target, eases out to full by 225ms, then holds.
    let max_r = 100;
    let start_r = max_r * RIPPLE_START_PERCENT / 100;
    assert_eq!(
        ripple_radius(0, max_r),
        start_r,
        "starts at 30% like InkRipple"
    );
    assert_eq!(
        ripple_radius(RIPPLE_RADIUS_MS, max_r),
        max_r,
        "full at 225ms"
    );
    assert_eq!(
        ripple_radius(RIPPLE_TOTAL_MS, max_r),
        max_r,
        "holds after expand"
    );
    let mut prev_r = -1;
    for t in 0..=RIPPLE_TOTAL_MS {
        let r = ripple_radius(t, max_r);
        assert!(r >= prev_r && r <= max_r, "radius monotonic at t={t}");
        prev_r = r;
    }
    // Alpha fades in over 75ms, holds at peak until fade-out begins, then
    // decays to zero at the end of the 375ms fade-out.
    assert_eq!(ripple_alpha(0, RIPPLE_PEAK_ALPHA), 0, "no alpha at press");
    assert_eq!(
        ripple_alpha(RIPPLE_FADE_IN_MS, RIPPLE_PEAK_ALPHA),
        RIPPLE_PEAK_ALPHA,
        "full alpha at 75ms"
    );
    assert_eq!(
        ripple_alpha(RIPPLE_FADE_OUT_START_MS, RIPPLE_PEAK_ALPHA),
        RIPPLE_PEAK_ALPHA,
        "holds peak until fade-out starts"
    );
    assert_eq!(
        ripple_alpha(RIPPLE_TOTAL_MS, RIPPLE_PEAK_ALPHA),
        0,
        "fully faded at 600ms"
    );
    let mut prev = 0;
    for t in 0..=RIPPLE_FADE_IN_MS {
        let a = ripple_alpha(t, RIPPLE_PEAK_ALPHA);
        assert!(a >= prev, "alpha rises during fade-in at t={t}");
        prev = a;
    }
    prev = RIPPLE_PEAK_ALPHA + 1;
    for t in RIPPLE_FADE_OUT_START_MS..=RIPPLE_TOTAL_MS {
        let a = ripple_alpha(t, RIPPLE_PEAK_ALPHA);
        assert!(a <= prev, "alpha falls during fade-out at t={t}");
        prev = a;
    }
    for t in 0..=RIPPLE_TOTAL_MS {
        let a = ripple_alpha(t, RIPPLE_PEAK_ALPHA);
        assert!(a >= 0 && a <= RIPPLE_PEAK_ALPHA, "alpha bounded at t={t}");
    }
    assert_eq!(
        RIPPLE_TOTAL_MS, 375,
        "splash ends with the fade-out controller"
    );
}

#[test]
fn canvas_ripple_is_deterministic_and_clip_aware() {
    let color = Rgba {
        r: 37,
        g: 209,
        b: 255,
        a: 120,
    };
    let draw = || {
        let mut c = Canvas::new(80, 80, Rgba::rgb(0, 0, 0));
        c.draw_ripple(40, 40, 20, color);
        c.pixels
    };
    let a = draw();
    let b = draw();
    assert_eq!(digest_hex(&a), digest_hex(&b), "ripple draw deterministic");
    // Center pixel changed from background, a corner outside the disc did not.
    let at = |x: usize, y: usize| {
        let i = (y * 80 + x) * 4;
        [a[i], a[i + 1], a[i + 2], a[i + 3]]
    };
    assert_ne!(at(40, 40), [0, 0, 0, 255], "ripple covers center");
    assert_eq!(at(0, 0), [0, 0, 0, 255], "ripple does not cover far corner");
}

#[test]
fn ripple_respects_canvas_clip() {
    let color = Rgba {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };
    let mut c = Canvas::new(80, 80, Rgba::rgb(0, 0, 0));
    c.clip = Some(Rect {
        x: 0,
        y: 0,
        w: 40,
        h: 80,
    });
    c.draw_ripple(40, 40, 20, color);
    let at = |x: usize, y: usize| {
        let i = (y * 80 + x) * 4;
        [
            c.pixels[i],
            c.pixels[i + 1],
            c.pixels[i + 2],
            c.pixels[i + 3],
        ]
    };
    // Left half (inside clip) painted; right half (outside clip) untouched.
    assert_ne!(at(30, 40), [0, 0, 0, 255], "inside clip painted");
    assert_eq!(at(50, 40), [0, 0, 0, 255], "outside clip not painted");
}

#[test]
fn zero_radius_and_transparent_color_draw_nothing() {
    let mut c = Canvas::new(40, 40, Rgba::rgb(0, 0, 0));
    let before = c.pixels.clone();
    c.draw_ripple(
        20,
        20,
        0,
        Rgba {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
    );
    c.draw_ripple(
        20,
        20,
        10,
        Rgba {
            r: 255,
            g: 0,
            b: 0,
            a: 0,
        },
    );
    assert_eq!(c.pixels, before, "no-op draws leave frame identical");
}

#[test]
fn default_path_keeps_the_two_counter_goldens() {
    // The ripple primitive has no caller in the default render path.
    let text = std::fs::read_to_string(COUNTER_PATH).unwrap();
    let ir = opurom_render::parse_ir(&text).unwrap();
    let state = opurom_render::StateView::from_ir(&ir);
    assert_eq!(
        digest_hex(
            &opurom_render::Renderer::new(320, 240)
                .render(&ir, &state)
                .pixels
        ),
        "8d45f412996a99c6"
    );
    assert_eq!(
        digest_hex(
            &opurom_render::Renderer::new(800, 600)
                .render(&ir, &state)
                .pixels
        ),
        "318beb1dd337bc4e"
    );
}
