//! Visual effects: deterministic RGBA8 integer compositing and the command
//! stream that reproduces it on the independent Metal compute backend.
//!
//! The software framebuffer uses integer arithmetic only. Effect-free IR emits
//! no effect commands, so the effect-free command stream stays byte-identical.

use crate::display::Command;
use crate::raster::Canvas;
use crate::{LaidOut, Rect, Rgba};

/// Fixed-point alpha unit: 1024 is fully opaque.
pub const EFFECT_UNIT: i64 = 1024;
/// Blur, glow, glass and shadow radii are clamped to this bound.
pub const EFFECT_MAX_RADIUS: i64 = 32;
/// Shadow offsets are clamped to this absolute bound.
pub const EFFECT_MAX_OFFSET: i64 = 64;

const LIGHT: Rgba = Rgba::rgb(255, 255, 255);
const DARK: Rgba = Rgba::rgb(0, 0, 0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisualEffectKind {
    Blur,
    Shadow,
    Glow,
    Glass,
    Liquid,
}

impl VisualEffectKind {
    pub fn parse(s: &str) -> Option<VisualEffectKind> {
        Some(match s {
            "blur" => VisualEffectKind::Blur,
            "shadow" => VisualEffectKind::Shadow,
            "glow" => VisualEffectKind::Glow,
            "glass" => VisualEffectKind::Glass,
            "liquid" => VisualEffectKind::Liquid,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            VisualEffectKind::Blur => "blur",
            VisualEffectKind::Shadow => "shadow",
            VisualEffectKind::Glow => "glow",
            VisualEffectKind::Glass => "glass",
            VisualEffectKind::Liquid => "liquid",
        }
    }
}

/// One optional `props.effects` entry. All fields are integer fixed point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisualEffect {
    pub kind: VisualEffectKind,
    pub radius: i64,
    pub dx: i64,
    pub dy: i64,
    pub alpha: i64,
    pub color: Option<Rgba>,
}

impl VisualEffect {
    pub fn radius(&self) -> i64 {
        self.radius.clamp(0, EFFECT_MAX_RADIUS)
    }

    pub fn dx(&self) -> i64 {
        self.dx.clamp(-EFFECT_MAX_OFFSET, EFFECT_MAX_OFFSET)
    }

    pub fn dy(&self) -> i64 {
        self.dy.clamp(-EFFECT_MAX_OFFSET, EFFECT_MAX_OFFSET)
    }

    pub fn alpha(&self) -> i64 {
        self.alpha.clamp(0, EFFECT_UNIT)
    }

    fn pad(&self) -> i64 {
        self.radius() + self.dx().abs().max(self.dy().abs())
    }
}

/// A blur-only node is handled as a final whole-frame pass; the other kinds
/// need the frame as it was immediately before the node was drawn.
pub(crate) fn has_local_effects(n: &LaidOut) -> bool {
    n.visual_effects
        .iter()
        .any(|e| e.kind != VisualEffectKind::Blur)
}

/// Apply local effects after a node's ordinary box, control and text drawing.
/// `backdrop` is the full RGBA frame captured before that ordinary drawing.
pub(crate) fn apply_node(canvas: &mut Canvas, n: &LaidOut, backdrop: &[u8]) {
    if canvas.is_capturing() {
        return;
    }
    let local: Vec<&VisualEffect> = n
        .visual_effects
        .iter()
        .filter(|e| e.kind != VisualEffectKind::Blur)
        .collect();
    if local.is_empty() {
        return;
    }
    let pad = local
        .iter()
        .map(|e| e.pad())
        .max()
        .unwrap_or(0)
        .min(EFFECT_MAX_RADIUS + EFFECT_MAX_OFFSET);
    let bounds = clipped_area(
        canvas.width,
        canvas.height,
        canvas.effective_clip(),
        n.rect,
        pad,
    );
    if bounds.w <= 0 || bounds.h <= 0 {
        return;
    }
    let mask = build_mask(canvas, n, backdrop, bounds);
    let pw = bounds.w as usize;
    let ph = bounds.h as usize;
    for e in local {
        match e.kind {
            VisualEffectKind::Shadow => {
                let soft = blur_mask(&mask, pw, ph, e.radius());
                shadow(canvas, e, bounds, &mask, &soft);
            }
            VisualEffectKind::Glow => {
                let soft = blur_mask(&mask, pw, ph, e.radius());
                glow(canvas, n, e, bounds, &mask, &soft);
            }
            VisualEffectKind::Glass => glass(canvas, n, backdrop, e),
            VisualEffectKind::Liquid => liquid(canvas, n, e),
            VisualEffectKind::Blur => {}
        }
    }
}

/// Apply every visible node's blur effects, in pre-order layout order, to the
/// complete RGBA frame. This is a separate pass so child nodes drawn after a
/// blur node are included in the frame that gets blurred.
pub(crate) fn apply_frame_blurs(canvas: &mut Canvas, laid: &[LaidOut]) {
    if canvas.is_capturing() {
        return;
    }
    for n in laid {
        for e in &n.visual_effects {
            if e.kind == VisualEffectKind::Blur {
                let mut blurred = vec![0u8; canvas.pixels.len()];
                box_blur_plane::<4>(
                    &canvas.pixels,
                    &mut blurred,
                    canvas.width as usize,
                    canvas.height as usize,
                    e.radius(),
                );
                canvas.pixels = blurred;
            }
        }
    }
}

/// Geometry and colours the effect commands need for one node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EffectFrame {
    pub has_local: bool,
    pub rect: Rect,
    pub clip: Rect,
    pub bounds: Rect,
    pub inner: Rect,
    pub shape_radius: i64,
    pub shape: bool,
    pub ink: Rgba,
}

