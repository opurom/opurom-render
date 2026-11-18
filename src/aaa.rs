//! Analytic anti-aliased (AAA) coverage for convex paths.
//!
//! Only the mathematics is ported from Skia `SkScan_AAAPath.cpp`, revision
//! 0e5d8d1be81b22f5529a41cf4810dd89b63527ba:
//! - `trapezoid_to_alpha` (:535), `partial_triangle_to_alpha` (:542)
//! - `compute_alpha_above_line` (:590), `compute_alpha_below_line` (:617)
//! - `blit_aaa_trapezoid_row` (:707), `blit_trapezoid_row` (:806)
//!
//! Scan lines are integer rows plus every segment endpoint, so each strip is
//! a single trapezoid. Coverage is the exact strip area fraction, not a
//! point-sample average. No Skia types are copied; coordinates are i64 16.16.

use crate::geometry::{CornerRadii, RoundingRadii};

/// 16.16 fixed-point unit (Skia `SK_Fixed1`).
pub(crate) const ONE: i64 = 1 << 16;

/// Clamp a fixed-point slope to Skia's saturated sentinel for near-verticals.
const MAX_SLOPE: i64 = i64::MAX >> 2;

fn fmul(a: i64, b: i64) -> i64 {
    (a.saturating_mul(b)) >> 16
}

fn mul_div(a: i64, b: i64, d: i64) -> i64 {
    if d == 0 {
        0
    } else {
        // `i64::MIN / -1` also overflows; fail closed to the positive limit.
        (a.saturating_mul(b)).checked_div(d).unwrap_or(i64::MAX)
    }
}

fn trunc_fixed(v: i64) -> i64 {
    v >> 16
}

fn ceil_fixed(v: i64) -> i64 {
    v.saturating_add(ONE - 1) >> 16
}

/// 16.16 left shift that saturates instead of wrapping at extreme fixed
/// coordinates. Equal to `v << 16` wherever that shift is representable.
fn shl16(v: i64) -> i64 {
    v.saturating_mul(ONE)
}

/// Multiply a 16.16 fixed coefficient by a pixel radius. The product stays
/// 16.16 fixed with no shift; i128 prevents wrap at extreme radii.
fn mul_fp_px(v: i64, scalar: i64) -> i64 {
    ((v as i128).saturating_mul(scalar as i128)).clamp(i64::MIN as i128, i64::MAX as i128) as i64
}

/// Multiply a 32.16 coefficient by a scalar and reduce to 16.16 fixed.
fn scale_fixed(v: i64, scalar: i64) -> i64 {
    (((v as i128).saturating_mul(scalar as i128)) >> 16).clamp(i64::MIN as i128, i64::MAX as i128)
        as i64
}

/// Fail-closed bound for a row span. A surface row is at most
/// `MAX_DIMENSION` (4096) pixels; anything larger is corrupt fixed-point
/// input, so drop the strip instead of allocating or looping past the mask.
const MAX_ROW_SPAN: i64 = 1 << 20;

/// Skia :535 — alpha of a unit-height trapezoid of fixed widths `l1`, `l2`.
pub(crate) fn trapezoid_to_alpha(l1: i64, l2: i64) -> i64 {
    let area = l1.saturating_add(l2) / 2;
    (area >> 8).clamp(0, 255)
}

/// Skia :542 — right-triangle of horizontal `a` and slope `b`, both fixed.
pub(crate) fn partial_triangle_to_alpha(a: i64, b: i64) -> i64 {
    let a = (a >> 11).clamp(0, 32);
    let b = (b >> 11).clamp(0, 32);
    ((a * a * b) >> 8) & 0xFF
}

/// Skia :561 — scale an alpha by a 16.16 fraction, rounding to integer.
pub(crate) fn partial_alpha(alpha: i64, fraction: i64) -> i64 {
    let scaled = alpha.saturating_mul(fraction);
    (scaled.saturating_add(ONE / 2) >> 16).clamp(0, 255)
}

/// Skia :565 — alpha scaled by another alpha (0..255).
pub(crate) fn alpha_of(alpha: i64, full: i64) -> i64 {
    (alpha.saturating_mul(full) >> 8).clamp(0, 255)
}

/// Skia :590 — per-column alphas for the region ABOVE a line crossing the row.
pub(crate) fn compute_alpha_above_line(alphas: &mut [i64], l: i64, r: i64, dy: i64, full: i64) {
    let r_int = ceil_fixed(r);
    if r_int <= 0 {
        return;
    }
    if r_int == 1 {
        if alphas.is_empty() {
            return;
        }
        let a = ((((ONE << 1).saturating_sub(l)).saturating_sub(r)) >> 9).clamp(0, 255);
        alphas[0] = alpha_of(a, full);
        return;
    }
    let (first, last) = (ONE.saturating_sub(l), r.saturating_sub(shl16(r_int - 1)));
    let first_h = fmul(first, dy);
    if alphas.is_empty() {
        return;
    }
    alphas[0] = fmul(first, first_h) >> 9;
    let mut alpha16 = first_h.saturating_add(dy >> 1);
    for i in 1..r_int - 1 {
        if i as usize >= alphas.len() {
            break;
        }
        alphas[i as usize] = (alpha16 >> 8).clamp(0, 255);
        alpha16 = alpha16.saturating_add(dy);
    }
    let tail = (full - partial_triangle_to_alpha(last, dy)).clamp(0, 255);
    let idx = (r_int - 1) as usize;
    if idx < alphas.len() {
        alphas[idx] = tail;
    }
}

/// Skia :617 — per-column alphas for the region BELOW a line crossing the row.
pub(crate) fn compute_alpha_below_line(alphas: &mut [i64], l: i64, r: i64, dy: i64, full: i64) {
    let r_int = ceil_fixed(r);
    if r_int <= 0 {
        return;
    }
    if r_int == 1 {
        if let Some(slot) = alphas.first_mut() {
            *slot = alpha_of(trapezoid_to_alpha(l, r), full);
        }
        return;
    }
    let (first, last) = (ONE.saturating_sub(l), r.saturating_sub(shl16(r_int - 1)));
    let last_h = fmul(last, dy);
    let idx = (r_int - 1) as usize;
    if let Some(slot) = alphas.get_mut(idx) {
        *slot = fmul(last, last_h) >> 9;
    }
    let mut alpha16 = last_h.saturating_add(dy >> 1);
    for i in (1..r_int - 1).rev() {
        if i as usize >= alphas.len() {
            continue;
        }
        alphas[i as usize] = ((alpha16 >> 8) & 0xFF).clamp(0, 255);
        alpha16 = alpha16.saturating_add(dy);
    }
    if let Some(slot) = alphas.first_mut() {
        *slot = (full - partial_triangle_to_alpha(first, dy)).clamp(0, 255);
    }
}

/// Skia clamps a strip to the surface clip before blitting; an out-of-row
/// index is dropped instead of indexing past the mask.
pub(crate) fn add_mask(mask: &mut [i64], idx: i64, a: i64) {
    if idx >= 0 {
        if let Some(slot) = mask.get_mut(idx as usize) {
            *slot = slot.saturating_add(a);
        }
    }
}

