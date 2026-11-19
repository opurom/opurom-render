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

/// Flutter `StrokeJoin` (`painting.dart:1136-1178`): the shape of the joint
/// between two stroked segments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrokeJoin {
    Miter,
    Round,
    Bevel,
}

/// One corner where two stroked segments meet: incoming direction
/// `a -> v`, outgoing direction `v -> b`, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Corner {
    pub a: Point,
    pub v: Point,
    pub b: Point,
}

/// `StrokeJoin.miter` limit contract (`painting.dart:1142-1146`): a miter is
/// converted to a bevel when `strokeWidth / (distance between the outer
/// corners) > 4`, i.e. the miter ratio exceeds 4. Flutter's doc comment pins
/// the exact wording; Skia implements the same rule as a half-angle check
/// against `kDefaultMiterLimit == 4`.
pub const MITER_LIMIT_RATIO: i128 = 4;

/// The sin(theta/2) threshold (16.16 fixed point) below which the miter
/// ratio exceeds 4 and Flutter converts the join to a bevel.
const MITER_MIN_SIN_HALF: i128 = 16384;

/// Length of the miter extension from the joint vertex to the outer tip,
/// integer hypot (floor). For a corner with segment length `L` on both sides
/// and half-angle `theta` between them, Skia's `strokeJoin.miter` tip sits at
/// distance `(width/2) / sin(theta/2)` from the vertex along the bisector.
pub fn miter_tip(corner: &Corner, width: i64) -> Option<Point> {
    let la = segment_length(&corner.a, &corner.v);
    let lb = segment_length(&corner.v, &corner.b);
    if la == 0 || lb == 0 {
        return None;
    }
    let dx1 = (corner.v.x - corner.a.x) as i128;
    let dy1 = (corner.v.y - corner.a.y) as i128;
    let dx2 = (corner.b.x - corner.v.x) as i128;
    let dy2 = (corner.b.y - corner.v.y) as i128;
    // Unit directions (16.16 fixed point keeps the bisector arithmetic
    // exact enough for a deterministic integer tip).
    let u1x = (dx1 * 65536) / la as i128;
    let u1y = (dy1 * 65536) / la as i128;
    let u2x = (dx2 * 65536) / lb as i128;
    let u2y = (dy2 * 65536) / lb as i128;
    let bx = u1x + u2x;
    let by = u1y + u2y;
    // degenerate: opposite directions, the join is a straight line
    if bx == 0 && by == 0 {
        return None;
    }
    let blen = isqrt_i128(bx * bx + by * by);
    if blen == 0 {
        return None;
    }
    // Straight-through join (sum of unit vectors ~= one direction): the two
    // segments are collinear, no corner geometry exists.
    let half: i128 = (blen as i128) / 2;
    // sin(theta/2) = sqrt(1 - cos^2(theta/2)) where cos(theta/2) = |b|/2 in
    // 16.16. All terms are 16.16: 1.0 = 65536; cos^2 needs shifting back.
    let sin_half_sq: i128 = (1i128 << 16) - ((half * half) >> 16);
    if sin_half_sq <= 0 {
        return None;
    }
    // isqrt of a 16.16 value: scale by 2^16 before the integer sqrt.
    let sin_half: i128 = isqrt_i128(sin_half_sq << 16) as i128;
    // Miter ratio = 1/sin(theta/2). Flutter converts miter to bevel when
    // `strokeWidth / (distance between the outer corners) > 4`
    // (painting.dart:1142-1146), i.e. ratio > 4 <=> sin(theta/2) < 1/4
    // <=> sin_half(16.16) < 16384.
    if sin_half < MITER_MIN_SIN_HALF {
        return None; // miter exceeds the limit: caller falls back to bevel
    }
    // Tip distance = (width/2) / sin(theta/2), in pixels.
    let half_w = width as i128 / 2;
    let dist = (half_w * 65536) / sin_half;
    // Tip = vertex + dist * bisector_unit.
    let ux = (bx * 65536) / blen as i128;
    let uy = (by * 65536) / blen as i128;
    Some(Point {
        x: corner.v.x + (dist * ux >> 16) as i64,
        y: corner.v.y + (dist * uy >> 16) as i64,
    })
}

/// `StrokeJoin.miter` bevel fallback: the two outer offset corners of the
/// stroke, i.e. the endpoints of the flat edge that replaces the miter tip.
/// Each is the vertex offset by `width/2` along the outward normal of its
/// segment; which side is "outward" is fixed by the corner winding.
pub fn bevel_corners(corner: &Corner, width: i64) -> Option<(Point, Point)> {
    let la = segment_length(&corner.a, &corner.v);
    let lb = segment_length(&corner.v, &corner.b);
    if la == 0 || lb == 0 {
        return None;
    }
    let half = width as i128 / 2;
    // Outward normals for the join's outer side: incoming direction (0,-1)
    // yields (-1,0), outgoing (1,0) yields (0,+1) for the right-hand turn;
    // both are the negative of the miter bisector side, which is the outer
    // side of a CW polygon. The caller fixed the winding consistently.
    let n1x = ((corner.v.y - corner.a.y) as i128 * half) / la as i128;
    let n1y = (-(corner.v.x - corner.a.x) as i128 * half) / la as i128;
    let n2x = (-(corner.b.y - corner.v.y) as i128 * half) / lb as i128;
    let n2y = ((corner.b.x - corner.v.x) as i128 * half) / lb as i128;
    Some((
        Point {
            x: corner.v.x + n1x as i64,
            y: corner.v.y + n1y as i64,
        },
        Point {
            x: corner.v.x + n2x as i64,
            y: corner.v.y + n2y as i64,
        },
    ))
}

