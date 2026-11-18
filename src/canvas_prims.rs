//! Minimal custom-draw primitives with Flutter 3.50.0 semantics, for IR nodes
//! that carry an explicit custom-draw payload in a future opt-in field.
//!
//! Source of truth: framework `ab2912035e16`, engine `371f77202a`,
//! `engine/src/flutter/lib/ui/painting.dart:1087-1129` (`StrokeCap`:
//! butt/round/square) and `:1136-1178` (`StrokeJoin`: miter/round/bevel);
//! `engine/src/flutter/lib/web_ui/lib/canvas.dart:76,91` (`drawLine`,
//! `drawPoints`). Geometry is integer-only and deterministic; the primitive
//! set is closed under the renderer's existing src-over blend and clip.
//! Skia's raster pipeline is unavailable in this checkout (submodule absent,
//! pinned `0e5d8d1be81b22f5529a41cf4810dd89b63527ba` via `DEPS:18,505`), so
//! cap/join geometry follows the framework contract above and marks the
//! Skia-level exact pixel rules as an unresolved gap.

/// Flutter `StrokeCap` (`painting.dart:1087-1129`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrokeCap {
    Butt,
    Round,
    Square,
}

/// One endpoint of a stroked segment: `(x, y)` in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point {
    pub x: i64,
    pub y: i64,
}

/// Total length of a segment, integer hypot (floor). Zero-length lines draw
/// only the cap region, mirroring `drawLine` with coincident endpoints.
pub fn segment_length(a: &Point, b: &Point) -> i64 {
    let dx = (b.x - a.x) as i128;
    let dy = (b.y - a.y) as i128;
    isqrt_i128(dx * dx + dy * dy)
}

/// `StrokeCap.square` extends the segment by half the stroke width at both
/// ends (`painting.dart:1117-1129`): the endpoint moves along the unit
/// direction by `width/2`. `butt` leaves the endpoint unchanged. The round
/// cap is not a pure segment operation and is resolved by the caller with
/// the renderer's disc primitive.
pub fn cap_offset(p: &Point, prev: &Point, cap: StrokeCap, width: i64) -> Point {
    match cap {
        StrokeCap::Butt => *p,
        StrokeCap::Square => {
            let len = segment_length(prev, p);
            if len == 0 {
                return *p;
            }
            let half = width / 2;
            let dx = (p.x - prev.x) as i128;
            let dy = (p.y - prev.y) as i128;
            let nx = (dx * half as i128) / len as i128;
            let ny = (dy * half as i128) / len as i128;
            Point {
                x: p.x + nx as i64,
                y: p.y + ny as i64,
            }
        }
        StrokeCap::Round => *p,
    }
}

/// Integer square root (floor) for the segment length.
fn isqrt_i128(v: i128) -> i64 {
    if v <= 0 {
        return 0;
    }
    let mut x = v as u128;
    let mut r: u128 = 0;
    let mut bit = 1u128 << 126;
    while bit > x {
        bit >>= 2;
    }
    while bit != 0 {
        if x >= r + bit {
            x -= r + bit;
            r = (r >> 1) + bit;
        } else {
            r >>= 1;
        }
        bit >>= 2;
    }
    r as i64
}

#[cfg(test)]
mod tests {
    use super::{cap_offset, segment_length, Point, StrokeCap};

    #[test]
    fn segment_length_matches_integer_hypot() {
        let a = Point { x: 0, y: 0 };
        let b = Point { x: 3, y: 4 };
        assert_eq!(segment_length(&a, &b), 5);
        assert_eq!(segment_length(&a, &a), 0);
    }

    #[test]
    fn square_cap_extends_by_half_stroke_width() {
        // painting.dart:1117-1129: square extends each contour by half the
        // stroke width. A 10px segment with width 6 extends by 3.
        let a = Point { x: 0, y: 0 };
        let b = Point { x: 10, y: 0 };
        let end = cap_offset(&b, &a, StrokeCap::Square, 6);
        assert_eq!(end, Point { x: 13, y: 0 });
        let start = cap_offset(&a, &b, StrokeCap::Square, 6);
        assert_eq!(start, Point { x: -3, y: 0 });
    }

    #[test]
    fn butt_and_round_caps_do_not_move_the_endpoint() {
        let a = Point { x: 0, y: 0 };
        let b = Point { x: 10, y: 0 };
        assert_eq!(cap_offset(&b, &a, StrokeCap::Butt, 6), b);
        assert_eq!(cap_offset(&b, &a, StrokeCap::Round, 6), b);
        // Zero-length segment cannot extend along a direction.
        assert_eq!(cap_offset(&a, &a, StrokeCap::Square, 6), a);
    }
}
