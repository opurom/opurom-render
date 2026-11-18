//! Adaptive conic subdivision and convex-path geometry for the analytic
//! coverage path. Subdivision follows Skia SkConic::computeQuadPOW2
//! (SkGeometry.cpp:1492), integer ported.

use opurom_render::aaa::{
    conic_quad_count_wang, conic_quad_pow2, rasterize_convex_row, round_rect_path,
};

#[test]
fn conic_subdivision_matches_skia_radius_rule() {
    // Expected pow2 from Skia's error rule at 1/8 px tolerance:
    // err_px = 0.06066*r, quartered per level, capped at 5 (32 quads).
    for (r, want) in [
        (1i64, 0u32),
        (2, 0),
        (4, 1),
        (8, 1),
        (10, 2),
        (16, 2),
        (20, 2),
        (32, 2),
        (50, 3),
        (100, 3),
        (1000, 5),
    ] {
        assert_eq!(conic_quad_pow2(r, 1, 8), want, "radius {r}");
    }
    // Monotone in radius, never above the cap.
    let mut prev = 0;
    for r in 1..=2000 {
        let p = conic_quad_pow2(r, 1, 8);
        assert!(p >= prev && p <= 5, "radius {r} pow2 {p}");
        prev = p;
    }
}

fn polygon_area(path: &[opurom_render::aaa::Segment]) -> f64 {
    // Shoelace on fixed-point vertices (tests may use float for measurement).
    let mut a = 0f64;
    for s in path {
        let (x0, y0) = (s.x0 as f64 / 65536.0, s.y0 as f64 / 65536.0);
        let (x1, y1) = (s.x1 as f64 / 65536.0, s.y1 as f64 / 65536.0);
        a += x0 * y1 - x1 * y0;
    }
    (a / 2.0).abs()
}

#[test]
fn rounded_path_stays_in_bounds_and_matches_area() {
    let (x, y, w, h, r) = (0i64, 0i64, 120i64, 80i64, 20i64);
    let path = round_rect_path(x, y, w, h, r);
    assert!(path.len() > 8, "adaptive arc must add segments");
    for s in &path {
        for (px, py) in [(s.x0, s.y0), (s.x1, s.y1)] {
            let (px, py) = (px as f64 / 65536.0, py as f64 / 65536.0);
            assert!(px >= -0.01 && px <= w as f64 + 0.01, "x {px}");
            assert!(py >= -0.01 && py <= h as f64 + 0.01, "y {py}");
        }
    }
    // A rounded rect loses (4 - pi) r^2 of area to its corners.
    let expected = w as f64 * h as f64 - (4.0 - std::f64::consts::PI) * (r * r) as f64;
    let got = polygon_area(&path);
    assert!(
        (got - expected).abs() / expected < 0.01,
        "area {got} vs {expected}"
    );
}

#[test]
fn straight_rect_path_is_four_segments_and_full_row() {
    let path = round_rect_path(0, 0, 10, 6, 0);
    assert_eq!(path.len(), 4);
    let mut row = vec![0i64; 12];
    rasterize_convex_row(&path, &mut row, -1, 2);
    for x in 0..10 {
        assert_eq!(row[(x + 1) as usize], 255, "interior pixel {x}");
    }
}

#[test]
fn edge_row_has_partial_coverage_near_the_boundary() {
    // Rounded corner row near the top: some pixels partial, some empty.
    let path = round_rect_path(0, 0, 40, 40, 20);
    let mut row = vec![0i64; 42];
    rasterize_convex_row(&path, &mut row, -1, 1);
    let partial = row.iter().filter(|&&a| a > 0 && a < 255).count();
    assert!(partial > 0, "a corner row must carry partial coverage");
    assert!(row.iter().any(|&a| a == 0), "outside corner stays empty");
}

#[test]
fn analytic_corner_produces_many_blend_levels() {
    // Large-radius corner: analytic area coverage should yield a rich alpha
    // ramp, not the coarse steps of supersampling.
    let path = round_rect_path(0, 0, 120, 120, 60);
    let mut levels = std::collections::BTreeSet::new();
    for y in 0..60 {
        let mut row = vec![0i64; 122];
        rasterize_convex_row(&path, &mut row, -1, y);
        for &a in &row {
            if a > 0 && a < 255 {
                levels.insert(a);
            }
        }
    }
    println!("ANALYTIC_LEVELS={}", levels.len());
    assert!(
        levels.len() > 64,
        "analytic coverage levels = {}",
        levels.len()
    );
}

#[test]
fn wang_conic_count_is_monotone_and_matches_impeller_tolerance() {
    // Impeller kPrecision=4 (1/4 px); quarter-circle weights w=sqrt(2)/2.
    // The count grows with radius and stays inside Skia's 32-quad cap.
    let mut prev = 0;
    for r in [1i64, 4, 8, 16, 32, 64, 128, 256, 512, 1024] {
        let n = conic_quad_count_wang(r);
        assert!(n >= 1 && n <= 32, "radius {r}: count {n}");
        assert!(n >= prev, "radius {r}: count {n} < {prev}");
        prev = n;
    }
    assert!(conic_quad_count_wang(512) > conic_quad_count_wang(32));
}