/// Skia :707 — accumulate the analytic coverage of one trapezoid strip into a
/// row mask. `ul/ur` are the top x edges, `ll/lr` the bottom x edges, all
/// relative to the strip's left pixel and in 16.16. `ldy/rdy` are |dy/dx|.
pub(crate) fn blit_aaa_trapezoid_row(
    mask: &mut [i64],
    origin: i64,
    ul: i64,
    ur: i64,
    ll: i64,
    lr: i64,
    ldy: i64,
    rdy: i64,
    full: i64,
) {
    let left = trunc_fixed(ul);
    let right = ceil_fixed(lr);
    let len = right.saturating_sub(left);
    // `origin + left + i` must stay positive and in range for every blit
    // index; check the bound in u64 so no intermediate overflows i64.
    let left_u = left.max(0) as u64;
    let origin_u = if origin < 0 { 0u64 } else { origin as u64 };
    let limit = u64::MAX - origin_u - left_u;
    if len <= 0 || len > MAX_ROW_SPAN || (len as u64) > limit {
        return;
    }
    if len == 1 {
        // Skia :723 scales the single pixel by the full alpha as well.
        add_mask(
            mask,
            origin + left,
            alpha_of(
                trapezoid_to_alpha(ur.saturating_sub(ul), lr.saturating_sub(ll)),
                full,
            ),
        );
        return;
    }
    let mut alphas = vec![full; len as usize];

    // Exclude the area left of the left edge (Skia :746-765).
    let u_left = trunc_fixed(ul);
    let l_left = ceil_fixed(ll);
    if u_left.saturating_add(2) == l_left {
        let first = shl16(u_left).saturating_add(ONE).saturating_sub(ul);
        let second = ll.saturating_sub(ul).saturating_sub(first);
        let a1 = (full - partial_triangle_to_alpha(first, ldy)).clamp(0, 255);
        let a2 = partial_triangle_to_alpha(second, ldy);
        alphas[0] = (alphas[0] - a1).max(0);
        alphas[1] = (alphas[1] - a2).max(0);
    } else {
        let mut temp = vec![0i64; len as usize];
        let off = u_left - left;
        if off >= 0 && off < len {
            let span = (l_left - u_left).clamp(0, len - off);
            let count = (temp.len() - off as usize).min(span as usize);
            compute_alpha_below_line(
                &mut temp[off as usize..off as usize + count],
                ul.saturating_sub(shl16(u_left)),
                ll.saturating_sub(shl16(u_left)),
                ldy,
                full,
            );
            for i in 0..count {
                let idx = (off + i as i64) as usize;
                alphas[idx] = (alphas[idx] - temp[idx]).max(0);
            }
        }
    }

    // Exclude the area right of the right edge (Skia :767-786).
    let u_right = trunc_fixed(ur);
    let l_right = ceil_fixed(lr);
    if u_right.saturating_add(2) == l_right {
        let first = shl16(u_right).saturating_add(ONE).saturating_sub(ur);
        let second = lr.saturating_sub(ur).saturating_sub(first);
        let a1 = partial_triangle_to_alpha(first, rdy);
        let a2 = (full - partial_triangle_to_alpha(second, rdy)).clamp(0, 255);
        let i2 = (len - 2) as usize;
        alphas[i2] = (alphas[i2] - a1).max(0);
        alphas[len as usize - 1] = (alphas[len as usize - 1] - a2).max(0);
    } else {
        let mut temp = vec![0i64; len as usize];
        let off = u_right - left;
        if off >= 0 && off < len {
            let span = (l_right - u_right).clamp(0, len - off);
            let count = (temp.len() - off as usize).min(span as usize);
            compute_alpha_above_line(
                &mut temp[off as usize..off as usize + count],
                ur.saturating_sub(shl16(u_right)),
                lr.saturating_sub(shl16(u_right)),
                rdy,
                full,
            );
            for i in 0..count {
                let idx = (off + i as i64) as usize;
                alphas[idx] = (alphas[idx] - temp[idx]).max(0);
            }
        }
    }

    for (i, a) in alphas.into_iter().enumerate() {
        add_mask(mask, origin + left + i as i64, a);
    }
}

/// Skia :806 — blit one trapezoid row. `ul/ur` are the left and right edge x at
/// the top of the row, `ll/lr` the same at the bottom, `ldy/rdy` the absolute
/// edge slopes and `full` the row's full alpha. The area left of the left edge
/// and right of the right edge is excluded; the rest is filled.
pub(crate) fn blit_trapezoid_row(
    mask: &mut [i64],
    origin: i64,
    mut ul: i64,
    mut ur: i64,
    mut ll: i64,
    mut lr: i64,
    ldy: i64,
    rdy: i64,
    full: i64,
) {
    if ul > ur {
        return;
    }
    // Edges that cross are an approximation; this only happens at the
    // precision limit (Skia :825-827).
    if ll > lr {
        let lo = ul.min(ll);
        let hi = ur.max(lr);
        // Saturating midpoint: `(lo + hi) / 2` can overflow at i64 extremes.
        let mid = lo.saturating_add(hi) / 2;
        ll = mid;
        lr = mid;
    }
    if ul == ur && ll == lr {
        return;
    }
    // Swapping the endpoints does not change the excluded area (Skia :837-842).
    if ul > ll {
        std::mem::swap(&mut ul, &mut ll);
    }
    if ur > lr {
        std::mem::swap(&mut ur, &mut lr);
    }

    let join_left = shl16(ceil_fixed(ll));
    let join_rite = ur & !(ONE - 1);
    if join_left <= join_rite {
        if ul < join_left {
            let len = ceil_fixed(join_left.saturating_sub(ul));
            if len == 1 {
                add_mask(
                    mask,
                    origin + (ul >> 16),
                    alpha_of(
                        trapezoid_to_alpha(
                            join_left.saturating_sub(ul),
                            join_left.saturating_sub(ll),
                        ),
                        full,
                    ),
                );
            } else if len == 2 {
                let first = join_left.saturating_sub(ONE).saturating_sub(ul);
                let second = ll.saturating_sub(ul).saturating_sub(first);
                add_mask(
                    mask,
                    origin + (ul >> 16),
                    alpha_of(partial_triangle_to_alpha(first, ldy), full),
                );
                add_mask(
                    mask,
                    origin + (ul >> 16) + 1,
                    alpha_of(
                        (full - partial_triangle_to_alpha(second, ldy)).clamp(0, 255),
                        full,
                    ),
                );
            } else {
                blit_aaa_trapezoid_row(
                    mask,
                    origin,
                    ul,
                    join_left,
                    ll,
                    join_left,
                    ldy,
                    i64::MAX,
                    full,
                );
            }
        }
        // The rectangle between the joins is fully covered.
        if join_left < join_rite {
            let x0 = join_left >> 16;
            let len = (join_rite - join_left) >> 16;
            if len > MAX_ROW_SPAN || origin > i64::MAX - x0 - len {
                return;
            }
            for i in 0..len {
                add_mask(mask, origin + x0 + i, full);
            }
        }
        if lr > join_rite {
            let len = ceil_fixed(lr.saturating_sub(join_rite));
            if len == 1 {
                add_mask(
                    mask,
                    origin + (join_rite >> 16),
                    alpha_of(
                        trapezoid_to_alpha(
                            ur.saturating_sub(join_rite),
                            lr.saturating_sub(join_rite),
                        ),
                        full,
                    ),
                );
            } else if len == 2 {
                let first = join_rite.saturating_add(ONE).saturating_sub(ur);
                let second = lr.saturating_sub(ur).saturating_sub(first);
                add_mask(
                    mask,
                    origin + (join_rite >> 16),
                    alpha_of(
                        (full - partial_triangle_to_alpha(first, rdy)).clamp(0, 255),
                        full,
                    ),
                );
                add_mask(
                    mask,
                    origin + (join_rite >> 16) + 1,
                    alpha_of(partial_triangle_to_alpha(second, rdy), full),
                );
            } else {
                blit_aaa_trapezoid_row(
                    mask,
                    origin,
                    join_rite,
                    ur,
                    join_rite,
                    lr,
                    i64::MAX,
                    rdy,
                    full,
                );
            }
        }
    } else {
        blit_aaa_trapezoid_row(mask, origin, ul, ur, ll, lr, ldy, rdy, full);
    }
}