impl EffectFrame {
    pub(crate) fn new(width: u32, height: u32, n: &LaidOut) -> EffectFrame {
        let local: Vec<&VisualEffect> = n
            .visual_effects
            .iter()
            .filter(|e| e.kind != VisualEffectKind::Blur)
            .collect();
        let clip = effective_clip(width, height, n.clip);
        let pad = local
            .iter()
            .map(|e| e.pad())
            .max()
            .unwrap_or(0)
            .min(EFFECT_MAX_RADIUS + EFFECT_MAX_OFFSET);
        EffectFrame {
            has_local: !local.is_empty(),
            rect: n.rect,
            clip,
            bounds: clipped_area(width, height, clip, n.rect, pad),
            inner: clipped_area(width, height, clip, n.rect, 0),
            shape_radius: shape_radius(n),
            shape: n.fill.is_some(),
            ink: ink(n),
        }
    }
}

/// The mask snapshot followed by one command per local effect, in IR order.
pub(crate) fn local_commands(frame: EffectFrame, effects: &[VisualEffect]) -> Vec<Command> {
    let mut out = vec![Command::EffectMask {
        rect: frame.rect,
        clip: frame.clip,
        bounds: frame.bounds,
        shape_radius: frame.shape_radius,
        shape: frame.shape,
    }];
    for e in effects.iter().filter(|e| e.kind != VisualEffectKind::Blur) {
        let color = match e.kind {
            VisualEffectKind::Shadow => e.color.unwrap_or(DARK),
            VisualEffectKind::Glow => e.color.unwrap_or(frame.ink),
            _ => e.color.unwrap_or(LIGHT),
        };
        out.push(match e.kind {
            VisualEffectKind::Shadow => Command::EffectShadow {
                bounds: frame.bounds,
                radius: e.radius(),
                dx: e.dx(),
                dy: e.dy(),
                alpha: e.alpha(),
                color,
            },
            VisualEffectKind::Glow => Command::EffectGlow {
                bounds: frame.bounds,
                radius: e.radius(),
                alpha: e.alpha(),
                color,
            },
            VisualEffectKind::Glass => Command::EffectGlass {
                rect: frame.rect,
                clip: frame.clip,
                bounds: frame.inner,
                shape_radius: frame.shape_radius,
                radius: e.radius(),
                alpha: e.alpha(),
                color,
            },
            VisualEffectKind::Liquid => Command::EffectLiquid {
                rect: frame.rect,
                clip: frame.clip,
                bounds: frame.inner,
                shape_radius: frame.shape_radius,
                denom: liquid_denom(frame.rect),
                alpha: e.alpha(),
                color,
            },
            VisualEffectKind::Blur => unreachable!("blur is a frame pass"),
        });
    }
    out
}

/// Whole-frame blur passes in pre-order layout order, after every node.
pub(crate) fn frame_blur_commands(laid: &[LaidOut]) -> Vec<Command> {
    laid.iter()
        .flat_map(|n| n.visual_effects.iter())
        .filter(|e| e.kind == VisualEffectKind::Blur)
        .map(|e| Command::EffectBlur { radius: e.radius() })
        .collect()
}

