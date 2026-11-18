//! Skia's analytic edge walker for quadratic corners.
//!
//! Ported from `SkAnalyticEdge.cpp` and the row blit in `SkScan_AAAPath.cpp`.
//! The walker keeps one edge per path side and advances it with a biased
//! forward difference, so a corner is rasterized from the curve itself
//! rather than from a sampled polyline.
//!
//! Fixed-point conventions, matching Skia:
//! - `SkFixed` is 16.16, `SkFDot6` is 6 fractional bits.
//! - Arithmetic is 32-bit and wraps like Skia's `int32_t`.
//!
//! shortcut: validation-only kernel, not wired into the default raster path until the zero-length top-edge blocker is resolved

use crate::aaa_inverse::quick_inverse;

/// Skia's `SK_Fixed1`.
pub const FIXED_ONE: i32 = 1 << 16;

/// Skia's `kDefaultAccuracy` (`SkAnalyticEdge.h:54`).
pub const DEFAULT_ACCURACY: u32 = 2;

/// Skia's `MAX_COEFF_SHIFT` (`SkAnalyticEdge.cpp:275`).
pub const MAX_COEFF_SHIFT: u32 = 6;

/// Skia's `SK_MaxS32`, used as the "no slope" sentinel.
pub const MAX_S32: i32 = i32::MAX;

/// Skia's `kInverseTableSize`.
const INVERSE_TABLE_SIZE: i32 = 1024;

/// Skia `SkFixedMul` (`(a * b) >> 16`).
fn fixed_mul(a: i32, b: i32) -> i32 {
    ((a as i64 * b as i64) >> 16) as i32
}

/// Skia `SkFixedRoundToFixed` (`((v + 32768) >> 16) << 16`).
fn fixed_round(v: i32) -> i32 {
    ((v + (FIXED_ONE >> 1)) >> 16) << 16
}

/// Skia `SkFixedToFDot6` (`>> 10`).
fn fixed_to_fdot6(value: i32) -> i32 {
    value >> 10
}

/// Skia `SkFDot6ToFixed` (`<< 10`).
fn fdot6_to_fixed(value: i32) -> i32 {
    value.wrapping_shl(10)
}

/// Skia `SkFDot6ToFixedDiv2` (`SkAnalyticEdge.cpp:318-322`): the fixed value
/// of half the input, kept as a shift so no bits are dropped.
fn fdot6_to_fixed_div2(value: i32) -> i32 {
    value.wrapping_shl(16 - 6 - 1)
}

/// Skia `SkFDot6Round` (`+ 32 >> 6`).
fn fdot6_round(value: i32) -> i32 {
    (value + 32) >> 6
}

/// Skia `SnapY` (`SkAnalyticEdge.h:56-60`): snap to quarter-pixel steps.
fn snap_y(y: i32) -> i32 {
    let accuracy = DEFAULT_ACCURACY;
    let half = (FIXED_ONE >> (accuracy + 1)) as u32;
    ((((y as u32).wrapping_add(half)) >> (16 - accuracy)) << (16 - accuracy)) as i32
}

/// Quantize a 16.16 coordinate to FDot6 the way Skia's `int(float)` does:
/// the pixel value is scaled by 256 and truncated towards zero.
fn scale_fdot6(value: i32) -> i32 {
    value / 256
}

/// Skia `quick_div` (`SkAnalyticEdge.cpp:136-155`): a fixed-point division
/// that uses the reciprocal table when the operands allow it.
pub fn quick_div(a: i32, b: i32) -> i32 {
    const MIN_BITS: u32 = 3;
    const MAX_BITS: u32 = 31;
    let max_abs_a = 1i32 << (MAX_BITS - (22 - MIN_BITS));
    let abs_a = a.saturating_abs();
    let abs_b = b.saturating_abs();
    if abs_b >= (1 << MIN_BITS) && abs_b < INVERSE_TABLE_SIZE && abs_a < max_abs_a {
        a.wrapping_mul(quick_inverse(b)) >> 6
    } else if b == 0 {
        MAX_S32
    } else {
        a / b
    }
}

