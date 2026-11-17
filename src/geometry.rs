//! Rounded-rect corner geometry in pure integer arithmetic.
//!
//! Direct integer ports of two Impeller rules, used later by the analytic
//! (AAA) corner coverage:
//! - `RoundingRadii::Scaled` — impeller/geometry/rounding_radii.cc:26-66
//! - `CornerContains` — impeller/geometry/round_rect.cc:30-64
//!
//! Every reduction uses floor division, which preserves the core invariant:
//! adjacent radii pairs never exceed their rectangle dimension.

/// Scale fixed point for the global radii reduction. Floor rounding under this
/// scale keeps `r1' + r2' <= dimension` because `floor(a) + floor(b) <= floor(a+b)`.
const SCALE_ONE: i64 = 1 << 16;

/// Clamp for corner radii used in containment tests: geometrically no radius
/// exceeds the canvas, and bounding the inputs keeps the i128 squares below
/// far from overflow while covering every real layout.
const RADIUS_CLAMP: i64 = 1 << 21;

/// One corner's elliptical radii in logical pixels. `x` and `y` may differ
/// (elliptical corner); `0` in either component makes the corner flat.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct CornerRadii {
    pub x: i64,
    pub y: i64,
}

impl CornerRadii {
    /// Impeller `Size::IsEmpty` (impeller/geometry/size.h:123): empty unless
    /// BOTH components are positive. Such a corner normalizes to fully flat.
    pub fn is_empty(self) -> bool {
        !(self.x > 0 && self.y > 0)
    }
}

/// The four corner radii of a rounded rectangle, unnormalized.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct RoundingRadii {
    pub top_left: CornerRadii,
    pub top_right: CornerRadii,
    pub bottom_left: CornerRadii,
    pub bottom_right: CornerRadii,
}

impl RoundingRadii {
    pub fn all(radius: i64) -> Self {
        let c = CornerRadii {
            x: radius,
            y: radius,
        };
        RoundingRadii {
            top_left: c,
            top_right: c,
            bottom_left: c,
            bottom_right: c,
        }
    }

    pub fn is_all_empty(self) -> bool {
        self.top_left.is_empty()
            && self.top_right.is_empty()
            && self.bottom_left.is_empty()
            && self.bottom_right.is_empty()
    }

    /// Integer port of `RoundingRadii::Scaled(bounds)` (rounding_radii.cc:26).
    /// Normalizes flat/negative corners to zero, then applies one global
    /// floor-scaled factor so no pair of adjacent radii exceeds the rectangle
    /// edge it sits on (AdjustScale, rounding_radii.cc:15-24).
    pub fn scaled(self, w: i64, h: i64) -> Self {
        if w <= 0 || h <= 0 || self.is_all_empty() {
            return RoundingRadii::default();
        }
        let mut r = self;
        for c in [
            &mut r.top_left,
            &mut r.top_right,
            &mut r.bottom_left,
            &mut r.bottom_right,
        ] {
            if c.is_empty() {
                *c = CornerRadii::default();
            }
        }
        // Global scale = min over the four edge constraints of dim/(r1+r2),
        // kept as a SCALE_ONE fixed-point fraction; floor keeps the invariant.
        let mut scale_fp = i128::MAX;
        let mut constraint = |a: i64, b: i64, dim: i64| {
            let sum = a as i128 + b as i128;
            if sum > dim as i128 {
                let fp = (dim as i128) * SCALE_ONE as i128;
                scale_fp = scale_fp.min(fp / sum);
            }
        };
        constraint(r.top_left.x, r.top_right.x, w);
        constraint(r.bottom_left.x, r.bottom_right.x, w);
        constraint(r.top_left.y, r.bottom_left.y, h);
        constraint(r.top_right.y, r.bottom_right.y, h);
        if scale_fp == i128::MAX {
            return r;
        }
        let shrink = |c: &mut CornerRadii| {
            c.x = ((c.x as i128 * scale_fp) / SCALE_ONE as i128) as i64;
            c.y = ((c.y as i128 * scale_fp) / SCALE_ONE as i128) as i64;
        };
        shrink(&mut r.top_left);
        shrink(&mut r.top_right);
        shrink(&mut r.bottom_left);
        shrink(&mut r.bottom_right);
        r
    }
}

/// Integer port of `CornerContains` (round_rect.cc:30-64).
///
/// `corner` is the rectangle corner in absolute coordinates, `(dx, dy)` is the
/// sign vector pointing from the rect center toward that corner (-1/-1 for the
/// upper left). Points outside the corner's curved quadrant are inside by this
/// corner's rule; inside the quadrant the elliptical arc test is the unit
/// circle `x^2 + y^2 <= 1` written with cross-multiplied integers:
/// `(qx*ry)^2 + (qy*rx)^2 <= (rx*ry)^2`.
///
/// Call with radii already passed through [`RoundingRadii::scaled`] for the
/// rectangle's bounds.
pub fn corner_contains(
    px: i64,
    py: i64,
    corner: (i64, i64),
    dir: (i64, i64),
    radii: CornerRadii,
) -> bool {
    let rx = radii.x.clamp(0, RADIUS_CLAMP);
    let ry = radii.y.clamp(0, RADIUS_CLAMP);
    if rx == 0 || ry == 0 {
        // Flat corner: containment here reduces to the bounds check the caller
        // already performed (round_rect.cc:35-39).
        return true;
    }
    // Positive distance from the corner toward the point (round_rect.cc:42).
    let crx = (corner.0 - px) * dir.0;
    let cry = (corner.1 - py) * dir.1;
    // Distance from the arc center, measured from the corner inward (:51).
    let qx = rx - crx;
    let qy = ry - cry;
    if qx <= 0 || qy <= 0 {
        // Not in this corner's curved quadrant (:52-55).
        return true;
    }
    let (qx, qy) = (qx as i128, qy as i128);
    let (rx, ry) = (rx as i128, ry as i128);
    let a = qx * ry;
    let b = qy * rx;
    let rr = rx * ry;
    a * a + b * b <= rr * rr
}

/// `RoundRect::Contains` for one scaled radii set: bounds plus the four
/// corner tests (round_rect.cc:73-88).
pub fn round_rect_contains(
    px: i64,
    py: i64,
    x: i64,
    y: i64,
    w: i64,
    h: i64,
    radii: RoundingRadii,
) -> bool {
    if px < x || py < y || px >= x + w || py >= y + h {
        return false;
    }
    corner_contains(px, py, (x, y), (-1, -1), radii.top_left)
        && corner_contains(px, py, (x + w, y), (1, -1), radii.top_right)
        && corner_contains(px, py, (x, y + h), (-1, 1), radii.bottom_left)
        && corner_contains(px, py, (x + w, y + h), (1, 1), radii.bottom_right)
}
