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

/// 16.16 fixed-point unit (Skia `SK_Fixed1`).
const ONE: i64 = 1 << 16;

/// Clamp a fixed-point slope to Skia's saturated sentinel for near-verticals.
const MAX_SLOPE: i64 = i64::MAX >> 2;

fn fmul(a: i64, b: i64) -> i64 {
    (a.saturating_mul(b)) >> 16
}

fn mul_div(a: i64, b: i64, d: i64) -> i64 {
    if d == 0 {
        0
    } else {
        (a.saturating_mul(b)) / d
    }
}

fn trunc_fixed(v: i64) -> i64 {
    v >> 16
}

fn ceil_fixed(v: i64) -> i64 {
    (v + ONE - 1) >> 16
}

/// Skia :535 — alpha of a unit-height trapezoid of fixed widths `l1`, `l2`.
fn trapezoid_to_alpha(l1: i64, l2: i64) -> i64 {
    let area = (l1 + l2) / 2;
    (area >> 8).clamp(0, 255)
}

/// Skia :542 — right-triangle of horizontal `a` and slope `b`, both fixed.
fn partial_triangle_to_alpha(a: i64, b: i64) -> i64 {
    let a = (a >> 11).clamp(0, 32);
    let b = (b >> 11).clamp(0, 32);
    ((a * a * b) >> 8) & 0xFF
}

/// Skia :561 — scale an alpha by a 16.16 fraction, rounding to integer.
fn partial_alpha(alpha: i64, fraction: i64) -> i64 {
    (alpha.saturating_mul(fraction) + ONE / 2) >> 16
}

/// Skia :565 — alpha scaled by another alpha (0..255).
fn alpha_of(alpha: i64, full: i64) -> i64 {
    (alpha * full) >> 8
}

/// Skia :590 — per-column alphas for the region ABOVE a line crossing the row.
fn compute_alpha_above_line(alphas: &mut [i64], l: i64, r: i64, dy: i64, full: i64) {
    let r_int = ceil_fixed(r);
    if r_int <= 0 {
        return;
    }
    if r_int == 1 {
        let a = (((ONE << 1) - l - r) >> 9).clamp(0, 255);
        alphas[0] = alpha_of(a, full);
        return;
    }
    let (first, last) = (ONE - l, r - ((r_int - 1) << 16));
    let first_h = fmul(first, dy);
    alphas[0] = fmul(first, first_h) >> 9;
    let mut alpha16 = first_h.saturating_add(dy >> 1);
    for i in 1..r_int - 1 {
        alphas[i as usize] = (alpha16 >> 8).clamp(0, 255);
        alpha16 = alpha16.saturating_add(dy);
    }
    let tail = (full - partial_triangle_to_alpha(last, dy)).clamp(0, 255);
    alphas[(r_int - 1) as usize] = tail;
}

/// Skia :617 — per-column alphas for the region BELOW a line crossing the row.
fn compute_alpha_below_line(alphas: &mut [i64], l: i64, r: i64, dy: i64, full: i64) {
    let r_int = ceil_fixed(r);
    if r_int <= 0 {
        return;
    }
    if r_int == 1 {
        alphas[0] = alpha_of(trapezoid_to_alpha(l, r), full);
        return;
    }
    let (first, last) = (ONE - l, r - ((r_int - 1) << 16));
    let last_h = fmul(last, dy);
    alphas[(r_int - 1) as usize] = fmul(last, last_h) >> 9;
    let mut alpha16 = last_h.saturating_add(dy >> 1);
    for i in (1..r_int - 1).rev() {
        alphas[i as usize] = ((alpha16 >> 8) & 0xFF).clamp(0, 255);
        alpha16 = alpha16.saturating_add(dy);
    }
    alphas[0] = (full - partial_triangle_to_alpha(first, dy)).clamp(0, 255);
}