/// Skia `cheap_distance` (`SkAnalyticEdge.cpp:277-288`).
fn cheap_distance(dx: i32, dy: i32) -> i32 {
    let mut dx = dx.saturating_abs();
    let dy = dy.saturating_abs();
    if dx > dy {
        dx += dy >> 1;
    } else {
        dx = dy + (dx >> 1);
    }
    dx
}

/// Skia `diff_to_shift` (`SkAnalyticEdge.cpp:290-304`): the subdivision
/// exponent that keeps the curve within the requested tolerance.
fn diff_to_shift(dx: i32, dy: i32, shift_aa: u32) -> u32 {
    let dist = cheap_distance(dx, dy);
    let dist = (dist + (1 << (2 + shift_aa))) >> (3 + shift_aa);
    let bits = 32 - dist.leading_zeros() as i32;
    (bits >> 1) as u32
}

/// A straight path side.
#[derive(Clone, Copy, Debug)]
pub struct LineEdge {
    /// Current x at `y`.
    pub x: i32,
    /// dx/dy.
    pub dx: i32,
    /// The x where this edge starts.
    pub upper_x: i32,
    /// Current y.
    pub y: i32,
    /// The y where this edge starts.
    pub upper_y: i32,
    /// The y where this edge ends.
    pub lower_y: i32,
    /// |dy/dx|, used when blitting the row.
    pub dy: i32,
}

impl LineEdge {
    /// Skia `SkAnalyticEdge::setLine` (`SkAnalyticEdge.cpp:157-207`).
    pub fn new(p0: (i32, i32), p1: (i32, i32)) -> Option<LineEdge> {
        let accuracy = DEFAULT_ACCURACY;
        let mut x0 = fdot6_to_fixed(scale_fdot6(p0.0)) >> accuracy;
        let mut y0 = snap_y(fdot6_to_fixed(scale_fdot6(p0.1)) >> accuracy);
        let mut x1 = fdot6_to_fixed(scale_fdot6(p1.0)) >> accuracy;
        let mut y1 = snap_y(fdot6_to_fixed(scale_fdot6(p1.1)) >> accuracy);
        if y0 > y1 {
            std::mem::swap(&mut x0, &mut x1);
            std::mem::swap(&mut y0, &mut y1);
        }
        let dy = fixed_to_fdot6(y1 - y0);
        if dy == 0 {
            return None;
        }
        let dx = fixed_to_fdot6(x1 - x0);
        let slope = quick_div(dx, dy);
        let abs_slope = slope.saturating_abs();
        let fdy = if dx == 0 || slope == 0 {
            MAX_S32
        } else if abs_slope < INVERSE_TABLE_SIZE {
            quick_inverse(abs_slope)
        } else {
            quick_div(dy, dx).saturating_abs()
        };
        Some(LineEdge {
            x: x0,
            dx: slope,
            upper_x: x0,
            y: y0,
            upper_y: y0,
            lower_y: y1,
            dy: fdy,
        })
    }

    /// Skia `goY` (`SkAnalyticEdge.h:63-73`).
    pub fn go_y(&mut self, y: i32) {
        if y == self.y.wrapping_add(FIXED_ONE) {
            self.x = self.x.wrapping_add(self.dx);
            self.y = y;
        } else if y != self.y {
            self.x = self
                .upper_x
                .wrapping_add(fixed_mul(self.dx, y - self.upper_y));
            self.y = y;
        }
    }
}

