//! Visual effects: deterministic RGBA8 integer compositing and the command
//! stream that reproduces it on the independent Metal compute backend.
//!
//! The software framebuffer uses integer arithmetic only. Effect-free IR emits
//! no effect commands, so the effect-free command stream stays byte-identical.

use crate::display::Command;
use crate::geometry::{CornerRadii, RoundingRadii};
use crate::raster::Canvas;
use crate::{LaidOut, Rect, Rgba};

/// Fixed-point alpha unit: 1024 is fully opaque.
pub const EFFECT_UNIT: i64 = 1024;
/// Blur, glow, glass and shadow radii are clamped to this bound.
pub const EFFECT_MAX_RADIUS: i64 = 32;
/// Shadow offsets are clamped to this absolute bound.
pub const EFFECT_MAX_OFFSET: i64 = 64;
/// Discrete-Gaussian radius range, mirroring the GPU kernel's accepted window.
pub const GAUSSIAN_MIN_RADIUS: i64 = 1;
pub const GAUSSIAN_MAX_RADIUS: i64 = 8;

/// Frame passes run after every node; local effects run right after their node.
fn is_frame_pass(kind: VisualEffectKind) -> bool {
    matches!(kind, VisualEffectKind::Blur | VisualEffectKind::Gaussian)
}

