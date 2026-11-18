//! The analytic edge walker (Skia `SkAnalyticEdge.cpp`).
//!
//! These tests pin the walker's per-row coverage against the signed Flutter
//! reference for rounded corners: the corner is rasterized from Skia's
//! quarter-circle conic rather than from a sampled polyline.

use opurom_render::aaa::round_rect_walker_edges;
use opurom_render::aaa_walker::walk;

const ONE: i64 = 1 << 16;
const WIDTH: i64 = 120;

/// Render one row of a rounded rectangle with the walker.
fn row_alpha(radius: i64, y: i64) -> Vec<i64> {
    let mut edges = round_rect_walker_edges(0, 0, WIDTH, WIDTH, radius);
    let mut mask = vec![0i64; WIDTH as usize];
    walk(
        &mut mask,
        0,
        &mut edges,
        (y * ONE) as i32,
        (y + 1) as i32,
        0,
        (WIDTH * ONE) as i32,
    );
    mask
}

#[test]
fn walker_matches_the_reference_top_row() {
    // Flutter's reference for a 16px corner: the top row ramps up from x=11.
    let row = row_alpha(16, 0);
    let expect: [(usize, i64); 7] = [
        (11, 78),
        (12, 163),
        (13, 206),
        (14, 226),
        (15, 246),
        (16, 255),
        (17, 255),
    ];
    for (x, want) in expect {
        assert!(
            (row[x] - want).abs() <= 2,
            "x={x}: got {} want {want}",
            row[x]
        );
    }
    assert_eq!(row[0], 0, "the corner starts covered at x=11");
}

#[test]
fn walker_covers_the_interior_of_the_row() {
    let row = row_alpha(16, 0);
    for x in 17..103 {
        assert_eq!(row[x as usize], 255, "interior pixel {x} is covered");
    }
}