/// Separable box blur over an N-channel byte plane. Window ends are clamped to
/// the plane, so edge windows have fewer samples. Each channel uses exact
/// half-up rounding: `(sum + count / 2) / count`.
fn box_blur_plane<const N: usize>(src: &[u8], dst: &mut [u8], w: usize, h: usize, radius: i64) {
    let r = radius.clamp(0, EFFECT_MAX_RADIUS) as usize;
    let mut tmp = vec![0u8; src.len()];
    let mut row = vec![[0i64; N]; w + 1];
    for y in 0..h {
        row[0] = [0i64; N];
        for x in 0..w {
            let i = (y * w + x) * N;
            for k in 0..N {
                row[x + 1][k] = row[x][k] + src[i + k] as i64;
            }
        }
        for x in 0..w {
            let left = x.saturating_sub(r);
            let right = (x + r + 1).min(w);
            let count = (right - left) as i64;
            let o = (y * w + x) * N;
            for k in 0..N {
                let sum = row[right][k] - row[left][k];
                tmp[o + k] = ((sum + count / 2) / count).clamp(0, 255) as u8;
            }
        }
    }
    let mut column = vec![[0i64; N]; h + 1];
    for x in 0..w {
        column[0] = [0i64; N];
        for y in 0..h {
            let i = (y * w + x) * N;
            for k in 0..N {
                column[y + 1][k] = column[y][k] + tmp[i + k] as i64;
            }
        }
        for y in 0..h {
            let top = y.saturating_sub(r);
            let bottom = (y + r + 1).min(h);
            let count = (bottom - top) as i64;
            let o = (y * w + x) * N;
            for k in 0..N {
                let sum = column[bottom][k] - column[top][k];
                dst[o + k] = ((sum + count / 2) / count).clamp(0, 255) as u8;
            }
        }
    }
}

fn blur_mask(mask: &[u8], w: usize, h: usize, radius: i64) -> Vec<u8> {
    let mut soft = vec![0u8; mask.len()];
    box_blur_plane::<1>(mask, &mut soft, w, h, radius);
    soft
}

fn alpha_u8(alpha: i64) -> u8 {
    let fixed = alpha.clamp(0, EFFECT_UNIT);
    ((fixed * 255 + EFFECT_UNIT / 2) / EFFECT_UNIT).clamp(0, 255) as u8
}

fn blend(dst: [u8; 4], src: Rgba, a: u8) -> [u8; 4] {
    let a = a as i64;
    let inv = 255 - a;
    let mix = |s: u8, d: u8| ((s as i64 * a + d as i64 * inv + 127) / 255).clamp(0, 255) as u8;
    [
        mix(src.r, dst[0]),
        mix(src.g, dst[1]),
        mix(src.b, dst[2]),
        255,
    ]
}

fn pixel(data: &[u8], width: usize, x: i64, y: i64) -> [u8; 4] {
    let i = (y as usize * width + x as usize) * 4;
    [data[i], data[i + 1], data[i + 2], data[i + 3]]
}

fn put(canvas: &mut Canvas, x: i64, y: i64, c: [u8; 4]) {
    if x < 0 || y < 0 || x >= canvas.width as i64 || y >= canvas.height as i64 {
        return;
    }
    let i = (y as usize * canvas.width as usize + x as usize) * 4;
    canvas.pixels[i..i + 4].copy_from_slice(&c);
}

/// The clip a canvas applies to its own draws, normalized to the viewport.
pub(crate) fn effective_clip(width: u32, height: u32, clip: Option<Rect>) -> Rect {
    let full = Rect {
        x: 0,
        y: 0,
        w: width as i64,
        h: height as i64,
    };
    match clip {
        None => full,
        Some(c) => {
            let x0 = c.x.max(0);
            let y0 = c.y.max(0);
            let x1 = c.x.saturating_add(c.w).min(full.w);
            let y1 = c.y.saturating_add(c.h).min(full.h);
            Rect {
                x: x0,
                y: y0,
                w: (x1 - x0).max(0),
                h: (y1 - y0).max(0),
            }
        }
    }
}