/// The join resolution for a corner: miter tip when the ratio is within the
/// limit, otherwise the bevel pair. Round joins are resolved by the caller
/// with the renderer's disc primitive at the vertex.
pub fn join_point(corner: &Corner, join: StrokeJoin, width: i64) -> JoinShape {
    match join {
        StrokeJoin::Miter => match miter_tip(corner, width) {
            Some(tip) => JoinShape::Miter(tip),
            None => JoinShape::Bevel,
        },
        StrokeJoin::Round => JoinShape::Round,
        StrokeJoin::Bevel => JoinShape::Bevel,
    }
}

/// Resolved join geometry: a miter tip point, or the bevel fallback whose
/// corners are `bevel_corners` (the caller renders the flat edge between
/// them).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinShape {
    Miter(Point),
    Bevel,
    Round,
}

#[cfg(test)]
mod tests {
    use super::{
        bevel_corners, cap_offset, join_point, miter_tip, segment_length, Corner, JoinShape, Point,
        StrokeCap, StrokeJoin,
    };

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

    #[test]
    fn right_angle_miter_tip_sits_on_the_bisector() {
        // 90-degree corner: theta/2 = 45 degrees, ratio = 1/sin(45) ~ 1.41
        // <= 4, so the miter is kept. Tip distance = (width/2)/sin(45).
        // For width 4: dist = 2/0.707 ~ 2.83, floor tip on the bisector.
        let c = Corner {
            a: Point { x: 0, y: 10 },
            v: Point { x: 0, y: 0 },
            b: Point { x: 10, y: 0 },
        };
        match join_point(&c, StrokeJoin::Miter, 4) {
            JoinShape::Miter(tip) => {
                // bisector of the two incoming directions (+x, -y) points
                // outward: positive x, negative y. Both components are the
                // same magnitude up to integer rounding of the fixed point.
                assert!(tip.x > 0 && tip.y < 0, "tip outward: {tip:?}");
                assert!(
                    (tip.x as i128 + tip.y as i128).abs() <= 1,
                    "tip on the bisector: {tip:?}"
                );
            }
            other => panic!("expected miter, got {other:?}"),
        }
    }

    #[test]
    fn shallow_corner_miter_falls_back_to_bevel() {
        // A shallow corner has a miter ratio > 4 (painting.dart:1142-1146:
        // strokeWidth / outer-corner distance > 4 converts to bevel). 10
        // degrees between the segments: ratio = 1/sin(5) ~ 11.5 > 4.
        let c = Corner {
            a: Point { x: 0, y: 0 },
            v: Point { x: 100, y: 0 },
            b: Point { x: 200, y: 17 }, // ~9.7 degrees deflection
        };
        assert_eq!(join_point(&c, StrokeJoin::Miter, 4), JoinShape::Bevel);
    }

    #[test]
    fn bevel_corners_are_offset_by_half_width() {
        let c = Corner {
            a: Point { x: 0, y: 0 },
            v: Point { x: 0, y: 0 },
            b: Point { x: 10, y: 0 },
        };
        // Degenerate zero-length incoming segment: no bevel geometry.
        assert_eq!(bevel_corners(&c, 4), None);
        let c = Corner {
            a: Point { x: 0, y: 10 },
            v: Point { x: 0, y: 0 },
            b: Point { x: 10, y: 0 },
        };
        let (p, q) = bevel_corners(&c, 4).unwrap();
        // Incoming direction (0,-10): right normal (dy,-dx) = (-10,0),
        // offset by half width 2 -> (-2, 0).
        assert_eq!(p, Point { x: -2, y: 0 });
        // Outgoing direction (10,0): right normal (-dy,dx) = (0,10),
        // offset by 2 -> (0, 2).
        assert_eq!(q, Point { x: 0, y: 2 });
    }

    #[test]
    fn straight_through_corner_has_no_miter() {
        let c = Corner {
            a: Point { x: 0, y: 0 },
            v: Point { x: 10, y: 0 },
            b: Point { x: 20, y: 0 },
        };
        assert_eq!(miter_tip(&c, 4), None);
        assert_eq!(join_point(&c, StrokeJoin::Miter, 4), JoinShape::Bevel);
    }

    #[test]
    fn round_and_explicit_bevel_resolve_to_their_own_shape() {
        let c = Corner {
            a: Point { x: 0, y: 10 },
            v: Point { x: 0, y: 0 },
            b: Point { x: 10, y: 0 },
        };
        assert_eq!(join_point(&c, StrokeJoin::Round, 4), JoinShape::Round);
        assert_eq!(join_point(&c, StrokeJoin::Bevel, 4), JoinShape::Bevel);
    }
}