/// A quadratic path side that advances through its own sub-segments.
#[derive(Clone, Copy, Debug)]
pub struct QuadEdge {
    /// Current x at `y`.
    pub x: i32,
    /// dx/dy of the current sub-segment.
    pub dx: i32,
    /// The x where the current sub-segment starts.
    pub upper_x: i32,
    /// Current y.
    pub y: i32,
    /// The y where the current sub-segment starts.
    pub upper_y: i32,
    /// The y where the current sub-segment ends.
    pub lower_y: i32,
    /// |dy/dx|, used when blitting the row.
    pub dy: i32,
    /// Remaining sub-segments; zero when the curve is exhausted.
    pub curve_count: i32,
    /// The parametric shift used by the forward difference.
    pub curve_shift: u32,
    qx: i32,
    qy: i32,
    qdx: i32,
    qdy: i32,
    qddx: i32,
    qddy: i32,
    last_x: i32,
    last_y: i32,
    snapped_x: i32,
    snapped_y: i32,
}

impl QuadEdge {
    /// Skia `setQuadraticWithoutUpdate` (`SkAnalyticEdge.cpp:324-424`) followed
    /// by `setQuadratic` (:426-447).
    ///
    /// `pts` are the three control points in 16.16. The points are scaled up
    /// by the accuracy shift before the coefficients are derived, exactly as
    /// Skia does, so the biased forward difference reproduces its rounding.
    pub fn new(pts: [(i32, i32); 3]) -> Option<QuadEdge> {
        let shift = DEFAULT_ACCURACY;
        let mut x0 = scale_fdot6(pts[0].0);
        let mut y0 = scale_fdot6(pts[0].1);
        let x1 = scale_fdot6(pts[1].0);
        let y1 = scale_fdot6(pts[1].1);
        let mut x2 = scale_fdot6(pts[2].0);
        let mut y2 = scale_fdot6(pts[2].1);

        if y0 > y2 {
            std::mem::swap(&mut x0, &mut x2);
            std::mem::swap(&mut y0, &mut y2);
        }
        if fdot6_round(y0) == fdot6_round(y2) {
            return None;
        }

        // Number of sub-segments, from the curve's deviation.
        let dx = (x1.wrapping_shl(1) - x0 - x2) >> 2;
        let dy = (y1.wrapping_shl(1) - y0 - y2) >> 2;
        let mut shift = diff_to_shift(dx, dy, shift);
        if shift == 0 {
            shift = 1;
        } else if shift > MAX_COEFF_SHIFT {
            shift = MAX_COEFF_SHIFT;
        }

        // The coefficients are stored at half their value; the forward
        // difference applies the 2x scale.
        let a = fdot6_to_fixed_div2(x0 - x1 - x1 + x2);
        let b = fdot6_to_fixed(x1 - x0);
        let qx = fdot6_to_fixed(x0);
        let qdx = b.wrapping_add(a >> shift);
        let qddx = a >> (shift - 1);

        let a = fdot6_to_fixed_div2(y0 - y1 - y1 + y2);
        let b = fdot6_to_fixed(y1 - y0);
        let qy = fdot6_to_fixed(y0);
        let qdy = b.wrapping_add(a >> shift);
        let qddy = a >> (shift - 1);

        let last_x = fdot6_to_fixed(x2);
        let last_y = fdot6_to_fixed(y2);

        let mut edge = QuadEdge {
            x: 0,
            dx: 0,
            upper_x: 0,
            y: 0,
            upper_y: 0,
            lower_y: 0,
            dy: MAX_S32,
            curve_count: 1 << shift,
            curve_shift: shift - 1,
            qx: qx >> DEFAULT_ACCURACY,
            qy: qy >> DEFAULT_ACCURACY,
            qdx: qdx >> DEFAULT_ACCURACY,
            qdy: qdy >> DEFAULT_ACCURACY,
            qddx: qddx >> DEFAULT_ACCURACY,
            qddy: qddy >> DEFAULT_ACCURACY,
            last_x: last_x >> DEFAULT_ACCURACY,
            last_y: last_y >> DEFAULT_ACCURACY,
            snapped_x: 0,
            snapped_y: 0,
        };
        edge.qy = snap_y(edge.qy);
        edge.last_y = snap_y(edge.last_y);
        edge.snapped_x = edge.qx;
        edge.snapped_y = edge.qy;
        if !edge.update_quadratic() {
            return None;
        }
        Some(edge)
    }