fn clipped_area(width: u32, height: u32, clip: Rect, r: Rect, pad: i64) -> Rect {
    let x0 = r.x.saturating_sub(pad).max(clip.x).max(0);
    let y0 = r.y.saturating_sub(pad).max(clip.y).max(0);
    let x1 =
        r.x.saturating_add(r.w.max(0))
            .saturating_add(pad)
            .min(clip.x.saturating_add(clip.w))
            .min(width as i64);
    let y1 =
        r.y.saturating_add(r.h.max(0))
            .saturating_add(pad)
            .min(clip.y.saturating_add(clip.h))
            .min(height as i64);
    Rect {
        x: x0,
        y: y0,
        w: (x1 - x0).max(0),
        h: (y1 - y0).max(0),
    }
}

fn shape_radius(n: &LaidOut) -> i64 {
    n.radius.max(0).min(n.rect.w.min(n.rect.h).max(0) / 2)
}

fn inside_shape(n: &LaidOut, x: i64, y: i64, rad: i64) -> bool {
    if x < n.rect.x
        || y < n.rect.y
        || x >= n.rect.x.saturating_add(n.rect.w.max(0))
        || y >= n.rect.y.saturating_add(n.rect.h.max(0))
    {
        return false;
    }
    !Canvas::outside_round(n.rect, x, y, rad)
}

fn build_mask(canvas: &Canvas, n: &LaidOut, backdrop: &[u8], bounds: Rect) -> Vec<u8> {
    let w = bounds.w as usize;
    let h = bounds.h as usize;
    let clip = canvas.effective_clip();
    let rad = shape_radius(n);
    let mut mask = vec![0u8; w * h];
    for iy in 0..h {
        let y = bounds.y + iy as i64;
        for ix in 0..w {
            let x = bounds.x + ix as i64;
            if x < clip.x
                || y < clip.y
                || x >= clip.x.saturating_add(clip.w)
                || y >= clip.y.saturating_add(clip.h)
            {
                continue;
            }
            let silhouette = if n.fill.is_some() {
                inside_shape(n, x, y, rad)
            } else {
                pixel(&canvas.pixels, canvas.width as usize, x, y)
                    != pixel(backdrop, canvas.width as usize, x, y)
            };
            if silhouette {
                mask[iy * w + ix] = 255;
            }
        }
    }
    mask
}

fn sample_mask(mask: &[u8], bounds: Rect, x: i64, y: i64) -> u8 {
    if x < bounds.x
        || y < bounds.y
        || x >= bounds.x.saturating_add(bounds.w)
        || y >= bounds.y.saturating_add(bounds.h)
    {
        return 0;
    }
    let ix = (x - bounds.x) as usize;
    let iy = (y - bounds.y) as usize;
    mask[iy * bounds.w as usize + ix]
}

fn ink(n: &LaidOut) -> Rgba {
    n.text_color.or(n.border).or(n.fill).unwrap_or(n.accent)
}

fn composite(canvas: &mut Canvas, x: i64, y: i64, color: Rgba, a: u8) {
    if a == 0 {
        return;
    }
    let dst = pixel(&canvas.pixels, canvas.width as usize, x, y);
    put(canvas, x, y, blend(dst, color, a));
}

fn shadow(canvas: &mut Canvas, e: &VisualEffect, bounds: Rect, mask: &[u8], soft: &[u8]) {
    let base = alpha_u8(e.alpha()) as i64;
    let color = e.color.unwrap_or(DARK);
    for y in bounds.y..bounds.y.saturating_add(bounds.h) {
        for x in bounds.x..bounds.x.saturating_add(bounds.w) {
            if sample_mask(mask, bounds, x, y) != 0 {
                continue; // shadow is behind the node
            }
            let sx = x.saturating_sub(e.dx());
            let sy = y.saturating_sub(e.dy());
            let coverage = sample_mask(soft, bounds, sx, sy) as i64;
            let a = ((base * coverage + 127) / 255).clamp(0, 255) as u8;
            composite(canvas, x, y, color, a);
        }
    }
}