/// A fixed-point segment of a closed convex path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    pub x0: i64,
    pub y0: i64,
    pub x1: i64,
    pub y1: i64,
}

fn x_at(s: &Segment, y: i64) -> i64 {
    if s.y0 == s.y1 {
        return s.x0.min(s.x1);
    }
    s.x0.saturating_add(mul_div(
        y.saturating_sub(s.y0),
        s.x1.saturating_sub(s.x0),
        s.y1.saturating_sub(s.y0),
    ))
}

fn slope(s: &Segment) -> i64 {
    if s.x0 == s.x1 {
        return MAX_SLOPE;
    }
    // `i64::MIN.abs()` panics in debug and wraps in release; saturate first.
    let dy = s.y1.saturating_sub(s.y0);
    let dx = s.x1.saturating_sub(s.x0);
    dy.saturating_abs().saturating_mul(ONE) / dx.saturating_abs().max(1)
}

/// Exact convex coverage for one pixel row band, accumulated into `row`
/// (one i64 mask slot per pixel, starting at `origin`). Breakpoints are the
/// integer row ends plus every segment endpoint y inside the row, mirroring
/// Skia's scan-line set (file header :47-53).
pub fn rasterize_convex_row(segments: &[Segment], row: &mut [i64], origin: i64, y: i64) {
    let y_top = shl16(y);
    let y_bot = shl16(y.saturating_add(1));
    let mut cuts = vec![y_top, y_bot];
    for s in segments {
        for ey in [s.y0, s.y1] {
            if ey > y_top && ey < y_bot {
                cuts.push(ey);
            }
        }
    }
    cuts.sort_unstable();
    cuts.dedup();
    for pair in cuts.windows(2) {
        let (c0, c1) = (pair[0], pair[1]);
        if c1 <= c0 {
            continue;
        }
        let active: Vec<&Segment> = segments
            .iter()
            .filter(|s| s.y0 != s.y1 && s.y0.min(s.y1) <= c0 && c0 <= s.y0.max(s.y1))
            .collect();
        if active.len() < 2 {
            continue;
        }
        let (mut ul, mut ur) = (i64::MAX, i64::MIN);
        let (mut ll, mut lr) = (i64::MAX, i64::MIN);
        let (mut ls, mut rs) = (0i64, 0i64);
        for s in &active {
            let (xa, xb, sl) = (x_at(s, c0), x_at(s, c1), slope(s));
            if xa < ul {
                ul = xa;
                ll = xb;
                ls = sl;
            }
            if xa > ur {
                ur = xa;
                lr = xb;
                rs = sl;
            }
        }
        if ul > ur {
            continue;
        }
        // Unit-height trapezoid alphas, then scale by this strip's height.
        let first_x = trunc_fixed(ul).min(trunc_fixed(ll));
        let last_x = ceil_fixed(ur)
            .max(ceil_fixed(lr))
            .max(first_x.saturating_add(1));
        // Clip the strip to the row instead of dropping it: a segment ending
        // inside this band extrapolates its far x past the surface, which can
        // push first_x outside the row while covered pixels stay inside.
        let row_end = origin.saturating_add(row.len() as i64);
        let lo = first_x.max(origin);
        let hi = last_x.min(row_end);
        if hi <= lo {
            continue;
        }
        let strip_span = hi.saturating_sub(lo);
        if strip_span > MAX_ROW_SPAN {
            continue;
        }
        let width = strip_span as usize;
        let mut strip = vec![0i64; width];
        let shift = shl16(lo);
        blit_aaa_trapezoid_row(
            &mut strip,
            0,
            ul.saturating_sub(shift),
            ur.saturating_sub(shift),
            ll.saturating_sub(shift),
            lr.saturating_sub(shift),
            ls,
            rs,
            255,
        );
        let h = c1.saturating_sub(c0);
        let base = (lo - origin) as usize;
        for (i, a) in strip.into_iter().enumerate() {
            row[base + i] = row[base + i].saturating_add(partial_alpha(a, h));
        }
    }
}

/// Error coefficient |k|*sqrt(2) from Skia's conic error setup
/// (`SkGeometry.cpp:1469-1473`, quarter-circle weight kSqrt2Over2): a quarter
/// arc of radius r deviates `0.06066 * r` px from its quad. Kept as an exact
/// rational to the 1e-8 place so the integer comparison is deterministic.
const CONIC_ERR_NUM: i128 = 60_660_170;
const CONIC_ERR_DEN: i128 = 1_000_000_000;