    /// Skia `updateQuadratic` (`SkAnalyticEdge.cpp:449-511`): advance the
    /// curve to its next sub-segment and install it as the current line.
    #[allow(unused_assignments)]
    fn update_quadratic(&mut self) -> bool {
        let mut success = false;
        let mut count = self.curve_count;
        let mut old_x = self.qx;
        let mut old_y = self.qy;
        let mut dx = self.qdx;
        let mut dy = self.qdy;
        let shift = self.curve_shift;
        let mut new_x = self.qx;
        let mut new_y = self.qy;
        let mut new_snapped_x = self.snapped_x;
        let mut new_snapped_y = self.snapped_y;

        loop {
            if count - 1 > 0 {
                count -= 1;
                new_x = old_x.wrapping_add(dx >> shift);
                new_y = old_y.wrapping_add(dy >> shift);
                // Only snap when the edge is steep enough and not too slanted.
                if (dy >> shift).saturating_abs() >= FIXED_ONE * 2
                    && (dy.saturating_abs() as i64).wrapping_shl(6) > dx.saturating_abs() as i64
                {
                    let diff_y = fixed_to_fdot6(new_y - self.snapped_y);
                    let slope = if diff_y != 0 {
                        quick_div(fixed_to_fdot6(new_x - self.snapped_x), diff_y)
                    } else {
                        MAX_S32
                    };
                    new_snapped_y = self.last_y.min(fixed_round(new_y));
                    new_snapped_x = new_x.wrapping_sub(fixed_mul(slope, new_y - new_snapped_y));
                    if slope < MAX_S32
                        && self.update_line(
                            self.snapped_x,
                            self.snapped_y,
                            new_snapped_x,
                            new_snapped_y,
                            slope,
                        )
                    {
                        success = true;
                    }
                } else {
                    new_snapped_y = self.last_y.min(snap_y(new_y));
                    new_snapped_x = new_x;
                    let diff_y = fixed_to_fdot6(new_snapped_y - self.snapped_y);
                    let slope = if diff_y != 0 {
                        quick_div(fixed_to_fdot6(new_x - self.snapped_x), diff_y)
                    } else {
                        MAX_S32
                    };
                    if slope < MAX_S32
                        && self.update_line(
                            self.snapped_x,
                            self.snapped_y,
                            new_snapped_x,
                            new_snapped_y,
                            slope,
                        )
                    {
                        success = true;
                    }
                }
                dx = dx.wrapping_add(self.qddx);
                dy = dy.wrapping_add(self.qddy);
            } else {
                count = 0;
                new_x = self.last_x;
                new_y = self.last_y;
                new_snapped_y = new_y;
                new_snapped_x = new_x;
                let diff_y = fixed_to_fdot6(new_y - self.snapped_y);
                let slope = if diff_y != 0 {
                    quick_div(fixed_to_fdot6(new_x - self.snapped_x), diff_y)
                } else {
                    MAX_S32
                };
                if slope < MAX_S32
                    && self.update_line(
                        self.snapped_x,
                        self.snapped_y,
                        new_snapped_x,
                        new_snapped_y,
                        slope,
                    )
                {
                    success = true;
                }
            }
            old_x = new_x;
            old_y = new_y;
            if !(count > 0 && !success) {
                break;
            }
        }

        self.qx = new_x;
        self.qy = new_y;
        self.qdx = dx;
        self.qdy = dy;
        self.snapped_x = new_snapped_x;
        self.snapped_y = new_snapped_y;
        self.curve_count = count;
        success
    }

