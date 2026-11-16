//! Integer fixed-point cubic Bezier evaluator for opt-in vector smoothing.
//! Every intermediate is an integer; no float and no host math library.

/// Fixed-point unit for the curve parameter.
const T_UNIT: i64 = 1 << 16;

/// Evaluate a cubic Bezier at `t` in `0..=T_UNIT`. The exact rational point is
/// rounded to the nearest integer coordinate, so the result is within one
/// integer unit of the analytic curve with zero floating-point arithmetic.
pub fn cubic_bezier(
    p0: (i64, i64),
    p1: (i64, i64),
    p2: (i64, i64),
    p3: (i64, i64),
    t: i64,
) -> (i64, i64) {
    let t = t.clamp(0, T_UNIT);
    let u = T_UNIT - t;
    let ti = t as i128;
    let ui = u as i128;
    let denom = (T_UNIT as i128) * (T_UNIT as i128) * (T_UNIT as i128);
    let round = denom / 2;
    let axis = |a: i64, b: i64, c: i64, d: i64| -> i64 {
        let num = ui * ui * ui * a as i128
            + 3 * ui * ui * ti * b as i128
            + 3 * ui * ti * ti * c as i128
            + ti * ti * ti * d as i128;
        ((num + round) / denom) as i64
    };
    (
        axis(p0.0, p1.0, p2.0, p3.0),
        axis(p0.1, p1.1, p2.1, p3.1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_and_clamped_parameters_are_exact() {
        let (a, b, c, d) = ((0, 0), (0, 100), (100, 100), (100, 0));
        assert_eq!(cubic_bezier(a, b, c, d, 0), (0, 0));
        assert_eq!(cubic_bezier(a, b, c, d, T_UNIT), (100, 0));
        assert_eq!(cubic_bezier(a, b, c, d, -9), (0, 0));
        assert_eq!(cubic_bezier(a, b, c, d, T_UNIT + 9), (100, 0));
    }
}