fn glow(
    canvas: &mut Canvas,
    n: &LaidOut,
    e: &VisualEffect,
    bounds: Rect,
    mask: &[u8],
    soft: &[u8],
) {
    let base = alpha_u8(e.alpha()) as i64;
    let color = e.color.unwrap_or_else(|| ink(n));
    for y in bounds.y..bounds.y.saturating_add(bounds.h) {
        for x in bounds.x..bounds.x.saturating_add(bounds.w) {
            let local = (y - bounds.y) as usize * bounds.w as usize + (x - bounds.x) as usize;
            if mask[local] != 0 {
                continue; // outward halo only
            }
            let coverage = soft[local] as i64;
            let a = ((base * coverage + 127) / 255).clamp(0, 255) as u8;
            composite(canvas, x, y, color, a);
        }
    }
}

fn glass(canvas: &mut Canvas, n: &LaidOut, backdrop: &[u8], e: &VisualEffect) {
    let w = canvas.width as usize;
    let h = canvas.height as usize;
    let mut blurred = vec![0u8; backdrop.len()];
    box_blur_plane::<4>(backdrop, &mut blurred, w, h, e.radius());
    let bounds = clipped_area(
        canvas.width,
        canvas.height,
        canvas.effective_clip(),
        n.rect,
        0,
    );
    let rad = shape_radius(n);
    let tint = e.color.unwrap_or(LIGHT);
    let a = alpha_u8(e.alpha());
    for y in bounds.y..bounds.y.saturating_add(bounds.h) {
        for x in bounds.x..bounds.x.saturating_add(bounds.w) {
            if !inside_shape(n, x, y, rad) {
                continue;
            }
            let base = pixel(&blurred, w, x, y);
            put(canvas, x, y, blend(base, tint, a));
        }
    }
}

fn liquid_denom(r: Rect) -> i64 {
    r.w.max(1)
        .saturating_add(r.h.max(1))
        .saturating_sub(2)
        .max(1)
}

fn liquid(canvas: &mut Canvas, n: &LaidOut, e: &VisualEffect) {
    let bounds = clipped_area(
        canvas.width,
        canvas.height,
        canvas.effective_clip(),
        n.rect,
        0,
    );
    let rad = shape_radius(n);
    let highlight = e.color.unwrap_or(LIGHT);
    let base = alpha_u8(e.alpha()) as i64;
    let denom = liquid_denom(n.rect);
    for y in bounds.y..bounds.y.saturating_add(bounds.h) {
        for x in bounds.x..bounds.x.saturating_add(bounds.w) {
            if !inside_shape(n, x, y, rad) {
                continue;
            }
            let d = x
                .saturating_sub(n.rect.x)
                .saturating_add(y.saturating_sub(n.rect.y));
            let t = ((d as i128 * EFFECT_UNIT as i128 / denom as i128)
                .clamp(0, EFFECT_UNIT as i128)) as i64;
            let band = (EFFECT_UNIT - (t * 3 - EFFECT_UNIT).abs()).max(0);
            let a = ((base * band + EFFECT_UNIT / 2) / EFFECT_UNIT).clamp(0, 255) as u8;
            let dst = pixel(&canvas.pixels, canvas.width as usize, x, y);
            put(canvas, x, y, blend(dst, highlight, a));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_blur_is_separable_with_half_up_rounding() {
        let mut src = vec![0u8; 9];
        src[4] = 255;
        let mut dst = vec![0u8; 9];
        box_blur_plane::<1>(&src, &mut dst, 3, 3, 1);
        assert_eq!(dst, vec![64, 43, 64, 43, 28, 43, 64, 43, 64]);
        box_blur_plane::<1>(&src, &mut dst, 3, 3, 0);
        assert_eq!(dst, src);
    }

    #[test]
    fn effect_alpha_and_pad_are_bounded() {
        assert_eq!(alpha_u8(i64::MIN), 0);
        assert_eq!(alpha_u8(EFFECT_UNIT / 2), 128);
        assert_eq!(alpha_u8(EFFECT_UNIT), 255);
        let e = VisualEffect {
            kind: VisualEffectKind::Shadow,
            radius: i64::MAX,
            dx: i64::MIN,
            dy: i64::MAX,
            alpha: i64::MAX,
            color: None,
        };
        assert_eq!(e.radius(), EFFECT_MAX_RADIUS);
        assert_eq!(e.dx(), -EFFECT_MAX_OFFSET);
        assert_eq!(e.dy(), EFFECT_MAX_OFFSET);
        assert_eq!(e.alpha(), EFFECT_UNIT);
        assert_eq!(e.pad(), EFFECT_MAX_RADIUS + EFFECT_MAX_OFFSET);
    }
}