    /// Skia `updateLine` (`SkAnalyticEdge.cpp:216-258`): install the current
    /// sub-segment as the edge's line.
    fn update_line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, slope: i32) -> bool {
        let (mut x0, mut y0, mut x1, mut y1) = (x0, y0, x1, y1);
        if y0 > y1 {
            std::mem::swap(&mut x0, &mut x1);
            std::mem::swap(&mut y0, &mut y1);
        }
        let dx = fixed_to_fdot6(x1 - x0);
        let dy = fixed_to_fdot6(y1 - y0);
        if dy == 0 {
            return false;
        }
        let abs_slope = fixed_to_fdot6(slope).saturating_abs();
        self.x = x0;
        self.dx = slope;
        self.upper_x = x0;
        self.y = y0;
        self.upper_y = y0;
        self.lower_y = y1;
        self.dy = if dx == 0 || slope == 0 {
            MAX_S32
        } else if abs_slope < INVERSE_TABLE_SIZE {
            quick_inverse(abs_slope)
        } else {
            quick_div(dy, dx).saturating_abs()
        };
        true
    }

    /// Skia `goY` (`SkAnalyticEdge.h:63-73`).
    pub fn go_y(&mut self, y: i32) {
        if y == self.y.wrapping_add(FIXED_ONE) {
            self.x = self.x.wrapping_add(self.dx);
            self.y = y;
        } else if y != self.y {
            self.x = self
                .upper_x
                .wrapping_add(fixed_mul(self.dx, y - self.upper_y));
            self.y = y;
        }
    }
}

/// One side of the path: either a straight run or a quadratic curve.
#[derive(Clone, Copy, Debug)]
pub enum Edge {
    Line(LineEdge),
    Quad(QuadEdge),
}

impl Edge {
    /// Set the edge's current position.
    pub fn set_position(&mut self, x: i32, y: i32) {
        match self {
            Edge::Line(e) => {
                e.x = x;
                e.y = y;
            }
            Edge::Quad(e) => {
                e.x = x;
                e.y = y;
            }
        }
    }

    /// True once the edge has no more sub-segments to advance to.
    pub fn done(&self) -> bool {
        match self {
            Edge::Line(_) => true,
            Edge::Quad(e) => e.curve_count <= 0,
        }
    }
}

impl Edge {
    pub fn x(&self) -> i32 {
        match self {
            Edge::Line(e) => e.x,
            Edge::Quad(e) => e.x,
        }
    }

    pub fn dx(&self) -> i32 {
        match self {
            Edge::Line(e) => e.dx,
            Edge::Quad(e) => e.dx,
        }
    }

    pub fn upper_y(&self) -> i32 {
        match self {
            Edge::Line(e) => e.upper_y,
            Edge::Quad(e) => e.upper_y,
        }
    }

    pub fn lower_y(&self) -> i32 {
        match self {
            Edge::Line(e) => e.lower_y,
            Edge::Quad(e) => e.lower_y,
        }
    }

    pub fn dy(&self) -> i32 {
        match self {
            Edge::Line(e) => e.dy,
            Edge::Quad(e) => e.dy,
        }
    }

    /// The x where the edge starts.
    pub fn upper_x(&self) -> i32 {
        match self {
            Edge::Line(e) => e.upper_x,
            Edge::Quad(e) => e.upper_x,
        }
    }

    /// The curve coefficients `is_smooth_enough` inspects, or `None` for a
    /// straight side.
    fn smooth_terms(&self) -> Option<(i32, i32, i32, i32, u32)> {
        match self {
            Edge::Line(_) => None,
            Edge::Quad(e) => Some((e.qdx, e.qddx, e.qdy, e.qddy, e.curve_shift)),
        }
    }

    pub fn go_y(&mut self, y: i32) {
        match self {
            Edge::Line(e) => e.go_y(y),
            Edge::Quad(e) => e.go_y(y),
        }
    }

    /// Advance to the next sub-segment. Returns false when the edge is done.
    pub fn update(&mut self) -> bool {
        match self {
            Edge::Line(_) => false,
            Edge::Quad(e) => e.update_quadratic(),
        }
    }
}