/// Skia :707 — accumulate the analytic coverage of one trapezoid strip into a
/// row mask. `ul/ur` are the top x edges, `ll/lr` the bottom x edges, all
/// relative to the strip's left pixel and in 16.16. `ldy/rdy` are |dy/dx|.
fn blit_aaa_trapezoid_row(
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
    let len = right - left;
    if len <= 0 {
        return;
    }
    if len == 1 {
        mask[(origin + left) as usize] =
            mask[(origin + left) as usize].saturating_add(trapezoid_to_alpha(ur - ul, lr - ll));
        return;
    }
    let mut alphas = vec![full; len as usize];

    // Exclude the area left of the left edge (Skia :746-765).
    let u_left = trunc_fixed(ul);
    let l_left = ceil_fixed(ll);
    if u_left + 2 == l_left {
        let first = (u_left << 16) + ONE - ul;
        let second = ll - ul - first;
        let a1 = (full - partial_triangle_to_alpha(first, ldy)).clamp(0, 255);
        let a2 = partial_triangle_to_alpha(second, ldy);
        alphas[0] = (alphas[0] - a1).max(0);
        alphas[1] = (alphas[1] - a2).max(0);
    } else {
        let mut temp = vec![0i64; len as usize];
        let off = u_left - left;
        if off >= 0 && off < len {
            let count = (temp.len() - off as usize).min((l_left - u_left) as usize);
            compute_alpha_below_line(
                &mut temp[off as usize..off as usize + count],
                ul - (u_left << 16),
                ll - (u_left << 16),
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
    if u_right + 2 == l_right {
        let first = (u_right << 16) + ONE - ur;
        let second = lr - ur - first;
        let a1 = partial_triangle_to_alpha(first, rdy);
        let a2 = (full - partial_triangle_to_alpha(second, rdy)).clamp(0, 255);
        let i2 = (len - 2) as usize;
        alphas[i2] = (alphas[i2] - a1).max(0);
        alphas[len as usize - 1] = (alphas[len as usize - 1] - a2).max(0);
    } else {
        let mut temp = vec![0i64; len as usize];
        let off = u_right - left;
        if off >= 0 && off < len {
            let count = (temp.len() - off as usize).min((l_right - u_right) as usize);
            compute_alpha_above_line(
                &mut temp[off as usize..off as usize + count],
                ur - (u_right << 16),
                lr - (u_right << 16),
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
        mask[(origin + left + i as i64) as usize] =
            mask[(origin + left + i as i64) as usize].saturating_add(a);
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
    s.x0 + mul_div(y - s.y0, s.x1 - s.x0, s.y1 - s.y0)
}

fn slope(s: &Segment) -> i64 {
    if s.x0 == s.x1 {
        return MAX_SLOPE;
    }
    (s.y1 - s.y0).abs().saturating_mul(ONE) / (s.x1 - s.x0).abs().max(1)
}

/// Exact convex coverage for one pixel row band, accumulated into `row`
/// (one i64 mask slot per pixel, starting at `origin`). Breakpoints are the
/// integer row ends plus every segment endpoint y inside the row, mirroring
/// Skia's scan-line set (file header :47-53).
pub fn rasterize_convex_row(segments: &[Segment], row: &mut [i64], origin: i64, y: i64) {
    let y_top = y << 16;
    let y_bot = (y + 1) << 16;
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
        let last_x = ceil_fixed(ur).max(ceil_fixed(lr)).max(first_x + 1);
        let width = (last_x - first_x) as usize;
        let mut strip = vec![0i64; width];
        let base = first_x - origin;
        if base >= 0 && (base as usize) + width <= row.len() {
            blit_aaa_trapezoid_row(
                &mut strip,
                0,
                ul - (first_x << 16),
                ur - (first_x << 16),
                ll - (first_x << 16),
                lr - (first_x << 16),
                ls,
                rs,
                255,
            );
            let h = c1 - c0;
            for (i, a) in strip.into_iter().enumerate() {
                row[(base as usize) + i] =
                    row[(base as usize) + i].saturating_add(partial_alpha(a, h));
            }
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
pub fn conic_quad_pow2(radius: i64, tol_num: i64, tol_den: i64) -> u32 {
    let r = radius.max(0) as i128;
    let err2 = (CONIC_ERR_NUM * r) * (CONIC_ERR_NUM * r);
    let tol = (tol_num.max(0) as i128 * CONIC_ERR_DEN) / tol_den.max(1) as i128;
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
        let (x1, y1) = (x + w, y + h);
        let f = |v: i64| v << 16;
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
    let n = 1usize << conic_quad_pow2(r, 1, 8);
    let mut pts: Vec<(i64, i64)> = Vec::with_capacity(4 * n + 4);
    // Unit quarter arc (c,s) = ((1-u^2)/(1+u^2), 2u/(1+u^2)), u in [0,1], so
    // (c,s) runs (1,0) -> (0,1). Each corner maps it onto its own tangent pair.
    // All math is 16.16 fixed point, no floats.
    // (mx/my sign mapping kept explicit per corner below.)
    // Top-left: (cx - r*c, cy - r*s), left -> top.
    {
        let (cx, cy) = (x + r, y + r);
        for i in 0..=n {
            let u = i as i128 * ONE as i128 / n as i128;
            let u2 = u * u / ONE as i128;
            let den = ONE as i128 + u2;
            let c = ((ONE as i128 - u2) << 16) / den;
            let s = (2 * u << 16) / den;
            pts.push((cx * ONE - (c as i64 * r), cy * ONE - (s as i64 * r)));
        }
    }
    // Top-right: (cx + s, cy - c), top -> right.
    {
        let (cx, cy) = (x + w - r, y + r);
        for i in 0..=n {
            let u = i as i128 * ONE as i128 / n as i128;
            let u2 = u * u / ONE as i128;
            let den = ONE as i128 + u2;
            let c = ((ONE as i128 - u2) << 16) / den;
            let s = (2 * u << 16) / den;
            pts.push((cx * ONE + (s as i64 * r), cy * ONE - (c as i64 * r)));
        }
    }
    // Bottom-right: (cx + c, cy + s), right -> bottom.
    {
        let (cx, cy) = (x + w - r, y + h - r);
        for i in 0..=n {
            let u = i as i128 * ONE as i128 / n as i128;
            let u2 = u * u / ONE as i128;
            let den = ONE as i128 + u2;
            let c = ((ONE as i128 - u2) << 16) / den;
            let s = (2 * u << 16) / den;
            pts.push((cx * ONE + (c as i64 * r), cy * ONE + (s as i64 * r)));
        }
    }
    // Bottom-left: (cx - s, cy + c), bottom -> left.
    {
        let (cx, cy) = (x + r, y + h - r);
        for i in 0..=n {
            let u = i as i128 * ONE as i128 / n as i128;
            let u2 = u * u / ONE as i128;
            let den = ONE as i128 + u2;
            let c = ((ONE as i128 - u2) << 16) / den;
            let s = (2 * u << 16) / den;
            pts.push((cx * ONE - (s as i64 * r), cy * ONE + (c as i64 * r)));
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
