//! Deterministic particle fields: a fixed-count scatter of integer dots over a
//! rectangle.
//!
//! Particles are a spatial primitive, not an animation: every dot position is a
//! pure integer function of the seed and the dot index, so the same command
//! paints the same bytes on every host. The display-list command
//! `{"kind":"particles",...}` carries the same bounded integers and is
//! evaluated per pixel by the cross-repo GPU backend with identical
//! arithmetic.

use crate::effects::EFFECT_UNIT;
use crate::{Rect, Rgba};

/// Maximum particle count; parse-time bound shared with the GPU parser.
pub const PARTICLE_MAX_COUNT: i64 = 512;
/// Maximum dot radius in pixels; parse-time bound shared with the GPU parser.
pub const PARTICLE_MAX_RADIUS: i64 = 32;
/// Maximum seed value; the position hash operates on 32-bit integers.
pub const PARTICLE_MAX_SEED: i64 = u32::MAX as i64;

/// One node's opt-in particle field. `alpha` is fixed point (`1024` is
/// fully opaque); `color` falls back to the node accent at draw time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Particles {
    pub count: i64,
    pub seed: i64,
    pub radius: i64,
    pub alpha: i64,
    pub color: Option<Rgba>,
}

/// Integer avalanche mix of one 32-bit word. It is the only randomness source:
/// no float, no clock, no global generator state.
fn mix(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

/// Centre of dot `index` inside `rect`. Two independent hashes of
/// `(seed, index)` are reduced with modulo into the rect, so dot order never
/// depends on the viewport, the frame buffer or any state outside the command.
/// `rect` must have positive width and height within the display-list integer
/// range; the caller guarantees this at every call site.
pub fn dot_center(rect: Rect, seed: i64, index: u32) -> (i64, i64) {
    let s = seed as u32;
    // The two constants decorrelate the axes; `u32` wrapping arithmetic keeps
    // the expression identical to the GPU backend's unsigned MSL math.
    let hx = mix(s ^ index.wrapping_mul(0x9e37_79b9).wrapping_add(0x6c07_8965));
    let hy = mix(s ^ index.wrapping_mul(0x9e37_79b9).wrapping_add(0x85eb_ca6b));
    (
        rect.x + i64::from(hx % rect.w as u32),
        rect.y + i64::from(hy % rect.h as u32),
    )
}

/// Byte blend alpha for a fixed-point particle alpha: `alpha * 255 / 1024`,
/// the same truncating reduction the gradient fill applies to its alpha.
pub(crate) fn alpha_byte(alpha: i64) -> u8 {
    ((alpha.clamp(0, EFFECT_UNIT) * 255) / EFFECT_UNIT) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_centers_stay_inside_the_rect_and_repeat_exactly() {
        let rect = Rect {
            x: -3,
            y: 7,
            w: 41,
            h: 23,
        };
        for seed in [0i64, 1, 7, 0x9237_ffff, PARTICLE_MAX_SEED] {
            for i in 0..PARTICLE_MAX_COUNT as u32 {
                let (cx, cy) = dot_center(rect, seed, i);
                assert!((cx - rect.x) < rect.w, "cx {cx} outside rect");
                assert!((cy - rect.y) < rect.h, "cy {cy} outside rect");
            }
            let first = dot_center(rect, seed, 17);
            assert_eq!(first, dot_center(rect, seed, 17));
            assert_ne!(first, dot_center(rect, seed, 18));
        }
    }

    #[test]
    fn alpha_byte_matches_the_gradient_reduction() {
        assert_eq!(alpha_byte(0), 0);
        assert_eq!(alpha_byte(512), 127);
        assert_eq!(alpha_byte(1024), 255);
        assert_eq!(alpha_byte(-5), 0);
        assert_eq!(alpha_byte(2000), 255);
    }
}