/// Skia `is_smooth_enough` for one edge (`SkScan_AAAPath.cpp:991-1009`).
///
/// A curved side is smooth when its second derivative is small against its
/// first and the side still advances by a full pixel; a straight side is
/// smooth when its slope matches the next side's.
fn edge_is_smooth(this: &Edge, next: &Edge) -> bool {
    match this.smooth_terms() {
        Some((qdx, qddx, qdy, qddy, curve_shift)) => {
            qdx.saturating_abs() >> 1 >= qddx.saturating_abs()
                && qdy.saturating_abs() >> 1 >= qddy.saturating_abs()
                && (qdy - qddy) >> curve_shift >= FIXED_ONE
        }
        None => {
            let delta = next.dx().saturating_sub(this.dx()).saturating_abs();
            delta <= FIXED_ONE && next.lower_y() - next.upper_y() >= FIXED_ONE
        }
    }
}

/// Skia `is_smooth_enough` for the two sides of a row
/// (`SkScan_AAAPath.cpp:1013-1036`).
///
/// When both sides change within the row, the two edges that follow decide
/// whether the row's boundary can be carried to the next whole pixel.
fn row_is_smooth(
    left: &Edge,
    rite: &Edge,
    curr: Option<usize>,
    edges: &[Edge],
    stop_y: i32,
) -> bool {
    let Some(curr) = curr else {
        return false;
    };
    let Some(curr_edge) = edges.get(curr) else {
        return false;
    };
    if curr_edge.upper_y() >= stop_y << 16 {
        return false;
    }
    if left.lower_y() + FIXED_ONE < rite.lower_y() {
        return edge_is_smooth(left, curr_edge);
    }
    if left.lower_y() > rite.lower_y() + FIXED_ONE {
        return edge_is_smooth(rite, curr_edge);
    }
    // Both sides change: the next two edges decide.
    let Some(next_curr) = edges.get(curr + 1) else {
        return false;
    };
    if next_curr.upper_y() >= stop_y << 16 {
        return false;
    }
    let (first, second) = if next_curr.upper_x() < curr_edge.upper_x() {
        (next_curr, curr_edge)
    } else {
        (curr_edge, next_curr)
    };
    edge_is_smooth(left, first) && edge_is_smooth(rite, second)
}

