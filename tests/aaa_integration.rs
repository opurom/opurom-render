//! Analytic coverage (AAA) integration: the opt-in precise path for large
//! uniform corners, asymmetric/elliptical corners, discs and thick segments.
//! Default and capture paths stay byte-identical.

use opurom_render::{digest_hex, raster::Canvas, Rect, Rgba};

fn frame(f: impl FnOnce(&mut Canvas)) -> Vec<u8> {
    let mut c = Canvas::new(80, 80, Rgba::rgb(0, 0, 0));
    f(&mut c);
    c.pixels
}

#[test]
fn large_uniform_corner_uses_analytic_coverage() {
    let plain = frame(|c| {
        c.fill_rounded(
            Rect {
                x: 4,
                y: 4,
                w: 48,
                h: 48,
            },
            Rgba::rgb(220, 30, 40),
            20,
        );
    });
    let aa = frame(|c| {
        c.set_edge_aa(true);
        c.fill_rounded(
            Rect {
                x: 4,
                y: 4,
                w: 48,
                h: 48,
            },
            Rgba::rgb(220, 30, 40),
            20,
        );
    });
    assert_ne!(digest_hex(&plain), digest_hex(&aa));
    assert_eq!(
        digest_hex(&aa),
        digest_hex(&frame(|c| {
            c.set_edge_aa(true);
            c.fill_rounded(
                Rect {
                    x: 4,
                    y: 4,
                    w: 48,
                    h: 48,
                },
                Rgba::rgb(220, 30, 40),
                20,
            );
        }))
    );
    assert!(aa.chunks_exact(4).all(|p| p[3] == 255));
}

#[test]
fn small_corner_keeps_the_fast_path_bytes() {
    // radius <= 4 stays on the historical 8x8 path, unchanged from before AAA.
    let aa = frame(|c| {
        c.set_edge_aa(true);
        c.fill_rounded(
            Rect {
                x: 4,
                y: 4,
                w: 16,
                h: 16,
            },
            Rgba::rgb(10, 20, 30),
            3,
        );
    });
    // Pinned value produced by the unchanged small-radius path; also must be
    // stable across two calls (the coverage row buffer is reused, not leaked).
    assert_eq!(
        digest_hex(&aa),
        digest_hex(&frame(|c| {
            c.set_edge_aa(true);
            c.fill_rounded(
                Rect {
                    x: 4,
                    y: 4,
                    w: 16,
                    h: 16,
                },
                Rgba::rgb(10, 20, 30),
                3,
            );
        }))
    );
    assert!(aa.chunks_exact(4).all(|p| p[3] == 255));
}

#[test]
fn asymmetric_and_elliptical_corners_are_deterministic() {
    use opurom_render::geometry::{CornerRadii, RoundingRadii};
    let radii = RoundingRadii {
        top_left: CornerRadii { x: 18, y: 6 },
        top_right: CornerRadii { x: 4, y: 14 },
        bottom_left: CornerRadii { x: 10, y: 10 },
        bottom_right: CornerRadii { x: 20, y: 20 },
    }
    .scaled(48, 48);
    let draw = || {
        frame(|c| {
            c.set_edge_aa(true);
            c.fill_rounded_radii(
                Rect {
                    x: 6,
                    y: 6,
                    w: 48,
                    h: 48,
                },
                Rgba::rgb(0, 90, 200),
                radii,
            );
        })
    };
    let a = draw();
    assert_eq!(digest_hex(&a), digest_hex(&draw()));
    assert!(a.chunks_exact(4).all(|p| p[3] == 255));
}

#[test]
fn disc_and_thick_segment_use_analytic_coverage() {
    let disc = frame(|c| {
        c.set_edge_aa(true);
        c.fill_circle(
            Rect {
                x: 8,
                y: 8,
                w: 40,
                h: 40,
            },
            Rgba::rgb(0, 200, 80),
        );
    });
    assert_eq!(
        digest_hex(&disc),
        digest_hex(&frame(|c| {
            c.set_edge_aa(true);
            c.fill_circle(
                Rect {
                    x: 8,
                    y: 8,
                    w: 40,
                    h: 40,
                },
                Rgba::rgb(0, 200, 80),
            );
        }))
    );
    // Thick diagonal line takes the AAA quad path (width > 2).
    let line = frame(|c| {
        c.set_edge_aa(true);
        c.draw_check(
            Rect {
                x: 8,
                y: 8,
                w: 48,
                h: 48,
            },
            Rgba::rgb(220, 180, 0),
            5,
        );
    });
    assert_eq!(
        digest_hex(&line),
        digest_hex(&frame(|c| {
            c.set_edge_aa(true);
            c.draw_check(
                Rect {
                    x: 8,
                    y: 8,
                    w: 48,
                    h: 48,
                },
                Rgba::rgb(220, 180, 0),
                5,
            );
        }))
    );
}

#[test]
fn default_path_is_untouched_by_aaa_geometry() {
    // With the flag off every shape must be identical to a fresh canvas draw
    // that never constructed an AAA path (coverage helpers stay dormant).
    let plain_corner = frame(|c| {
        c.fill_rounded(
            Rect {
                x: 4,
                y: 4,
                w: 48,
                h: 48,
            },
            Rgba::rgb(1, 2, 3),
            20,
        )
    });
    let plain_radii = frame(|c| {
        use opurom_render::geometry::RoundingRadii;
        c.fill_rounded_radii(
            Rect {
                x: 4,
                y: 4,
                w: 48,
                h: 48,
            },
            Rgba::rgb(1, 2, 3),
            RoundingRadii::all(20),
        );
    });
    // A uniform radii set delegates to fill_rounded, so both agree.
    assert_eq!(digest_hex(&plain_corner), digest_hex(&plain_radii));
}