const LIGHT: Rgba = Rgba::rgb(255, 255, 255);
const DARK: Rgba = Rgba::rgb(0, 0, 0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisualEffectKind {
    Blur,
    /// Whole-frame discrete-Gaussian blur, the GPU `effect_gaussian_blur` pass.
    Gaussian,
    Shadow,
    Glow,
    Glass,
    Liquid,
}

impl VisualEffectKind {
    pub fn parse(s: &str) -> Option<VisualEffectKind> {
        Some(match s {
            "blur" => VisualEffectKind::Blur,
            "gaussian" => VisualEffectKind::Gaussian,
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
            VisualEffectKind::Gaussian => "gaussian",
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
    n.visual_effects.iter().any(|e| !is_frame_pass(e.kind))
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
        .filter(|e| !is_frame_pass(e.kind))
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
            VisualEffectKind::Blur | VisualEffectKind::Gaussian => {}
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
            match e.kind {
                VisualEffectKind::Blur => {
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
                VisualEffectKind::Gaussian => {
                    gaussian_blur_frame(
                        &mut canvas.pixels,
                        canvas.width,
                        canvas.height,
                        e.radius(),
                    );
                }
                _ => {}
            }
        }
    }
}

/// Whole-frame discrete-Gaussian blur: separable horizontal then vertical 1D
/// passes with the binomial row `C(2r, 0..=2r)`, clamped edges and half-up
/// rounding — the exact arithmetic of the GPU `gaussian_blur_cpu` oracle, so
/// the software frame and the command stream agree byte for byte.
fn gaussian_blur_frame(pixels: &mut [u8], width: u32, height: u32, radius: i64) {
    let r = radius.clamp(GAUSSIAN_MIN_RADIUS, GAUSSIAN_MAX_RADIUS);
    let n = 2 * r;
    let mut weights = vec![1i64; (n + 1) as usize];
    for k in 1..=n as usize {
        weights[k] = weights[k - 1] * (n - k as i64 + 1) / k as i64;
    }
    let sum: i64 = weights.iter().sum();
    let (w, h) = (width as i64, height as i64);
    let taps = 2 * r;
    let mut horizontal = vec![0u8; pixels.len()];
    for y in 0..h {
        for x in 0..w {
            for k in 0..4i64 {
                let mut acc = 0i64;
                for j in 0..=taps {
                    let sx = (x + j - r).clamp(0, w - 1);
                    let idx = ((y * w + sx) * 4 + k) as usize;
                    acc += i64::from(pixels[idx]) * weights[j as usize];
                }
                let out = (acc + sum / 2) / sum;
                horizontal[((y * w + x) * 4 + k) as usize] = out.clamp(0, 255) as u8;
            }
        }
    }
    for y in 0..h {
        for x in 0..w {
            for k in 0..4i64 {
                let mut acc = 0i64;
                for j in 0..=taps {
                    let sy = (y + j - r).clamp(0, h - 1);
                    let idx = ((sy * w + x) * 4 + k) as usize;
                    acc += i64::from(horizontal[idx]) * weights[j as usize];
                }
                let out = (acc + sum / 2) / sum;
                pixels[((y * w + x) * 4 + k) as usize] = out.clamp(0, 255) as u8;
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
        let bounds = clipped_area(width, height, clip, n.rect, pad);
        EffectFrame {
            // A node with no visible area paints no local effects: the software
            // path would no-op anyway, and the GPU rejects dead bounds.
            has_local: !local.is_empty() && bounds.w > 0 && bounds.h > 0,
            rect: n.rect,
            clip,
            bounds,
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
    for e in effects.iter().filter(|e| !is_frame_pass(e.kind)) {
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
            VisualEffectKind::Blur | VisualEffectKind::Gaussian => {
                unreachable!("frame passes are whole-frame commands")
            }
        });
    }
    out
}

/// Whole-frame blur passes in pre-order layout order, after every node. Box
/// blurs and Gaussian blurs keep their IR order; each maps to its own command.
pub(crate) fn frame_blur_commands(laid: &[LaidOut]) -> Vec<Command> {
    laid.iter()
        .flat_map(|n| n.visual_effects.iter())
        .filter(|e| is_frame_pass(e.kind))
        .map(|e| match e.kind {
            VisualEffectKind::Gaussian => Command::EffectGaussianBlur {
                radius: e.radius().clamp(GAUSSIAN_MIN_RADIUS, GAUSSIAN_MAX_RADIUS),
            },
            _ => Command::EffectBlur { radius: e.radius() },
        })
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

/// True only when the node's radial geometry came from `cornerRadii` rather
/// than the scalar `radius`. Uniform/elliptical nodes keep the byte-identical
/// scalar shadow path; any explicit per-corner data takes the new path.
fn has_per_corner_radii(n: &LaidOut) -> bool {
    n.radii != RoundingRadii::all(shape_radius(n))
}

/// Per-layer shadow radii: grow by positive spread and clamp each axis into
/// the layer box, mirroring the scalar `lrad` rule. Flutter `_paintShadows`
/// offsets/inflates the rect then `_paintBox` reuses the same borderRadius.
fn shadow_layer_radii(radii: RoundingRadii, spread: i64, box_r: Rect) -> RoundingRadii {
    let grow = spread.max(0);
    let max_x = (box_r.w / 2).max(0);
    let max_y = (box_r.h / 2).max(0);
    let grow_corner = |r: CornerRadii| CornerRadii {
        x: r.x.saturating_add(grow).clamp(0, max_x),
        y: r.y.saturating_add(grow).clamp(0, max_y),
    };
    RoundingRadii {
        top_left: grow_corner(radii.top_left),
        top_right: grow_corner(radii.top_right),
        bottom_left: grow_corner(radii.bottom_left),
        bottom_right: grow_corner(radii.bottom_right),
    }
}

fn inside_shape(n: &LaidOut, x: i64, y: i64, rad: i64) -> bool {
    if x < n.rect.x
        || y < n.rect.y
        || x >= n.rect.x.saturating_add(n.rect.w.max(0))
        || y >= n.rect.y.saturating_add(n.rect.h.max(0))
    {
        return false;
    }
    if Canvas::uniform_radius(n.radii).is_none() {
        return crate::geometry::round_rect_contains(
            x, y, n.rect.x, n.rect.y, n.rect.w, n.rect.h, n.radii,
        );
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

/// Flutter Material elevation: one layer is `(offset_x, offset_y, blur,
/// spread, alpha)`. Alpha values are `@docImport` colours
/// `_kKeyUmbraOpacity` (#33000000), `_kKeyPenumbraOpacity` (#24000000) and
/// `_kAmbientShadowOpacity` (#1F000000).
///
/// Source of truth: Flutter stable 3.50.0, framework ab2912035e16,
/// `packages/flutter/lib/src/material/shadows.dart:40-221`
/// (`kElevationToShadow`, `:43-45` for the three colors). Each entry is
/// `[ambient, penumbra, umbra]`; the draw order is far-to-near.
/// Level 5 and levels above 24 are absent from Flutter's map, and level 0
/// maps to an empty shadow list.
pub type ElevationLayer = (i64, i64, i64, i64, u8);

pub const ELEVATION_TABLE: [Option<[ElevationLayer; 3]>; 25] = [
    None,                                                               // 0  shadows.dart:48
    Some([(0, 1, 3, 0, 31), (0, 1, 1, 0, 36), (0, 2, 1, -1, 51)]),      // 1  :50-59
    Some([(0, 1, 5, 0, 31), (0, 2, 2, 0, 36), (0, 3, 1, -2, 51)]),      // 2  :61-70
    Some([(0, 1, 8, 0, 31), (0, 3, 4, 0, 36), (0, 3, 3, -2, 51)]),      // 3  :72-81
    Some([(0, 1, 10, 0, 31), (0, 4, 5, 0, 36), (0, 2, 4, -1, 51)]),     // 4  :83-92
    None,                                                               // 5  (no level)
    Some([(0, 1, 18, 0, 31), (0, 6, 10, 0, 36), (0, 3, 5, -1, 51)]),    // 6  :94-103
    None,                                                               // 7  (no level)
    Some([(0, 3, 14, 2, 31), (0, 8, 10, 1, 36), (0, 5, 5, -3, 51)]),    // 8  :105-122
    Some([(0, 3, 16, 2, 31), (0, 9, 12, 1, 36), (0, 5, 6, -3, 51)]),    // 9  :124-141
    None,                                                               // 10 (no level)
    None,                                                               // 11 (no level)
    Some([(0, 5, 22, 4, 31), (0, 12, 17, 2, 36), (0, 7, 8, -4, 51)]),   // 12 :143-160
    None,                                                               // 13 (no level)
    None,                                                               // 14 (no level)
    None,                                                               // 15 (no level)
    Some([(0, 6, 30, 5, 31), (0, 16, 24, 2, 36), (0, 8, 10, -5, 51)]),  // 16 :162-179
    None,                                                               // 17 (no level)
    None,                                                               // 18 (no level)
    None,                                                               // 19 (no level)
    None,                                                               // 20 (no level)
    None,                                                               // 21 (no level)
    None,                                                               // 22 (no level)
    None,                                                               // 23 (no level)
    Some([(0, 9, 46, 8, 31), (0, 24, 38, 3, 36), (0, 11, 15, -7, 51)]), // 24 :181-198
];

/// Resolve a `shadow` strength (0..8) to a table level: the largest level
/// `<= min(strength, 6)`. Strength 5 falls back to 4, 7/8 to 6, 0 has none.
/// This is the legacy `shadow` canonicalizer only; state-driven elevation
/// must use [`project_state_level`] instead.
pub fn elevation_level(strength: i64) -> usize {
    let s = strength.clamp(0, 8) as usize;
    if s >= 6 {
        6
    } else if s == 5 {
        4
    } else {
        s
    }
}

/// State-only elevation projector `M(x)`: the largest whitelist level in
/// `{0,1,2,3,4,6}` not greater than `clamp(round_half_up(x), 0, 6)`.
/// Accepts only state-delta values or in-flight interpolated scalars (already
/// round-half-up by `AnimValue::lerp`), never an authored `elevation`, which is
/// whitelist-validated at parse time. Distinct from [`elevation_level`] so the
/// shadow-strength and state-elevation semantics cannot be conflated.
pub fn project_state_level(x: i64) -> usize {
    match x.clamp(0, 6) {
        6 => 6,
        5 => 4,
        r => r as usize,
    }
}

/// Record one material-elevation display command for capture replay.
/// The replay backend expands `level` via [`ELEVATION_TABLE`]; level 0 is never
/// recorded (no shadow).
pub(crate) fn emit_elevation_command(canvas: &mut Canvas, n: &LaidOut, level: i64) {
    let clip = canvas.effective_clip();
    // Elliptical/per-corner nodes carry their radii; uniform nodes keep the
    // scalar `radius` only so the default command stream stays byte-identical.
    let radii = if has_per_corner_radii(n) {
        Some(Canvas::gpu_radii(n.radii))
    } else {
        None
    };
    canvas.emit(Command::EffectElevation {
        rect: n.rect,
        clip,
        radius: shape_radius(n),
        radii,
        level,
        color: n.shadow_color,
    });
}

/// Draw a material elevation shadow for `n` at `level`, far-to-near. Uses
/// deterministic box blur and integer compositing; no floats, no wall clock.
/// The software path runs only outside capture; no global switch remains.
pub(crate) fn draw_elevation(canvas: &mut Canvas, n: &LaidOut, level: usize) {
    let Some(layers) = ELEVATION_TABLE.get(level).copied().flatten() else {
        return;
    };
    let clip = canvas.effective_clip();
    let rad = shape_radius(n);
    let per_corner = has_per_corner_radii(n);
    for (dx, dy, blur, spread, alpha) in layers {
        // Layer box: node rect expanded by spread (negative shrinks), shifted
        // by the layer offset. Clamp radius into the (possibly smaller) box.
        let box_r = Rect {
            x: n.rect.x.saturating_add(dx).saturating_sub(spread),
            y: n.rect.y.saturating_add(dy).saturating_sub(spread),
            w: (n.rect.w.saturating_add(spread.saturating_mul(2))).max(0),
            h: (n.rect.h.saturating_add(spread.saturating_mul(2))).max(0),
        };
        if box_r.w <= 0 || box_r.h <= 0 {
            continue;
        }
        // Flutter `Shadow.convertRadiusToSigma` (engine 371f772,
        // lib/ui/painting.dart:8720-8721):
        // sigma = radius > 0 ? radius * 0.57735 + 0.5 : 0.
        // Integer form: floor(radius * 57735/100000) + 1 for radius > 0.
        let sigma = if blur > 0 {
            (blur * 57735) / 100000 + 1
        } else {
            0
        };
        let pad = sigma.saturating_mul(3).max(1);
        let bounds = clipped_area(canvas.width, canvas.height, clip, box_r, pad);
        if bounds.w <= 0 || bounds.h <= 0 {
            continue;
        }
        let bw = bounds.w as usize;
        let bh = bounds.h as usize;
        let mut mask = vec![0u8; bw * bh];
        let lrad = rad
            .saturating_add(spread.max(0))
            .max(0)
            .min(box_r.w.min(box_r.h) / 2);
        let layer_radii = if per_corner {
            Some(shadow_layer_radii(n.radii, spread, box_r))
        } else {
            None
        };
        for iy in 0..bh {
            let y = bounds.y + iy as i64;
            for ix in 0..bw {
                let x = bounds.x + ix as i64;
                if x < clip.x
                    || y < clip.y
                    || x >= clip.x.saturating_add(clip.w)
                    || y >= clip.y.saturating_add(clip.h)
                {
                    continue;
                }
                if x >= box_r.x
                    && y >= box_r.y
                    && x < box_r.x.saturating_add(box_r.w)
                    && y < box_r.y.saturating_add(box_r.h)
                {
                    let inside = match layer_radii {
                        Some(lr) => crate::geometry::round_rect_contains(
                            x, y, box_r.x, box_r.y, box_r.w, box_r.h, lr,
                        ),
                        None => !Canvas::outside_round(box_r, x, y, lrad),
                    };
                    if inside {
                        mask[iy * bw + ix] = 255;
                    }
                }
            }
        }
        let soft = blur_mask(&mask, bw, bh, sigma);
        for iy in 0..bh {
            let y = bounds.y + iy as i64;
            for ix in 0..bw {
                let x = bounds.x + ix as i64;
                // Skip pixels under the node's own body: the shadow is behind it.
                let under_body = match layer_radii {
                    Some(_) => crate::geometry::round_rect_contains(
                        x, y, n.rect.x, n.rect.y, n.rect.w, n.rect.h, n.radii,
                    ),
                    None => {
                        x >= n.rect.x
                            && y >= n.rect.y
                            && x < n.rect.x.saturating_add(n.rect.w)
                            && y < n.rect.y.saturating_add(n.rect.h)
                            && !Canvas::outside_round(n.rect, x, y, rad)
                    }
                };
                if under_body {
                    continue;
                }
                let cov = soft[iy * bw + ix] as i64;
                let a = ((alpha as i64 * cov + 127) / 255).clamp(0, 255) as u8;
                // Rounded-container clip coverage applies to shadows too.
                // Without it the rounded corner's cut-out keeps full shadow
                // alpha while clipped content is erased: a dark fringe.
                let clip_cov = canvas.clip_coverage(x, y);
                let a = ((a as i64 * clip_cov as i64 + 127) / 255).clamp(0, 255) as u8;
                let shadow_color = n.shadow_color;
                composite(canvas, x, y, shadow_color, a);
            }
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

    /// Shadow-only: `draw_elevation` rasterizes no body, so any pixel delta
    /// between elliptical and circular radii comes from the shadow silhouette.
    /// Mutating the per-corner branch to scalar makes both renders identical.
    #[test]
    fn shadow_silhouette_follows_per_corner_radii() {
        fn shadow(elliptical: bool) -> Vec<u8> {
            let radii = if elliptical {
                r#","cornerRadii":{"topLeft":[40,10],"topRight":[40,10],"bottomLeft":[40,10],"bottomRight":[40,10]}"#
            } else {
                ""
            };
            let text = format!(
                r##"{{"ir":"0.1.0","app":"S","state":[],"tree":{{"id":0,"kind":"Column","props":{{"padding":20,"color":"#FFFFFF","border":"#FFFFFF"}},"events":{{}},"children":[{{"id":1,"kind":"Container","props":{{"width":80,"height":60,"color":"#FFFFFF","border":"#FFFFFF","radius":30{radii},"elevation":3}},"events":{{}},"children":[]}}]}},"theme":{{"mode":"light","accent":"#097FEB","radius":4}}}}"##
            );
            let ir = crate::parse_ir(&text).unwrap();
            let laid = crate::layout::layout(&ir, &crate::StateView::from_ir(&ir), 160, 140);
            let node = laid.iter().find(|n| n.id == 1).unwrap().clone();
            let mut canvas = Canvas::new(160, 140, Rgba::rgb(255, 255, 255));
            draw_elevation(&mut canvas, &node, 3);
            canvas.pixels
        }
        let ell = shadow(true);
        let cir = shadow(false);
        let changed = ell
            .iter()
            .zip(cir.iter())
            .step_by(4)
            .filter(|(a, b)| a != b)
            .count();
        assert!(
            changed > 20,
            "elliptical elevation shadow must differ from circular, got {changed} px"
        );
    }
}