/// Rasterize a convex path whose sides are analytic edges, one pixel row at a
/// time.
///
/// This is Skia's `aaa_walk_convex_edges` (`SkScan_AAAPath.cpp:1038-1305`).
/// A pixel row can contain several sub-segments of a curved side, so the row
/// is split the way Skia splits it: an optional partial row at the top, the
/// full rows in the middle, and a partial row at the bottom. Each part is
/// blitted as one trapezoid whose full alpha is the part's height.
pub fn walk(
    mask: &mut [i64],
    origin: i64,
    edges: &mut [Edge],
    start_y: i32,
    stop_y: i32,
    left_bound: i32,
    right_bound: i32,
) {
    // Skia keeps the edges in a list sorted by their upper y, so the next edge
    // to activate is always the following one.
    edges.sort_by_key(|e| (e.upper_y(), e.upper_x()));
    let mut cursor = 0usize;
    let mut y = start_y;
    while y < stop_y << 16 {
        // Advance the sides that ended above this row to their next
        // sub-segment (Skia :1056-1073).
        for edge in edges.iter_mut() {
            while edge.lower_y() <= y && !edge.done() {
                if !edge.update() {
                    break;
                }
            }
        }

        // Order the sides: the leftmost crossing is the left edge.
        let mut left: Option<usize> = None;
        let mut right: Option<usize> = None;
        for (i, edge) in edges.iter().enumerate() {
            // A quad whose forward differences are exhausted (curve_count==0)
            // still has its final line segment active until y passes lower_y;
            // done() only means "no further sub-segment", not "no coverage".
            if edge.upper_y() > y || edge.lower_y() <= y {
                continue;
            }
            match left {
                None => left = Some(i),
                Some(l) => {
                    if edge.x() < edges[l].x() {
                        left = Some(i);
                    }
                }
            }
            match right {
                None => right = Some(i),
                Some(r) => {
                    if edge.x() > edges[r].x() {
                        right = Some(i);
                    }
                }
            }
        }
        let (Some(mut l), Some(mut r)) = (left, right) else {
            break;
        };
        if l == r {
            break;
        }
        edges[l].go_y(y);
        edges[r].go_y(y);
        if edges[l].x() > edges[r].x()
            || (edges[l].x() == edges[r].x() && edges[l].dx() > edges[r].dx())
        {
            edges.swap(l, r);
            std::mem::swap(&mut l, &mut r);
        }

        // The cursor points at the next edge that has not started yet.
        while cursor < edges.len() && edges[cursor].upper_y() <= y {
            cursor += 1;
        }
        // The row ends where the shorter side ends, and a smooth row carries
        // its boundary to the next whole pixel (Skia :1093-1097).
        let mut local_bot = edges[l].lower_y().min(edges[r].lower_y()).min(stop_y << 16);
        if row_is_smooth(&edges[l], &edges[r], Some(cursor), edges, stop_y) {
            local_bot = ceil_to_fixed(local_bot);
        }

        let mut left_x = edges[l].x().max(left_bound);
        let d_left = edges[l].dx();
        let mut right_x = edges[r].x().min(right_bound);
        let d_right = edges[r].dx();
        let l_dy = edges[l].dy();
        let r_dy = edges[r].dy();

        if (d_left | d_right) == 0 {
            // Both sides are vertical: the row is a rectangle (Skia :1103-1187).
            let full_left = ceil_to_int(left_x);
            let full_right = floor_to_int(right_x);
            let partial_left = (full_left << 16) - left_x;
            let partial_right = right_x - (full_right << 16);
            let full_top = ceil_to_int(y);
            let full_bot = floor_to_int(local_bot);
            let mut partial_top = (full_top << 16) - y;
            let mut partial_bot = local_bot - (full_bot << 16);
            if full_top > full_bot {
                partial_top -= FIXED_ONE - partial_bot;
                partial_bot = 0;
            }
            if full_right >= full_left {
                if partial_top > 0 {
                    let a = fixed_to_alpha(fixed_mul(partial_top, partial_left));
                    crate::aaa::add_mask(mask, origin + full_left as i64 - 1, a);
                    let a = fixed_to_alpha(partial_top);
                    for x in full_left..full_right {
                        crate::aaa::add_mask(mask, origin + x as i64, a);
                    }
                    if partial_right > 0 {
                        let a = fixed_to_alpha(fixed_mul(partial_top, partial_right));
                        crate::aaa::add_mask(mask, origin + full_right as i64, a);
                    }
                }
                if full_bot > full_top {
                    let a = fixed_to_alpha(partial_left);
                    crate::aaa::add_mask(mask, origin + full_left as i64 - 1, a);
                    for x in full_left..full_right {
                        crate::aaa::add_mask(mask, origin + x as i64, 255);
                    }
                    let a = fixed_to_alpha(partial_right);
                    crate::aaa::add_mask(mask, origin + full_right as i64, a);
                }
                if partial_bot > 0 {
                    let a = fixed_to_alpha(fixed_mul(partial_bot, partial_left));
                    crate::aaa::add_mask(mask, origin + full_left as i64 - 1, a);
                    let a = fixed_to_alpha(partial_bot);
                    for x in full_left..full_right {
                        crate::aaa::add_mask(mask, origin + x as i64, a);
                    }
                    if partial_right > 0 {
                        let a = fixed_to_alpha(fixed_mul(partial_bot, partial_right));
                        crate::aaa::add_mask(mask, origin + full_right as i64, a);
                    }
                }
            }
            y = local_bot;
            edges[l].set_position(left_x, y);
            edges[r].set_position(right_x, y);
            continue;
        }

        // Snap the x values to a 1/16 grid (Skia :1194-1198).
        let snap_digit = FIXED_ONE >> 4;
        let snap_half = snap_digit >> 1;
        let snap_mask = !(snap_digit - 1);
        left_x = left_x.wrapping_add(snap_half);
        right_x = right_x.wrapping_add(snap_half);

        let mut count = ceil_to_int(local_bot) - floor_to_int(y);
        if count > 1 {
            // A partial row at the top of this pixel row.
            if (y & !0xFFFF) != y {
                count -= 1;
                let next_y = ceil_to_fixed(y + 1);
                let d_y = next_y - y;
                let next_left = left_x.wrapping_add(fixed_mul(d_left, d_y));
                let next_right = right_x.wrapping_add(fixed_mul(d_right, d_y));
                crate::aaa::blit_trapezoid_row(
                    mask,
                    origin,
                    (left_x & snap_mask) as i64,
                    (right_x & snap_mask) as i64,
                    (next_left & snap_mask) as i64,
                    (next_right & snap_mask) as i64,
                    l_dy as i64,
                    r_dy as i64,
                    full_alpha(d_y),
                );
                left_x = next_left;
                right_x = next_right;
                y = next_y;
            }
            // Full rows in the middle.
            while count > 1 {
                count -= 1;
                let next_left = left_x.wrapping_add(d_left);
                let next_right = right_x.wrapping_add(d_right);
                crate::aaa::blit_trapezoid_row(
                    mask,
                    origin,
                    (left_x & snap_mask) as i64,
                    (right_x & snap_mask) as i64,
                    (next_left & snap_mask) as i64,
                    (next_right & snap_mask) as i64,
                    l_dy as i64,
                    r_dy as i64,
                    255,
                );
                left_x = next_left;
                right_x = next_right;
                y = y.wrapping_add(FIXED_ONE);
            }
        }

        // The partial row at the bottom.
        let d_y = local_bot - y;
        let next_left = left_x
            .wrapping_add(fixed_mul(d_left, d_y))
            .max(left_bound + snap_half);
        let next_right = right_x
            .wrapping_add(fixed_mul(d_right, d_y))
            .min(right_bound + snap_half);
        crate::aaa::blit_trapezoid_row(
            mask,
            origin,
            (left_x & snap_mask) as i64,
            (right_x & snap_mask) as i64,
            (next_left & snap_mask) as i64,
            (next_right & snap_mask) as i64,
            l_dy as i64,
            r_dy as i64,
            full_alpha(d_y),
        );
        left_x = next_left;
        right_x = next_right;
        y = local_bot;

        edges[l].set_position(left_x.wrapping_sub(snap_half), y);
        edges[r].set_position(right_x.wrapping_sub(snap_half), y);
    }
}

/// Skia `get_partial_alpha(0xFF, height)` (`SkScan_AAAPath.cpp:561-563`):
/// the row height as an alpha.
fn full_alpha(height: i32) -> i64 {
    (((255i64 * height as i64) + (FIXED_ONE as i64 >> 1)) >> 16) as i64
}

/// Skia `fixed_to_alpha` (`SkScan_AAAPath.cpp:572-575`).
fn fixed_to_alpha(value: i32) -> i64 {
    full_alpha(value)
}

/// Skia `SkFixedFloorToInt`.
fn floor_to_int(value: i32) -> i32 {
    value >> 16
}

/// Skia `SkFixedCeilToInt`.
fn ceil_to_int(value: i32) -> i32 {
    (value + FIXED_ONE - 1) >> 16
}

/// Skia `SkFixedCeilToFixed`.
fn ceil_to_fixed(value: i32) -> i32 {
    (value + FIXED_ONE - 1) & !0xFFFF
}
