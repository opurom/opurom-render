//! A3/M4 ripple primitive (ANIMATION_AND_PHYSICS.md §8): integer fixed-point
//! ripple progress/radius/alpha and a clip-aware Canvas draw. The radius and
//! alpha follow Flutter's InkRipple timing (fade-in / ease expansion / delayed
//! fade-out). Opt-in only; no default path calls it, so goldens stay identical.

use opurom_render::{
    digest_hex, raster::Canvas, ripple_alpha_confirmed, ripple_alpha_held, ripple_frame,
    ripple_radius_confirmed, ripple_radius_unconfirmed, ripple_target_radius, PressPhase, Rect,
    Rgba, RIPPLE_FADE_IN_MS, RIPPLE_FADE_OUT_MS, RIPPLE_PEAK_ALPHA, RIPPLE_RADIUS_MS,
    RIPPLE_UNCONFIRMED_MS,
};

const COUNTER_PATH: &str = "../opurom-examples/golden/counter.opuir.json";

fn press(confirm: Option<i64>) -> PressPhase {
    PressPhase {
        node_id: 1,
        press_start_tick: 0,
        confirm_tick: confirm,
        touch: None,
    }
}

/// Flutter reference frames for a 100px target, held down for one second:
/// radius 30 -> 34.5 -> 56.4 -> 81.2 -> 96.4 -> 105 (target+5), alpha full
/// after 75 ms and never fading while the pointer stays down.
#[test]
fn held_ripple_matches_flutter_frame_table() {
    let peak = RIPPLE_PEAK_ALPHA;
    assert_eq!(ripple_frame(&press(None), 0, 100, peak), None);
    let cases = [
        (75i64, 34i64),
        (225, 56),
        (400, 81),
        (600, 96),
        (1000, 105),
        (1200, 105),
    ];
    for (t, radius) in cases {
        let frame = ripple_frame(&press(None), t, 100, peak).expect("held frame");
        assert_eq!(frame.0, radius, "held radius at {t} ms");
        assert_eq!(frame.1, peak, "held alpha stays full at {t} ms");
    }
    assert_eq!(ripple_alpha_held(74, peak), 252);
    assert_eq!(ripple_alpha_held(75, peak), peak);
    assert_eq!(ripple_radius_unconfirmed(2000, 100), 105);
}

/// Flutter confirm at t=300 ms: radius continues from ~68.5%, accelerates to
/// target+5 by +225 ms, alpha holds to +225 ms, halves at +300 ms, gone at
/// +375 ms.
#[test]
fn confirmed_ripple_matches_flutter_frame_table() {
    let peak = RIPPLE_PEAK_ALPHA;
    let confirm = Some(300i64);
    let cases = [
        (0i64, Some((68i64, 256i64))),
        (225, Some((105, 256))),
        (300, Some((105, 128))),
        (375, None),
    ];
    for (since, expected) in cases {
        assert_eq!(
            ripple_frame(&press(confirm), 300 + since, 100, peak),
            expected,
            "confirmed frame at +{since} ms"
        );
    }
    // The radius never shrinks and alpha never rises after confirm.
    let mut prev_r = -1;
    let mut prev_a = i64::MAX;
    for since in 0..=RIPPLE_FADE_OUT_MS {
        if let Some((r, a)) = ripple_frame(&press(confirm), 300 + since, 100, peak) {
            assert!(r >= prev_r && a <= prev_a, "monotonic at +{since} ms");
            prev_r = r;
            prev_a = a;
        }
    }
}

/// The three controllers are independent: radius keeps growing past the old
/// 225 ms aggregate while alpha holds, and the fade window is 225..375 ms.
#[test]
fn controllers_are_independent_integer_curves() {
    assert_eq!(RIPPLE_FADE_IN_MS, 75);
    assert_eq!(RIPPLE_RADIUS_MS, 225);
    assert_eq!(RIPPLE_FADE_OUT_MS, 375);
    assert_eq!(RIPPLE_UNCONFIRMED_MS, 1000);
    assert_eq!(
        ripple_alpha_confirmed(0, RIPPLE_PEAK_ALPHA),
        RIPPLE_PEAK_ALPHA
    );
    assert_eq!(
        ripple_alpha_confirmed(225, RIPPLE_PEAK_ALPHA),
        RIPPLE_PEAK_ALPHA
    );
    assert_eq!(ripple_alpha_confirmed(300, RIPPLE_PEAK_ALPHA), 128);
    assert_eq!(ripple_alpha_confirmed(375, RIPPLE_PEAK_ALPHA), 0);
    assert_eq!(ripple_radius_confirmed(300, 225, 100), 105);
}

/// S3: Flutter `_getTargetRadius` is half the box diagonal, so the splash
/// reaches all four corners. `max(w,h)/2` stopped at the edge midpoints.
#[test]
fn target_radius_is_half_the_diagonal() {
    assert_eq!(ripple_target_radius(0, 0), 0);
    assert_eq!(ripple_target_radius(3, 4), 2); // isqrt(25)/2
    assert_eq!(ripple_target_radius(100, 100), 70); // isqrt(20000)=141
    assert_eq!(ripple_target_radius(120, 60), 67); // isqrt(18000)=134
    assert_eq!(ripple_target_radius(60, 120), 67); // symmetric
    let target = ripple_target_radius(120, 60);
    assert_eq!(ripple_radius_unconfirmed(1000, target), target + 5);
    assert_eq!(ripple_radius_unconfirmed(2000, target), target + 5);
    assert_eq!(ripple_radius_unconfirmed(0, target), target * 30 / 100);
}

/// S3 visual proof: the diagonal target+5 paints the box corner; the old
/// `max(w,h)/2` target leaves it untouched.
#[test]
fn diagonal_target_covers_button_corners() {
    let color = Rgba {
        r: 9,
        g: 127,
        b: 235,
        a: 255,
    };
    let corner_pixels = |radius: i64| {
        let mut c = Canvas::new(120, 60, Rgba::rgb(0, 0, 0));
        c.draw_ripple(60, 30, radius, color);
        let mut n = 0;
        for y in 0..6 {
            for x in 0..6 {
                let i = (y * 120 + x) * 4;
                if [c.pixels[i], c.pixels[i + 1], c.pixels[i + 2]] != [0, 0, 0] {
                    n += 1;
                }
            }
        }
        n
    };
    assert_eq!(
        corner_pixels(60),
        0,
        "old max(w,h)/2 target misses the corner"
    );
    assert!(
        corner_pixels(ripple_target_radius(120, 60) + 5) > 0,
        "diagonal target reaches the corner"
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