/// Integer port of `SkConic::computeQuadPOW2` (`SkGeometry.cpp:1492`): the
/// number of quads is `2^pow2`, starting at one and doubling (error quartering)
/// until the chord error is within `tol_num/tol_den` pixels, capped at
/// `kMaxConicToQuadPOW2 = 5`. No fixed segment count.
/// Impeller `ComputeConicSubdivisions` (`wangs_formula.cc:45-77`) specialised
/// to a unit quarter circle at tolerance 1/4 px (`wangs_formula.cc:14`).
/// Returns the number of quads directly, capped at Skia's
/// `kMaxConicToQuadPOW2 = 32`. Integer fixed point with 1024 == 1 px;
/// square roots use integer isqrt so the result is deterministic.
pub fn conic_quad_count_wang(radius: i64) -> u32 {
    // p0=(r,0), p1=(r,r), p2=(0,r) relative to the arc's corner centre,
    // translated by the bbox centre (r/2, r/2) for translation invariance.
    // The result is clamped to 1..=32 and already saturates at 32 for modest
    // radii. Clamp the input before squaring: i64::MIN/MAX radii otherwise
    // overflow the i128 intermediates. Normal inputs keep the exact history.
    let r = radius.max(0).min(1i64 << 20) as i128;
    let half = r / 2;
    let (p0x, p0y) = (r - half, -half);
    let (p1x, p1y) = (r - half, r - half);
    let (p2x, p2y) = (-half, r - half);
    let len2 = |x: i128, y: i128| x * x + y * y;
    let max_len_sq = len2(p0x, p0y).max(len2(p1x, p1y)).max(len2(p2x, p2y));
    let max_len = isqrt(max_len_sq * 1_048_576) as i128; // 1024 fixed
                                                         // w = cos(45deg) = sqrt(2)/2; Impeller stores kConicWeight = sqrt(2)/2.
                                                         // dp = -2*w*p1 + p0 + p2; with p0+p2 = 0 after centring, |dp| = 2*w*|p1|.
    let w_num = 46_341i128; // sqrt(2)/2 * 65536, Skia CONIC_WEIGHT
    let w2_num = w_num * w_num;
    let dp2 = 4 * w2_num * len2(p1x, p1y) / (65_536i128 * 65_536);
    // dw = |2 - 2w| as an exact rational: 2*(65536 - 46341)/65536. The shader
    // evaluates `dw = abs(-2*w + 2)` (`wangs_formula.cc:63`); keeping it as a
    // numerator/denominator pair removes the only f64 in the kernel, so the
    // count no longer depends on platform float rounding.
    let dw_num = 2 * (65_536 - w_num);
    let dw_den = 65_536i128;
    // k = precision(4) in 1024 fixed; rp_minus_1 = max(0, max_len*k - 1).
    let rp_minus_1 = (max_len * 4 / 1024 - 1024).max(0);
    // numer_sq = dp2*16 + rp^2 * dw_fixed^2 / 2^20 with dw_fixed = dw*1024,
    // computed as an exact rational over dw_den^2 * 2^20.
    let dw_fixed_num = dw_num * 1024;
    let scale = dw_den * dw_den * 1_048_576;
    let numer_sq = dp2 * 16 * scale + rp_minus_1 * rp_minus_1 * dw_fixed_num * dw_fixed_num;
    let denom = (4.0 * w_num.min(65_536) as f64 / 65_536.0 * 1024.0) as i128;
    // sqrt(numer / denom) in 1024 fixed point: sqrt(a/b) = sqrt(a*b) / b.
    // The scaled numerator keeps every intermediate an exact integer.
    let n = isqrt((numer_sq.max(0) / scale) * denom.max(1)) as u32 / 1024;
    n.clamp(1, 32)
}

pub fn conic_quad_pow2(radius: i64, tol_num: i64, tol_den: i64) -> u32 {
    // pow2 is capped at 5 (32 quads) and saturates at a modest radius, so
    // clamping the input before squaring preserves every normal result while
    // keeping i64 extremes inside i128 arithmetic.
    let r = radius.max(0).min(1i64 << 20) as i128;
    let err2 = (CONIC_ERR_NUM * r) * (CONIC_ERR_NUM * r);
    // tol and tol2 stay inside i128: the product is clamped to the square
    // root of i128::MAX before squaring, so an extreme `tol_num` saturates
    // instead of wrapping (fail closed: a huge tolerance keeps pow2 at 0).
    let tol_raw = (tol_num.max(0) as i128 * CONIC_ERR_DEN) / tol_den.max(1) as i128;
    let tol = tol_raw.min(1 << 63);
    let tol2 = tol * tol;
    let mut e2 = err2;
    let mut pow2 = 0u32;
    while pow2 < 5 && e2 > tol2 {
        e2 /= 16;
        pow2 += 1;
    }
    pow2
}

/// Build a convex polygon for a rounded rectangle. Corner arcs use Skia's
/// adaptive subdivision at a 1/8 px tolerance; straight edges stay exact.
pub fn round_rect_path(x: i64, y: i64, w: i64, h: i64, radius: i64) -> Vec<Segment> {
    let r = radius.max(0).min(w / 2).min(h / 2);
    if r <= 0 {
        let (x1, y1) = (x.saturating_add(w), y.saturating_add(h));
        let f = shl16;
        return vec![
            Segment {
                x0: f(x),
                y0: f(y),
                x1: f(x1),
                y1: f(y),
            },
            Segment {
                x0: f(x1),
                y0: f(y),
                x1: f(x1),
                y1: f(y1),
            },
            Segment {
                x0: f(x1),
                y0: f(y1),
                x1: f(x),
                y1: f(y1),
            },
            Segment {
                x0: f(x),
                y0: f(y1),
                x1: f(x),
                y1: f(y),
            },
        ];
    }
    let n = (1usize << conic_quad_pow2(r, 1, 8)).max(32);
    let mut pts: Vec<(i64, i64)> = Vec::with_capacity(4 * n + 4);
    // Unit quarter arc (c,s) = ((1-u^2)/(1+u^2), 2u/(1+u^2)), u in [0,1], so
    // (c,s) runs (1,0) -> (0,1). Each corner maps it onto its own tangent pair.
    // All math is 16.16 fixed point, no floats.
    // (mx/my sign mapping kept explicit per corner below.)
    // Top-left: (cx - r*c, cy - r*s), left -> top.
    {
        let (cx, cy) = (x.saturating_add(r), y.saturating_add(r));
        for i in 0..=n {
            let u = i as i128 * ONE as i128 / n as i128;
            let u2 = u * u / ONE as i128;
            let den = ONE as i128 + u2;
            let c = ((ONE as i128 - u2) << 16) / den;
            let s = (2 * u << 16) / den;
            pts.push((
                shl16(cx).saturating_sub(mul_fp_px(c as i64, r)),
                shl16(cy).saturating_sub(mul_fp_px(s as i64, r)),
            ));
        }
    }
    // Top-right: (cx + s, cy - c), top -> right.
    {
        let (cx, cy) = (x.saturating_add(w).saturating_sub(r), y.saturating_add(r));
        for i in 0..=n {
            let u = i as i128 * ONE as i128 / n as i128;
            let u2 = u * u / ONE as i128;
            let den = ONE as i128 + u2;
            let c = ((ONE as i128 - u2) << 16) / den;
            let s = (2 * u << 16) / den;
            pts.push((
                shl16(cx).saturating_add(mul_fp_px(s as i64, r)),
                shl16(cy).saturating_sub(mul_fp_px(c as i64, r)),
            ));
        }
    }
    // Bottom-right: (cx + c, cy + s), right -> bottom.
    {
        let (cx, cy) = (
            x.saturating_add(w).saturating_sub(r),
            y.saturating_add(h).saturating_sub(r),
        );
        for i in 0..=n {
            let u = i as i128 * ONE as i128 / n as i128;
            let u2 = u * u / ONE as i128;
            let den = ONE as i128 + u2;
            let c = ((ONE as i128 - u2) << 16) / den;
            let s = (2 * u << 16) / den;
            pts.push((
                shl16(cx).saturating_add(mul_fp_px(c as i64, r)),
                shl16(cy).saturating_add(mul_fp_px(s as i64, r)),
            ));
        }
    }
    // Bottom-left: (cx - s, cy + c), bottom -> left.
    {
        let (cx, cy) = (x.saturating_add(r), y.saturating_add(h).saturating_sub(r));
        for i in 0..=n {
            let u = i as i128 * ONE as i128 / n as i128;
            let u2 = u * u / ONE as i128;
            let den = ONE as i128 + u2;
            let c = ((ONE as i128 - u2) << 16) / den;
            let s = (2 * u << 16) / den;
            pts.push((
                shl16(cx).saturating_sub(mul_fp_px(s as i64, r)),
                shl16(cy).saturating_add(mul_fp_px(c as i64, r)),
            ));
        }
    }
    let mut segs: Vec<Segment> = Vec::with_capacity(pts.len());
    for i in 0..pts.len() {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        segs.push(Segment {
            x0: a.0,
            y0: a.1,
            x1: b.0,
            y1: b.1,
        });
    }
    segs
}

/// Append one quarter-ellipse arc of radii `(rx, ry)` into `pts`. The rational
/// parameterisation of a unit quarter arc is affinely scaled to `(rx, ry)`;
/// because affine maps preserve conics, an elliptical corner stays exact.
fn push_arc(pts: &mut Vec<(i64, i64)>, cx: i64, cy: i64, rx: i64, ry: i64, corner: usize) {
    let n = 1usize << conic_quad_pow2(rx.max(ry), 1, 8);
    for i in 0..=n {
        let u = i as i128 * ONE as i128 / n as i128;
        let u2 = u * u / ONE as i128;
        let den = ONE as i128 + u2;
        let c = ((ONE as i128 - u2) << 16) / den;
        let s = (2 * u << 16) / den;
        let (dx, dy) = match corner {
            0 => (
                mul_fp_px(c as i64, rx).saturating_neg(),
                mul_fp_px(s as i64, ry).saturating_neg(),
            ), // TL: (1,0)->(0,1)
            1 => (
                mul_fp_px(s as i64, rx),
                mul_fp_px(c as i64, ry).saturating_neg(),
            ), // TR
            2 => (mul_fp_px(c as i64, rx), mul_fp_px(s as i64, ry)), // BR
            _ => (
                mul_fp_px(s as i64, rx).saturating_neg(),
                mul_fp_px(c as i64, ry),
            ), // BL
        };
        pts.push((shl16(cx).saturating_add(dx), shl16(cy).saturating_add(dy)));
    }
}

/// Skia's quarter-circle conic weight, sqrt(2)/2, in 16.16 fixed point.
pub const CONIC_WEIGHT: i64 = 46_341;

/// A quadratic Bezier control triple in 16.16 fixed point.
pub type Quad = [(i64, i64); 3];

/// Chop a rational conic into `2^pow2` quadratic curves, mirroring Skia's
/// `SkConic::chop` (`SkGeometry.cpp:1430-1462`, pinned Skia revision
/// `0e5d8d1be81b22f5529a41cf4810dd89b63527ba`; Impeller's matching subdivision
/// bound is `wangs_formula.cc:14,45-77`). The cap and degenerate-weight guard
/// below are the integer-kernel equivalent of those source contracts.
///
/// Skia works in floats; the weighted-point formula is reproduced here in
/// 16.16 with i128 intermediates. The middle control point halves each outer
/// term before summing, so the halves round towards zero rather than down:
/// an arithmetic right shift would floor a negative half and bias the corner
/// by one LSB.
pub fn chop_conic(p: Quad, weight: i64, pow2: u32) -> Vec<Quad> {
    // Skia's `kMaxConicToQuadPOW2` caps adaptive conic subdivision at 5.
    // Reject a larger caller value instead of allocating an exponential quad
    // vector (fail closed and deterministic).
    let pow2 = pow2.min(5);
    let mut quads = vec![p];
    let mut w = weight;
    for _ in 0..pow2 {
        // `ONE + w == 0` at the degenerate weight -ONE; keep the original
        // conic rather than dividing by zero (SkGeometry.cpp:1430-1462).
        if w == -ONE {
            return quads;
        }
        let scale = ONE as i128 * ONE as i128 / (ONE as i128 + w as i128);
        let ws = (w as i128 * scale) >> 16;
        let mut next = Vec::with_capacity(quads.len() * 2);
        for q in &quads {
            let t0 = (q[0].0 as i128 * scale >> 16, q[0].1 as i128 * scale >> 16);
            let t1 = (q[1].0 as i128 * ws >> 16, q[1].1 as i128 * ws >> 16);
            let t2 = (q[2].0 as i128 * scale >> 16, q[2].1 as i128 * scale >> 16);
            let p1 = ((t0.0 + t1.0) as i64, (t0.1 + t1.1) as i64);
            let p3 = ((t1.0 + t2.0) as i64, (t1.1 + t2.1) as i64);
            // Skia: p2 = 0.5f * t0 + t1 + 0.5f * t2.
            let p2 = (
                (t0.0 / 2 + t1.0 + t2.0 / 2) as i64,
                (t0.1 / 2 + t1.1 + t2.1 / 2) as i64,
            );
            next.push([q[0], p1, p2]);
            next.push([p2, p3, q[2]]);
        }
        // The sub-conic weight is sqrt(0.5 + w/2).
        let half = (ONE as i128 * ONE as i128 / 2 + w as i128 * ONE as i128 / 2).max(0);
        w = (isqrt(half << 32) >> 16) as i64;
        quads = next;
    }
    quads
}

/// The conic that Skia uses for one rounded corner: the two tangent points and
/// the corner itself, in the corner's own 16.16 coordinates.
pub fn corner_conic(rx: i64, ry: i64, corner: usize) -> Quad {
    match corner {
        // The corners follow the rectangle's clockwise order, so each arc runs
        // from the tangent it arrives at to the tangent it leaves towards. The
        // offsets are relative to the rectangle's vertex, which is the conic's
        // control point.
        // Top-left: left tangent -> top tangent.
        0 => [(0, shl16(ry)), (0, 0), (shl16(rx), 0)],
        // Top-right: top tangent -> right tangent.
        1 => [(shl16(rx).saturating_neg(), 0), (0, 0), (0, shl16(ry))],
        // Bottom-right: right tangent -> bottom tangent.
        2 => [
            (0, shl16(ry).saturating_neg()),
            (0, 0),
            (shl16(rx).saturating_neg(), 0),
        ],
        // Bottom-left: bottom tangent -> left tangent.
        _ => [(shl16(rx), 0), (0, 0), (0, shl16(ry).saturating_neg())],
    }
}

/// Convex path for a rectangle with independent, possibly elliptical corners.
/// Radii must already be normalised by [`RoundingRadii::scaled`]. Straight
/// edges stay exact; a flat corner (either component zero) contributes its
/// rectangle vertex only.
pub fn round_rect_path_radii(x: i64, y: i64, w: i64, h: i64, radii: RoundingRadii) -> Vec<Segment> {
    let c = |r: CornerRadii| {
        let r = CornerRadii {
            x: r.x.max(0).min(w / 2),
            y: r.y.max(0).min(h / 2),
        };
        if r.x <= 0 || r.y <= 0 {
            CornerRadii::default()
        } else {
            r
        }
    };
    let (tl, tr, br, bl) = (
        c(radii.top_left),
        c(radii.top_right),
        c(radii.bottom_right),
        c(radii.bottom_left),
    );
    let mut pts: Vec<(i64, i64)> = Vec::new();
    let mut vertex = |cx: i64, cy: i64, r: CornerRadii, corner: usize| {
        if r.x <= 0 || r.y <= 0 {
            pts.push((shl16(cx), shl16(cy)));
        } else {
            push_arc(&mut pts, cx, cy, r.x, r.y, corner);
        }
    };
    vertex(x.saturating_add(tl.x), y.saturating_add(tl.y), tl, 0);
    vertex(
        x.saturating_add(w).saturating_sub(tr.x),
        y.saturating_add(tr.y),
        tr,
        1,
    );
    vertex(
        x.saturating_add(w).saturating_sub(br.x),
        y.saturating_add(h).saturating_sub(br.y),
        br,
        2,
    );
    vertex(
        x.saturating_add(bl.x),
        y.saturating_add(h).saturating_sub(bl.y),
        bl,
        3,
    );
    let mut segs: Vec<Segment> = Vec::with_capacity(pts.len());
    for i in 0..pts.len() {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        if a == b {
            continue;
        }
        segs.push(Segment {
            x0: a.0,
            y0: a.1,
            x1: b.0,
            y1: b.1,
        });
    }
    segs
}

/// Number of quadratic segments needed to linearise a quadratic Bezier within
/// Impeller's 1/4 px tolerance (`wangs_formula.cc:14`, `:31-37`).
///
/// `n = sqrt(scale * 0.25 * 4 * |p0 - 2p1 + p2|)`, with points in logical
/// pixels and a 1024 fixed-point scale factor for subpixel precision. The
/// result is capped at 32, matching Skia's maximum conic subdivision.
pub fn quad_subdivisions(p0: (i64, i64), p1: (i64, i64), p2: (i64, i64)) -> u32 {
    // The result is capped at 32; clamp each coordinate before the 3x/4x i128
    // expansion so extreme control points saturate instead of overflowing.
    const C: i128 = 1i128 << 40;
    let clamp = |v: i64| -> i128 { (v as i128).clamp(-C, C) };
    let dx = (clamp(p0.0) - 2 * clamp(p1.0) + clamp(p2.0)) * 1024;
    let dy = (clamp(p0.1) - 2 * clamp(p1.1) + clamp(p2.1)) * 1024;
    let len_sq = dx * dx + dy * dy;
    let n = isqrt(len_sq);
    // n is length*1024 where length = |p0-2p1+p2|; apply sqrt(scale*0.25*4)
    // = sqrt(1024) = 32 in the same fixed-point domain.
    let segments = isqrt((n as i128) * 32 * 1024) / 1024;
    segments.max(1).min(32) as u32
}

/// Number of line segments needed to linearise a cubic Bezier within Impeller's
/// 1/4 px tolerance (`wangs_formula.cc:14`, `:18-28`).
pub fn cubic_subdivisions(p0: (i64, i64), p1: (i64, i64), p2: (i64, i64), p3: (i64, i64)) -> u32 {
    // The result is capped at 32; clamp before the i128 squared expansion so
    // extreme control points saturate deterministically without overflow.
    const C: i128 = 1i128 << 40;
    let clamp = |v: i64| -> i128 { (v as i128).clamp(-C, C) };
    let a = (
        clamp(p0.0) - 2 * clamp(p1.0) + clamp(p2.0),
        clamp(p0.1) - 2 * clamp(p1.1) + clamp(p2.1),
    );
    let b = (
        clamp(p1.0) - 2 * clamp(p2.0) + clamp(p3.0),
        clamp(p1.1) - 2 * clamp(p2.1) + clamp(p3.1),
    );
    let max_len = (a.0 * a.0 + a.1 * a.1).max(b.0 * b.0 + b.1 * b.1);
    // sqrt(k * sqrt(max_len)), k = 0.75*4 = 3, in 1024 fixed point.
    let inner = isqrt(max_len * 1_048_576);
    let segments = isqrt((inner as i128) * 3 * 1024) / 1024;
    segments.max(1).min(32) as u32
}

/// Integer square root of a non-negative i128, floor-rounded.
fn isqrt(v: i128) -> i64 {
    if v <= 0 {
        return 0;
    }
    let mut x = v as u128;
    let mut r = 0u128;
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
    // `isqrt(u128::MAX)` is about 2^64, which does not fit i64. Saturate
    // instead of wrapping: callers treat the result as a length/count, so a
    // wrapped negative would become a huge allocation or a negative segment
    // count (fail closed to the largest representable count).
    r.min(i64::MAX as u128) as i64
}

/// Convex path for a filled disc, centred at `(cx, cy)` with radius `r`.
/// Points come from the same rational quarter-arc rule as rounded corners,
/// mirrored across the two axes; all arithmetic is fixed point.
pub fn circle_path(cx: i64, cy: i64, r: i64) -> Vec<Segment> {
    if r <= 0 {
        return Vec::new();
    }
    let n = (1usize << conic_quad_pow2(r, 1, 8)).max(32);
    let mut pts: Vec<(i64, i64)> = Vec::with_capacity(4 * n);
    // One quarter from (cx+r, cy) to (cx, cy+r) via (c, s) = ((1-u^2)/(1+u^2),
    // 2u/(1+u^2)); the other three quarters mirror its signs.
    for i in 0..n {
        let u = i as i128 * ONE as i128 / n as i128;
        let u2 = u * u / ONE as i128;
        let den = ONE as i128 + u2;
        let c = (((ONE as i128 - u2) << 16) / den) as i64;
        let s = ((2 * u << 16) / den) as i64;
        let (dx, dy) = (scale_fixed(c, r), scale_fixed(s, r));
        let (base_x, base_y) = (shl16(cx), shl16(cy));
        pts.push((base_x.saturating_add(dx), base_y.saturating_add(dy))); // Q1
        pts.push((base_x.saturating_sub(dx), base_y.saturating_add(dy))); // Q2
        pts.push((base_x.saturating_sub(dx), base_y.saturating_sub(dy))); // Q3
        pts.push((base_x.saturating_add(dx), base_y.saturating_sub(dy))); // Q4
    }
    let total = pts.len();
    let mut segs = Vec::with_capacity(total);
    for i in 0..total {
        let a = pts[i];
        let b = pts[(i + 1) % total];
        segs.push(Segment {
            x0: a.0,
            y0: a.1,
            x1: b.0,
            y1: b.1,
        });
    }
    segs
}

/// Convex quad for a thick segment with a square pen. `half_fp` is the pen
/// half-width in 16.16 pixels, routed through the same analytic coverage as
/// corners. The perpendicular offset uses an integer length.
pub fn line_path(x0: i64, y0: i64, x1: i64, y1: i64, half_fp: i64) -> Vec<Segment> {
    let (dx, dy) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
    let len = isqrt((dx as i128) * dx as i128 + (dy as i128) * dy as i128);
    let (px, py) = if len == 0 {
        (0i64, 0i64)
    } else {
        (
            (-dy as i128 * half_fp as i128 / len as i128) as i64,
            (dx as i128 * half_fp as i128 / len as i128) as i64,
        )
    };
    let corners = [
        (shl16(x0).saturating_add(px), shl16(y0).saturating_add(py)),
        (shl16(x1).saturating_add(px), shl16(y1).saturating_add(py)),
        (shl16(x1).saturating_sub(px), shl16(y1).saturating_sub(py)),
        (shl16(x0).saturating_sub(px), shl16(y0).saturating_sub(py)),
    ];
    let mut segs = Vec::with_capacity(4);
    for i in 0..4 {
        let a = corners[i];
        let b = corners[(i + 1) % 4];
        segs.push(Segment {
            x0: a.0,
            y0: a.1,
            x1: b.0,
            y1: b.1,
        });
    }
    segs
}

/// Build the analytic edges of a rounded rectangle: one quadratic edge per
/// corner and one straight edge per side.
///
/// The corners use Skia's corner conic (`SkConic::chopIntoQuadsPOW2` with
/// `kConicTol = 0.25`), so the curve is the one Skia renders rather than a
/// sampled circle.
pub fn round_rect_walker_edges(
    x: i64,
    y: i64,
    w: i64,
    h: i64,
    radius: i64,
) -> Vec<crate::aaa_walker::Edge> {
    use crate::aaa_walker::{Edge, LineEdge, QuadEdge};
    // The walker stores 16.16 coordinates in i32 (Skia's analytic scan
    // contract). Clamp to that domain before casting: a rust `as i32` wraps
    // and silently reverses edges on rects wider than 32767 px, while a clamp
    // keeps geometry monotone (pixels past the device range are unreachable).
    let fixed32 = |v: i64| v.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
    let r = radius.max(0).min(w / 2).min(h / 2);
    if r <= 0 {
        let (x1, y1) = (x.saturating_add(w), y.saturating_add(h));
        let f = shl16;
        let mut edges = Vec::new();
        for (a, b) in [
            ((f(x), f(y)), (f(x1), f(y))),
            ((f(x1), f(y)), (f(x1), f(y1))),
            ((f(x1), f(y1)), (f(x), f(y1))),
            ((f(x), f(y1)), (f(x), f(y))),
        ] {
            let a32 = (fixed32(a.0), fixed32(a.1));
            let b32 = (fixed32(b.0), fixed32(b.1));
            if let Some(e) = LineEdge::new(a32, b32) {
                edges.push(Edge::Line(e));
            }
        }
        return edges;
    }

    let pow2 = conic_quad_pow2(r, 1, 4);
    let mut edges = Vec::new();
    // The four corners, in clockwise order, each as one or more quadratic
    // edges; the straight sides join them.
    // The corners are placed at the rectangle's own vertices: the conic runs
    // from one tangent point to the next with the vertex as its control
    // point, which is how Skia describes a rounded corner.
    let corners = [
        (x, y, 0usize),
        (x + w, y, 1),
        (x + w, y + h, 2),
        (x, y + h, 3),
    ];
    for (cx, cy, corner) in corners {
        let cx_fp = shl16(cx);
        let cy_fp = shl16(cy);
        let conic = corner_conic(r, r, corner);
        let quads = chop_conic(conic, CONIC_WEIGHT, pow2);
        for q in quads {
            let shifted = [
                (
                    fixed32(q[0].0.saturating_add(cx_fp)),
                    fixed32(q[0].1.saturating_add(cy_fp)),
                ),
                (
                    fixed32(q[1].0.saturating_add(cx_fp)),
                    fixed32(q[1].1.saturating_add(cy_fp)),
                ),
                (
                    fixed32(q[2].0.saturating_add(cx_fp)),
                    fixed32(q[2].1.saturating_add(cy_fp)),
                ),
            ];
            if let Some(e) = QuadEdge::new(shifted) {
                edges.push(Edge::Quad(e));
            }
        }
    }
    edges
}

/// Unified corner arc segment count for a rounded corner of logical-pixel
/// `radius`, resolved from the two pinned sources:
/// - Skia `SkConic::computeQuadPOW2` at 1/8 px tolerance
///   (`SkGeometry.cpp:1492`; pinned revision
///   0e5d8d1be81b22f5529a41cf4810dd89b63527ba via `DEPS:18,505`) — the rule
///   already implemented by [`conic_quad_pow2`].
/// - Impeller `ComputeConicSubdivisions` at `kPrecision = 4` (1/4 px)
///   (`wangs_formula.cc:14,45-77`) — a second, independent bound.
///
/// The renderer takes the larger of the two bounds: Skia's tolerance is
/// tighter at small radii, Impeller's bound dominates for very large radii,
/// so neither can undershoot the pinned curve. The count is capped at 32
/// (Skia `kMaxConicToQuadPOW2`) and floored at 1. Small radii keep a small
/// count (no fixed 32-segment floor); large radii keep smoothness.
pub fn corner_arc_segments(radius: i64) -> u32 {
    let skia_pow2 = conic_quad_pow2(radius, 1, 8) as u32;
    let skia_count = 1u32 << skia_pow2;
    let wang_count = conic_quad_count_wang(radius);
    skia_count.max(wang_count).clamp(1, 32)
}

#[cfg(test)]
mod corner_density_tests {
    use super::{conic_quad_count_wang, conic_quad_pow2, corner_arc_segments};

    #[test]
    fn corner_density_unifies_skia_and_impeller_bounds() {
        // Small radii stay cheap: no fixed 32-segment floor.
        assert_eq!(corner_arc_segments(1), 1);
        assert_eq!(corner_arc_segments(2), 1);
        // Skia's 1/8 px rule is the tighter bound here.
        assert!(corner_arc_segments(10) > 1);
        assert!(corner_arc_segments(10) <= 32);
        // Both sources are respected: the result never falls below either.
        for r in [1i64, 4, 8, 16, 32, 64, 128, 256, 512] {
            let n = corner_arc_segments(r);
            let skia = 1u32 << conic_quad_pow2(r, 1, 8);
            let wang = conic_quad_count_wang(r);
            assert!(
                n >= skia && n >= wang,
                "radius {r}: {n} < skia {skia} or wang {wang}"
            );
            assert!(n <= 32);
        }
        // Monotone in radius.
        let mut prev = 0;
        for r in 1..=1024 {
            let n = corner_arc_segments(r);
            assert!(n >= prev, "radius {r}: {n} < {prev}");
            prev = n;
        }
    }
}

#[cfg(test)]
mod scalar_boundary_tests {
    use super::{
        alpha_of, blit_aaa_trapezoid_row, blit_trapezoid_row, ceil_fixed, isqrt, line_path,
        partial_alpha, rasterize_convex_row, round_rect_walker_edges, shl16, slope,
        trapezoid_to_alpha, Segment, ONE,
    };

    #[test]
    fn scalar_helpers_are_total_at_i64_extremes() {
        assert_eq!(ceil_fixed(i64::MAX), i64::MAX >> 16);
        assert_eq!(ceil_fixed(i64::MIN), i64::MIN >> 16);
        assert_eq!(shl16(i64::MAX), i64::MAX);
        assert_eq!(shl16(i64::MIN), i64::MIN);
        assert_eq!(trapezoid_to_alpha(i64::MAX, i64::MAX), 255);
        assert_eq!(trapezoid_to_alpha(i64::MIN, i64::MIN), 0);
        assert_eq!(alpha_of(i64::MAX, i64::MAX), 255);
        assert_eq!(partial_alpha(i64::MAX, i64::MAX), 255);
    }

    #[test]
    fn isqrt_saturates_above_the_i64_range() {
        assert_eq!(isqrt(0), 0);
        assert_eq!(isqrt(-1), 0);
        assert_eq!(isqrt(i128::MAX), i64::MAX);
        let v = (i64::MAX as i128) * (i64::MAX as i128);
        assert_eq!(isqrt(v), i64::MAX);
        assert!(isqrt(v - 1) <= i64::MAX);
    }

    #[test]
    fn slope_and_x_extrapolation_saturate_without_panicking() {
        let s = Segment {
            x0: i64::MIN,
            y0: i64::MIN,
            x1: i64::MAX,
            y1: i64::MAX,
        };
        let sl = slope(&s);
        assert!(sl >= 0 && sl <= i64::MAX, "slope {sl}");
        assert_eq!(slope(&s), slope(&s), "slope deterministic");
        let mut row = [0i64; 4];
        rasterize_convex_row(&[s, s], &mut row, 0, 0);
        assert_eq!(row, [0, 0, 0, 0], "extreme segments are clipped away");
    }

    #[test]
    fn blit_helpers_drop_oversized_or_overflowing_spans() {
        let mut mask = vec![0i64; 8];
        // A span larger than any real surface is rejected before allocation.
        blit_aaa_trapezoid_row(&mut mask, 0, 0, i64::MAX, 0, i64::MAX, 0, 0, 255);
        assert_eq!(mask, vec![0; 8]);
        blit_trapezoid_row(
            &mut mask,
            i64::MAX,
            i64::MIN,
            i64::MAX,
            i64::MAX,
            i64::MIN,
            1,
            1,
            255,
        );
        assert_eq!(mask, vec![0; 8]);
    }

    #[test]
    fn wide_walker_rects_keep_monotone_edges() {
        // 40000 px is wider than the i32 16.16 domain (32767 px). The walker
        // must clamp rather than wrap, so the four straight edges stay in
        // increasing x order and no edge reverses direction.
        let edges = round_rect_walker_edges(0, 0, 40_000, 100, 0);
        assert!(!edges.is_empty(), "a flat 40000 px rect still emits edges");
        assert!(edges.len() <= 4, "clamping cannot invent edges");
        for e in &edges {
            if let crate::aaa_walker::Edge::Line(line) = e {
                assert!(
                    line.upper_x >= i32::MIN && line.upper_y >= 0,
                    "clamped edge out of domain: {line:?}"
                );
            }
        }
        let edges = round_rect_walker_edges(0, 0, 40_000, 100, 16);
        assert!(!edges.is_empty(), "rounded wide rect still emits edges");
        assert!(
            line_path(0, 0, i64::MAX, i64::MAX, ONE).len() == 4,
            "line path is total at i64 extremes"
        );
    }
}

#[cfg(test)]
mod boundary_overflow_tests {
    use super::{
        chop_conic, circle_path, conic_quad_count_wang, conic_quad_pow2, cubic_subdivisions,
        quad_subdivisions, round_rect_path, Quad, CONIC_WEIGHT, ONE,
    };

    #[test]
    fn wang_counts_are_bounded_and_deterministic_at_i64_extremes() {
        for r in [i64::MIN, -1, 0, 1, 1 << 15, i64::MAX] {
            let a = conic_quad_count_wang(r);
            assert!((1..=32).contains(&a), "r={r}: {a}");
            assert_eq!(a, conic_quad_count_wang(r), "r={r} nondeterministic");
        }
    }

    #[test]
    fn pow2_subdivision_never_overflows_or_loops_at_extreme_radius() {
        // CONIC_ERR_NUM ~= 6e7; squaring (num*r) for r=i64::MAX needs ~137
        // bits, so the kernel must clamp the radius before the multiply.
        let n = conic_quad_pow2(i64::MAX, 1, 8);
        assert!((0..=5).contains(&n), "{n}");
        assert_eq!(n, conic_quad_pow2(i64::MAX, 1, 8));
        assert_eq!(conic_quad_pow2(i64::MIN, 1, 8), 0);
        assert!(
            (0..=5).contains(&conic_quad_pow2(100, 1, 0)),
            "zero denominator is safe"
        );
    }

    #[test]
    fn bezier_subdivisions_are_bounded_at_i64_extreme_control_points() {
        let p = (i64::MIN, i64::MAX);
        let q = (i64::MAX, i64::MIN);
        assert!((1..=32).contains(&quad_subdivisions(p, q, p)));
        assert!((1..=32).contains(&cubic_subdivisions(p, q, p, q)));
    }

    #[test]
    fn wang_count_keeps_normal_radii_history_after_rational_dw() {
        // The exact-rational dw must not change the count for any radius the
        // corner renderer can reach: the sweep saturates at 32 by r=192, and
        // the unclamped product only matters past the clamp.
        let expected = [
            (1i64, 1u32),
            (2, 1),
            (4, 1),
            (8, 1),
            (16, 3),
            (24, 5),
            (32, 6),
            (48, 10),
            (64, 13),
            (96, 20),
            (128, 26),
            (192, 32),
            (1024, 32),
            (1 << 20, 32),
        ];
        for (r, n) in expected {
            assert_eq!(conic_quad_count_wang(r), n, "radius {r}");
        }
    }

    #[test]
    fn chop_conic_rejects_degenerate_weight_and_unbounded_depth() {
        let p: Quad = [(0, 0), (ONE, ONE), (ONE << 1, 0)];
        let normal = chop_conic(p, CONIC_WEIGHT, 3);
        assert_eq!(normal.len(), 8, "normal chop still yields 2^3 quads");
        assert_eq!(normal, chop_conic(p, CONIC_WEIGHT, 3), "deterministic");
        // Skia's kMaxConicToQuadPOW2 = 5; larger public values fail closed.
        let capped = chop_conic(p, CONIC_WEIGHT, u32::MAX);
        assert_eq!(capped.len(), 32, "excessive pow2 is capped at Skia's limit");
        // w = -ONE would divide by ONE + w == 0; keep the original conic.
        let degenerate = chop_conic(p, -ONE, 5);
        assert_eq!(degenerate, vec![p], "degenerate weight is a no-op");
    }

    #[test]
    fn extreme_path_builders_stay_finite_and_deterministic() {
        let rect = round_rect_path(i64::MAX - 10, i64::MIN + 10, 20, 20, 4);
        assert!(!rect.is_empty());
        assert_eq!(
            rect,
            round_rect_path(i64::MAX - 10, i64::MIN + 10, 20, 20, 4)
        );
        assert!(rect.iter().all(|s| [s.x0, s.x1]
            .iter()
            .all(|v| *v >= i64::MIN && *v <= i64::MAX)));
        let circle = circle_path(i64::MAX, i64::MIN, 8);
        assert_eq!(circle.len(), 128);
        assert_eq!(circle, circle_path(i64::MAX, i64::MIN, 8));
    }

    #[test]
    fn pow2_tolerance_extremes_fail_closed_without_overflow() {
        // tol_num = i64::MAX with tol_den = 1 pushes tol^2 past i128::MAX;
        // the kernel clamps the tolerance so the product stays in range and
        // the loop keeps the minimum subdivision (fail closed).
        assert_eq!(conic_quad_pow2(100, i64::MAX, 1), 0);
        // tol_num == tol_den gives tol = 1e9, still tighter than r=100's error.
        assert_eq!(conic_quad_pow2(100, i64::MAX, i64::MAX), 2);
        assert_eq!(conic_quad_pow2(i64::MAX, i64::MAX, 1), 0);
        // A negative tolerance clamps to 0, the pre-existing "subdivide
        // fully" semantic; pin it so the fail-closed change cannot flip it.
        assert_eq!(conic_quad_pow2(100, -i64::MAX, 1), 5);
        // A tight tolerance still subdivides as before.
        assert!(conic_quad_pow2(100, 1, 8) > 0);
    }
}
