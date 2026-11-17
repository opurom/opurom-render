//! Deterministic RGBA8 software raster: rounded fills/borders, canonical style
//! effects, bitmap text and framebuffer effects. Every pixel write is integer
//! arithmetic; the frame is opaque.

use crate::display::Command;
use crate::geometry::{round_rect_contains, CornerRadii, RoundingRadii};
use crate::icons;
use crate::image;
use crate::interaction::{self, NodeState, Variant, FOCUS_RING};
use crate::layout::Control;
use crate::{font, Align, Kind, LaidOut, Rect, Rgba};

/// Fixed-point unit: 1024 == fully opaque.
const UNIT: i64 = 1024;

/// Fixed-point unit for the opt-in 16-bit gradient intermediate.
const GRADIENT_UNIT: i64 = 1 << 16;

/// Bayer 2x2 base, indexed by [x bit][y bit].
const BAYER2: [[i64; 2]; 2] = [[0, 3], [2, 1]];

/// 256x256 ordered dither threshold in 0..=65535, built from eight bit levels
/// of the classic Bayer recursion. Pure integer, fixed for every frame.
fn bayer256(x: i64, y: i64) -> i64 {
    let mut v = 0i64;
    for bit in 0..8 {
        let bx = ((x >> bit) & 1) as usize;
        let by = ((y >> bit) & 1) as usize;
        v += BAYER2[bx][by] << (2 * (7 - bit));
    }
    v
}

/// Quantize 16-bit linear channels to RGBA8 with deterministic ordered
/// dithering: floor(v/65536*255 + (threshold+0.5)/65536), all integer.
fn dither_rgb8(v: [i64; 3], x: i64, y: i64) -> [u8; 3] {
    let threshold = bayer256(x, y);
    let bias = (2 * threshold + 1) * (GRADIENT_UNIT / 2);
    let denom = GRADIENT_UNIT * 65536;
    let mut out = [0u8; 3];
    for k in 0..3 {
        out[k] = ((v[k] * 255 * 65536 + bias) / denom).clamp(0, 255) as u8;
    }
    out
}

// Data primitives use theme-independent, deterministic ink colours. They are
// only ever drawn for the eight new kinds, so they cannot affect 0.1.0 goldens.
const INK: Rgba = Rgba::rgb(0x1b, 0x1f, 0x27);
const LINE: Rgba = Rgba::rgb(0x90, 0x98, 0xa8);
const LIGHT: Rgba = Rgba::rgb(0xff, 0xff, 0xff);
const LAND: Rgba = Rgba::rgb(0xdf, 0xe6, 0xd8);
const BUBBLE: Rgba = Rgba::rgb(0x24, 0x2a, 0x38);

/// Rounded boundary of a surface clip, carried from the container to its
/// children. This is internal seam consistency only, NOT Flutter parity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipShape {
    pub rect: Rect,
    pub radii: RoundingRadii,
}

/// Clip shape is raster presentation state, not layout identity; neutral
/// equality keeps settled layout comparisons byte-identical.
#[derive(Clone, Debug, Default, Eq)]
pub struct ClipShapeSlot(pub Option<ClipShape>);

impl PartialEq for ClipShapeSlot {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl ClipShapeSlot {
    pub fn as_ref(&self) -> Option<&ClipShape> {
        self.0.as_ref()
    }
}

pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    /// Optional clip rectangle (`Scroll` viewport).
    pub clip: Option<Rect>,
    /// Rounded container boundary for opt-in content clip coverage.
    clip_shape: Option<ClipShape>,
    /// Opt-in 4x sub-pixel coverage on geometric edges. Off by default so the
    /// pinned golden frames stay byte-identical; capture mode ignores it so the
    /// display-list/GPU command stream is never altered.
    edge_aa: bool,
    /// Gradient intermediate precision: 10 is the pinned default, 16 switches
    /// to the dithered RGBA8 path. Capture ignores the switch.
    gradient_bits: u8,
    /// Opt-in display-layer split: skip bitmap text so the host can overlay
    /// system-font spans. Off by default so every golden stays byte-identical.
    suppress_text: bool,
    capture: bool,
    capture_unsupported: Option<String>,
    commands: Vec<Command>,
}

/// Sub-samples per axis for opt-in straight-line coverage.
const EDGE_AA: i64 = 4;
/// Finer sub-samples for curved edges (rounded boxes, discs, rings). Curvature
/// needs more than the 16 straight-line samples to avoid visible stair-steps;
/// 8x8 gives 65 coverage levels. Independent of EDGE_AA so line raster stays
/// byte-stable. Kept only as the small-radius fast path; larger and irregular
/// corners use the analytic coverage in [`crate::aaa`].
const CORNER_AA: i64 = 8;
/// Radius above which the analytic path replaces corner supersampling. Small
/// uniform corners stay on the cheap per-pixel test.
const AAA_MIN_RADIUS: i64 = 4;

impl Canvas {
    pub fn new(width: u32, height: u32, bg: Rgba) -> Canvas {
        let w = width as usize;
        let h = height as usize;
        let mut pixels = vec![0u8; w * h * 4];
        if w > 0 && h > 0 {
            let row: Vec<u8> = [bg.r, bg.g, bg.b, 255].repeat(w);
            for y in 0..h {
                pixels[y * w * 4..(y + 1) * w * 4].copy_from_slice(&row);
            }
        }
        Canvas {
            width,
            height,
            pixels,
            clip: None,
            clip_shape: None,
            edge_aa: false,
            gradient_bits: 10,
            suppress_text: false,
            capture: false,
            capture_unsupported: None,
            commands: Vec::new(),
        }
    }

    /// Enable opt-in geometric edge coverage. Capture mode keeps its command
    /// stream byte-identical, so this is a no-op while recording display lists.
    pub fn set_edge_aa(&mut self, on: bool) {
        self.edge_aa = on && !self.capture;
    }

    /// Select the 16-bit dithered gradient path. Capture ignores the switch so
    /// the recorded command stream stays byte-identical to the default path.
    pub fn set_gradient_bits(&mut self, bits: u8) {
        if !self.capture && bits == 16 {
            self.gradient_bits = 16;
        }
    }

    /// Suppress bitmap text rasterization so a display host can paint system
    /// font spans instead. Every other primitive is unaffected. Default false.
    pub fn set_suppress_text(&mut self, on: bool) {
        self.suppress_text = on;
    }

    /// Apply the root `renderQuality` opt-in signals. Capture ignores every
    /// signal so the display-list command stream stays byte-identical; the
    /// software-only paths change pixels but never the recorded commands.
    pub fn set_render_quality(&mut self, rq: Option<&crate::ir::RenderQuality>) {
        if self.capture {
            return;
        }
        if let Some(rq) = rq {
            if rq.msaa == Some(4) {
                self.edge_aa = true;
            }
            if rq.gradient_bits == Some(16) {
                self.gradient_bits = 16;
            }
        }
    }

    /// 8x8 integer coverage of a rounded-box edge at one pixel. Sub-samples are
    /// taken in an 8x-scaled space so the existing quarter-circle predicate is
    /// reused without floats. Interior pixels return 255 and keep the fast path.
    fn round_coverage(r: Rect, x: i64, y: i64, rad: i64) -> u8 {
        if rad <= 0 {
            return 255;
        }
        let big = Rect {
            x: r.x * CORNER_AA,
            y: r.y * CORNER_AA,
            w: r.w * CORNER_AA,
            h: r.h * CORNER_AA,
        };
        let bx = x * CORNER_AA;
        let by = y * CORNER_AA;
        let mut hits = 0i64;
        for sy in 0..CORNER_AA {
            for sx in 0..CORNER_AA {
                if !Self::outside_round(big, bx + sx, by + sy, rad * CORNER_AA) {
                    hits += 1;
                }
            }
        }
        let n = CORNER_AA * CORNER_AA;
        ((255 * hits + n / 2) / n) as u8
    }

    /// 8x8 integer coverage of a disc (`inner` is the squared inner radius for
    /// rings; pass 0 for a filled circle) at one pixel.
    fn circle_coverage(cx: i64, cy: i64, outer: i64, inner: i64, x: i64, y: i64) -> u8 {
        let mut hits = 0i64;
        for sy in 0..CORNER_AA {
            for sx in 0..CORNER_AA {
                let dx = (x * CORNER_AA + sx) - cx * CORNER_AA;
                let dy = (y * CORNER_AA + sy) - cy * CORNER_AA;
                let d = dx * dx + dy * dy;
                if d <= outer && d > inner {
                    hits += 1;
                }
            }
        }
        let n = CORNER_AA * CORNER_AA;
        ((255 * hits + n / 2) / n) as u8
    }

    /// Blend a colour whose alpha encodes edge coverage; 255 keeps the exact
    /// original opaque write.
    fn set_covered(&mut self, x: i64, y: i64, c: Rgba, cov: u8) {
        if cov == 0 {
            return;
        }
        if cov == 255 {
            self.set_quiet(x, y, c);
        } else {
            let mut soft = c;
            soft.a = ((c.a as i64 * cov as i64) / 255) as u8;
            self.set_quiet(x, y, soft);
        }
    }

    /// Rasterize a convex path with analytic per-row coverage. `x_from..x_to`
    /// and `y_from..y_to` bound the box in absolute canvas pixels; the row mask
    /// is indexed from column 0 and reused across rows. Only opt-in callers
    /// reach this, so capture's command stream is never touched.
    fn fill_aaa_path(
        &mut self,
        path: &[crate::aaa::Segment],
        x_from: i64,
        x_to: i64,
        y_from: i64,
        y_to: i64,
        c: Rgba,
    ) {
        self.fill_aaa_path_ex(path, x_from, x_to, y_from, y_to, c, y_from, y_from);
    }

    /// As [`Canvas::fill_aaa_path`], but rows in `[inner_y0, inner_y1)` are
    /// known to span the full box width, so they skip the trapezoid scan and
    /// fill directly. Only opt-in callers with that guarantee use this.
    #[allow(clippy::too_many_arguments)]
    fn fill_aaa_path_ex(
        &mut self,
        path: &[crate::aaa::Segment],
        x_from: i64,
        x_to: i64,
        y_from: i64,
        y_to: i64,
        c: Rgba,
        inner_y0: i64,
        inner_y1: i64,
    ) {
        let mut row = vec![0i64; self.width as usize];
        for y in y_from..y_to {
            if y >= inner_y0 && y < inner_y1 {
                self.fill_rect_raw(
                    Rect {
                        x: x_from,
                        y,
                        w: x_to - x_from,
                        h: 1,
                    },
                    c,
                );
                continue;
            }
            for a in row.iter_mut() {
                *a = 0;
            }
            crate::aaa::rasterize_convex_row(path, &mut row, 0, y);
            let w = self.width as usize;
            for x in x_from..x_to {
                let cov = row[x as usize].clamp(0, 255) as u8;
                if cov == 255 {
                    // Bounds and clip already resolved by x_from/x_to and the
                    // caller's y_from/y_to; write the opaque pixel directly.
                    let i = (y as usize * w + x as usize) * 4;
                    self.pixels[i] = c.r;
                    self.pixels[i + 1] = c.g;
                    self.pixels[i + 2] = c.b;
                    self.pixels[i + 3] = 255;
                } else {
                    self.set_covered(x, y, c, cov);
                }
            }
        }
    }

    /// Capture the exact high-level draw commands for the display list.
    pub fn capturing(width: u32, height: u32, bg: Rgba) -> Canvas {
        let mut canvas = Canvas::new(width, height, bg);
        canvas.capture = true;
        canvas
    }

    /// Wrap an existing RGBA buffer so an incremental render can start from the
    /// previous frame and only touch changed regions. The caller owns the
    /// buffer; the canvas writes in place.
    pub fn from_pixels(width: u32, height: u32, pixels: Vec<u8>) -> Canvas {
        Canvas {
            width,
            height,
            pixels,
            clip: None,
            clip_shape: None,
            edge_aa: false,
            gradient_bits: 10,
            suppress_text: false,
            capture: false,
            capture_unsupported: None,
            commands: Vec::new(),
        }
    }

    pub fn into_commands(self) -> Vec<Command> {
        self.commands
    }

    pub(crate) fn is_capturing(&self) -> bool {
        self.capture
    }

    pub(crate) fn reject_capture(&mut self, reason: &str) {
        if self.capture_unsupported.is_none() {
            self.capture_unsupported = Some(reason.to_string());
        }
    }

    pub(crate) fn take_unsupported(&mut self) -> Option<String> {
        self.capture_unsupported.take()
    }

    pub(crate) fn effective_clip(&self) -> Rect {
        crate::effects::effective_clip(self.width, self.height, self.clip)
    }

    fn visible_rect(&self, r: Rect, c: Rgba) -> bool {
        if c.a == 0 || r.w <= 0 || r.h <= 0 {
            return false;
        }
        let clip = self.effective_clip();
        let x0 = r.x.max(clip.x).max(0);
        let y0 = r.y.max(clip.y).max(0);
        let x1 =
            r.x.saturating_add(r.w)
                .min(clip.x.saturating_add(clip.w))
                .min(self.width as i64);
        let y1 =
            r.y.saturating_add(r.h)
                .min(clip.y.saturating_add(clip.h))
                .min(self.height as i64);
        x1 > x0 && y1 > y0
    }

    pub(crate) fn emit(&mut self, command: Command) {
        if self.capture {
            self.commands.push(command);
        }
    }

    fn set_quiet(&mut self, x: i64, y: i64, c: Rgba) -> bool {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return false;
        }
        if let Some(clip) = self.clip {
            if x < clip.x || y < clip.y || x >= clip.x + clip.w || y >= clip.y + clip.h {
                return false;
            }
        }
        let clip_cov = if self.edge_aa {
            self.clip_shape
                .map(|s| Self::radii_coverage(s.rect, x, y, s.radii))
                .unwrap_or(255)
        } else {
            255
        };
        if clip_cov == 0 {
            return false;
        }
        let c = if clip_cov == 255 {
            c
        } else {
            let mut soft = c;
            soft.a = ((c.a as i64 * clip_cov as i64) / 255) as u8;
            if soft.a == 0 {
                return false;
            }
            soft
        };
        let i = (y as usize * self.width as usize + x as usize) * 4;
        if c.a == 0 {
            return false;
        }
        if c.a == 255 {
            self.pixels[i] = c.r;
            self.pixels[i + 1] = c.g;
            self.pixels[i + 2] = c.b;
            self.pixels[i + 3] = 255;
            return true;
        }
        let a = c.a as u32;
        let inv = 255 - a;
        let src = [c.r, c.g, c.b];
        for k in 0..3 {
            let dst = self.pixels[i + k] as u32;
            self.pixels[i + k] = ((src[k] as u32 * a + dst * inv) / 255) as u8;
        }
        self.pixels[i + 3] = 255;
        true
    }

    pub fn set(&mut self, x: i64, y: i64, c: Rgba) {
        let wrote = self.set_quiet(x, y, c);
        if wrote && self.capture {
            let clip = self.effective_clip();
            self.emit(Command::Solid {
                rect: Rect { x, y, w: 1, h: 1 },
                clip,
                color: c,
            });
        }
    }

    pub fn fill_rect(&mut self, r: Rect, c: Rgba) {
        if self.capture && self.visible_rect(r, c) {
            let clip = self.effective_clip();
            self.emit(Command::Solid {
                rect: r,
                clip,
                color: c,
            });
        }
        self.fill_rect_raw(r, c);
    }

    fn fill_rect_raw(&mut self, r: Rect, c: Rgba) {
        // Iterate only the intersection with the clip. `set_quiet` already
        // rejects out-of-clip pixels; shrinking the loop bounds skips those
        // rejects entirely, which matters when a large background box is
        // redrawn inside a tiny dirty region. Written pixels are identical.
        let clip = self.effective_clip();
        let x0 = r.x.max(clip.x).max(0);
        let y0 = r.y.max(clip.y).max(0);
        let x1 = (r.x + r.w).min(clip.x + clip.w).min(self.width as i64);
        let y1 = (r.y + r.h).min(clip.y + clip.h).min(self.height as i64);
        // Opaque fill: write one repeated pixel pattern per row instead of a
        // per-pixel bounds/clip/alpha test. The written bytes are identical.
        // Both dimensions must be non-empty: the old per-pixel loop wrote
        // nothing when y1 <= y0, so an empty (e.g. stale-clip) range must
        // stay a no-op rather than index past the framebuffer.
        if c.a == 255 && x1 > x0 && y1 > y0 {
            let w = self.width as usize;
            let row_len = (x1 - x0) as usize * 4;
            let first = (y0 as usize * w + x0 as usize) * 4;
            let px = [c.r, c.g, c.b, 255u8];
            for ch in self.pixels[first..first + row_len].chunks_exact_mut(4) {
                ch.copy_from_slice(&px);
            }
            for y in y0 + 1..y1 {
                let dst = (y as usize * w + x0 as usize) * 4;
                self.pixels.copy_within(first..first + row_len, dst);
            }
            return;
        }
        for y in y0..y1 {
            for x in x0..x1 {
                self.set_quiet(x, y, c);
            }
        }
    }

    /// Integer corner test: `true` when the pixel is outside the rounded box.
    pub(crate) fn outside_round(r: Rect, x: i64, y: i64, rad: i64) -> bool {
        if rad <= 0 {
            return false;
        }
        let cr = rad - 1;
        let limit = cr as i128 * cr as i128;
        let dx = x.saturating_sub(r.x);
        let right = r.w.saturating_sub(rad);
        let cx: i128 = if dx < rad {
            cr as i128 - dx as i128
        } else if dx >= right {
            dx as i128 - right as i128
        } else {
            -1
        };
        let dy = y.saturating_sub(r.y);
        let bottom = r.h.saturating_sub(rad);
        let cy: i128 = if dy < rad {
            cr as i128 - dy as i128
        } else if dy >= bottom {
            dy as i128 - bottom as i128
        } else {
            -1
        };
        cx >= 0 && cy >= 0 && cx * cx + cy * cy > limit
    }

    /// True when all four corners are the same circular radius; such a shape
    /// keeps the historical scalar path byte-for-byte.
    pub(crate) fn uniform_radius(radii: RoundingRadii) -> Option<i64> {
        let c = radii.top_left;
        if c.x == c.y && radii.top_right == c && radii.bottom_left == c && radii.bottom_right == c {
            Some(c.x)
        } else {
            None
        }
    }

    fn radii_inset(radii: RoundingRadii, amount: i64) -> RoundingRadii {
        let shrink = |c: CornerRadii| CornerRadii {
            x: (c.x - amount).max(0),
            y: (c.y - amount).max(0),
        };
        RoundingRadii {
            top_left: shrink(radii.top_left),
            top_right: shrink(radii.top_right),
            bottom_left: shrink(radii.bottom_left),
            bottom_right: shrink(radii.bottom_right),
        }
    }

    fn radii_coverage(r: Rect, x: i64, y: i64, radii: RoundingRadii) -> u8 {
        let scale = |c: CornerRadii| CornerRadii {
            x: c.x * CORNER_AA,
            y: c.y * CORNER_AA,
        };
        let big = Rect {
            x: r.x * CORNER_AA,
            y: r.y * CORNER_AA,
            w: r.w * CORNER_AA,
            h: r.h * CORNER_AA,
        };
        let scaled = RoundingRadii {
            top_left: scale(radii.top_left),
            top_right: scale(radii.top_right),
            bottom_left: scale(radii.bottom_left),
            bottom_right: scale(radii.bottom_right),
        };
        let mut hits = 0i64;
        for sy in 0..CORNER_AA {
            for sx in 0..CORNER_AA {
                if round_rect_contains(
                    x * CORNER_AA + sx,
                    y * CORNER_AA + sy,
                    big.x,
                    big.y,
                    big.w,
                    big.h,
                    scaled,
                ) {
                    hits += 1;
                }
            }
        }
        let n = CORNER_AA * CORNER_AA;
        ((255 * hits + n / 2) / n) as u8
    }

    /// Fill with independent per-corner radii. Uniform radii delegate to the
    /// scalar path so every existing frame stays byte-identical.
    pub fn fill_rounded_radii(&mut self, r: Rect, c: Rgba, radii: RoundingRadii) {
        if let Some(rad) = Self::uniform_radius(radii) {
            self.fill_rounded(r, c, rad);
            return;
        }
        if self.capture {
            // The display list carries four independent radii in the GPU wire
            // order, so record the shape instead of dropping it.
            let clip = self.effective_clip();
            self.emit(Command::RoundRectPerCorner {
                rect: r,
                clip,
                radii: Self::gpu_radii(radii),
                color: c,
            });
            return;
        }
        if r.w <= 0 || r.h <= 0 {
            return;
        }
        let clip = self.effective_clip();
        let x_from = r.x.max(clip.x).max(0);
        let x_to = (r.x + r.w).min(clip.x + clip.w).min(self.width as i64);
        let y_from = r.y.max(clip.y).max(0);
        let y_to = (r.y + r.h).min(clip.y + clip.h).min(self.height as i64);
        let max_r = radii
            .top_left
            .x
            .max(radii.top_left.y)
            .max(radii.top_right.x)
            .max(radii.top_right.y)
            .max(radii.bottom_left.x)
            .max(radii.bottom_left.y)
            .max(radii.bottom_right.x)
            .max(radii.bottom_right.y);
        if self.edge_aa && max_r > AAA_MIN_RADIUS {
            let path = crate::aaa::round_rect_path_radii(r.x, r.y, r.w, r.h, radii);
            self.fill_aaa_path(&path, x_from, x_to, y_from, y_to, c);
            return;
        }
        for y in y_from..y_to {
            for x in x_from..x_to {
                let cov = if self.edge_aa {
                    Self::radii_coverage(r, x, y, radii)
                } else if round_rect_contains(x, y, r.x, r.y, r.w, r.h, radii) {
                    255
                } else {
                    0
                };
                if self.edge_aa {
                    self.set_covered(x, y, c, cov);
                } else if cov != 0 {
                    self.set_quiet(x, y, c);
                }
            }
        }
    }

    /// Filled rounded rectangle using an integer quarter-circle corner test.
    pub fn fill_rounded(&mut self, r: Rect, c: Rgba, radius: i64) {
        if r.w <= 0 || r.h <= 0 {
            return;
        }
        let rad = radius.max(0).min(r.w.min(r.h) / 2);
        let x1 = (r.x + r.w).min(self.width as i64);
        let y1 = (r.y + r.h).min(self.height as i64);
        if rad <= 0 {
            self.fill_rect(r, c);
            return;
        }
        if self.capture && self.visible_rect(r, c) {
            let clip = self.effective_clip();
            self.emit(Command::Rounded {
                rect: r,
                clip,
                radius: rad,
                color: c,
            });
        }
        // Shrink the raster loop to the clip∩rect intersection. set_quiet /
        // set_covered already reject out-of-clip pixels, so written pixels are
        // unchanged; this avoids scanning a huge box for a small dirty region.
        let clip = self.effective_clip();
        let x_from = r.x.max(clip.x).max(0);
        let x_to = x1.min(clip.x + clip.w);
        let y_from = r.y.max(clip.y).max(0);
        let y_to = y1.min(clip.y + clip.h);
        if self.edge_aa && rad > AAA_MIN_RADIUS {
            let path = crate::aaa::round_rect_path(r.x, r.y, r.w, r.h, rad);
            self.fill_aaa_path_ex(
                &path,
                x_from,
                x_to,
                y_from,
                y_to,
                c,
                r.y + rad,
                r.y + r.h - rad,
            );
            return;
        }
        for y in y_from..y_to {
            for x in x_from..x_to {
                if self.edge_aa {
                    let cov = Self::round_coverage(r, x, y, rad);
                    self.set_covered(x, y, c, cov);
                } else if !Self::outside_round(r, x, y, rad) {
                    self.set_quiet(x, y, c);
                }
            }
        }
    }

    /// Two-stop gradient inside a rounded rectangle, sweeping vertically by
    /// default or along the horizontal axis when asked. The per-pixel maths is
    /// identical on both axes, so the recorded command reproduces the frame.
    /// Corner radii in the GPU wire order (top-left, top-right, bottom-left,
    /// bottom-right), each component clamped to [`GPU_RADIUS_LIMIT`]. The
    /// software raster clamps to a wider limit, so both agree for every radius
    /// a layout can produce.
    pub(crate) fn gpu_radii(radii: RoundingRadii) -> [[i64; 2]; 4] {
        let clamp = |c: CornerRadii| {
            [
                c.x.clamp(0, crate::display::GPU_RADIUS_LIMIT),
                c.y.clamp(0, crate::display::GPU_RADIUS_LIMIT),
            ]
        };
        [
            clamp(radii.top_left),
            clamp(radii.top_right),
            clamp(radii.bottom_left),
            clamp(radii.bottom_right),
        ]
    }

    /// Border-only rounded outline with independent per-corner radii.
    pub fn stroke_rounded_radii(&mut self, r: Rect, width: i64, c: Rgba, radii: RoundingRadii) {
        if r.w <= 0 || r.h <= 0 {
            return;
        }
        let w = width.max(1).min(r.w.min(r.h) / 2);
        if let Some(rad) = Self::uniform_radius(radii) {
            draw_outline(self, r, w, c, false);
            let _ = rad;
            return;
        }
        if self.capture {
            self.reject_capture("per-corner radii are not yet supported by the display list");
            return;
        }
        let inner = inset(r, w);
        let inner_radii = Self::radii_inset(radii, w).scaled(inner.w, inner.h);
        let clip = self.effective_clip();
        let x0 = r.x.max(clip.x).max(0);
        let y0 = r.y.max(clip.y).max(0);
        let x1 = (r.x + r.w).min(clip.x + clip.w).min(self.width as i64);
        let y1 = (r.y + r.h).min(clip.y + clip.h).min(self.height as i64);
        for y in y0..y1 {
            for x in x0..x1 {
                if !round_rect_contains(x, y, r.x, r.y, r.w, r.h, radii) {
                    continue;
                }
                if inner.w > 0
                    && inner.h > 0
                    && round_rect_contains(x, y, inner.x, inner.y, inner.w, inner.h, inner_radii)
                {
                    continue;
                }
                self.set_quiet(x, y, c);
            }
        }
    }

    /// Gradient fill with independent per-corner radii.
    pub fn fill_rounded_gradient_radii(
        &mut self,
        r: Rect,
        top: Rgba,
        bottom: Rgba,
        radii: RoundingRadii,
        alpha: i64,
        horizontal: bool,
    ) {
        if let Some(rad) = Self::uniform_radius(radii) {
            self.fill_rounded_gradient(r, top, bottom, rad, alpha, horizontal);
            return;
        }
        if self.capture {
            self.reject_capture(
                "per-corner radii with a gradient are not yet supported by the display list",
            );
            return;
        }
        let x1 = (r.x + r.w).min(self.width as i64);
        let y1 = (r.y + r.h).min(self.height as i64);
        let alpha_u8 = ((alpha.clamp(0, UNIT) * 255) / UNIT) as u8;
        let last = if horizontal { r.w - 1 } else { r.h - 1 };
        for y in r.y..y1 {
            for x in r.x..x1 {
                let cov = if self.edge_aa {
                    Self::radii_coverage(r, x, y, radii)
                } else if round_rect_contains(x, y, r.x, r.y, r.w, r.h, radii) {
                    255
                } else {
                    0
                };
                if cov == 0 {
                    continue;
                }
                let t = if horizontal {
                    (x - r.x) as i128 * UNIT as i128 / last.max(1) as i128
                } else {
                    (y - r.y) as i128 * UNIT as i128 / last.max(1) as i128
                };
                let mut c = crate::layout::lerp_color(top, bottom, t as i64);
                c.a = ((c.a as i64 * alpha_u8 as i64 + 127) / 255) as u8;
                if self.edge_aa {
                    self.set_covered(x, y, c, cov);
                } else {
                    self.set_quiet(x, y, c);
                }
            }
        }
    }

    pub fn fill_rounded_gradient(
        &mut self,
        r: Rect,
        top: Rgba,
        bottom: Rgba,
        radius: i64,
        alpha: i64,
        horizontal: bool,
    ) {
        if r.w <= 0 || r.h <= 0 {
            return;
        }
        let rad = radius.max(0).min(r.w.min(r.h) / 2);
        let x1 = (r.x + r.w).min(self.width as i64);
        let y1 = (r.y + r.h).min(self.height as i64);
        let last = if horizontal { r.w - 1 } else { r.h - 1 };
        let alpha_u8 = ((alpha.clamp(0, UNIT) * 255) / UNIT) as u8;
        if self.capture && alpha_u8 > 0 && self.visible_rect(r, top) {
            let clip = self.effective_clip();
            let command = if horizontal {
                Command::GradientHorizontal {
                    rect: r,
                    clip,
                    radius: rad,
                    top,
                    bottom,
                    alpha: alpha_u8,
                }
            } else {
                Command::Gradient {
                    rect: r,
                    clip,
                    radius: rad,
                    top,
                    bottom,
                    alpha: alpha_u8,
                }
            };
            self.emit(command);
        }
        let clip = self.effective_clip();
        let x_from = r.x.max(clip.x).max(0);
        let x_to = x1.min(clip.x + clip.w);
        let y_from = r.y.max(clip.y).max(0);
        let y_to = y1.min(clip.y + clip.h);
        for y in y_from..y_to {
            for x in x_from..x_to {
                let local = if horizontal { x - r.x } else { y - r.y };
                let c = if self.gradient_bits == 16 {
                    let t = if last <= 0 {
                        GRADIENT_UNIT
                    } else {
                        (local * GRADIENT_UNIT) / last
                    };
                    let mix = |a: u8, b: u8| -> i64 {
                        let lo = a as i64 * 257;
                        let hi = b as i64 * 257;
                        lo + ((hi - lo) * t) / GRADIENT_UNIT
                    };
                    let rgb = dither_rgb8(
                        [
                            mix(top.r, bottom.r),
                            mix(top.g, bottom.g),
                            mix(top.b, bottom.b),
                        ],
                        x,
                        y,
                    );
                    Rgba {
                        r: rgb[0],
                        g: rgb[1],
                        b: rgb[2],
                        a: 255,
                    }
                } else {
                    let t = if last <= 0 {
                        UNIT
                    } else {
                        (local * UNIT) / last
                    };
                    crate::layout::lerp_color(top, bottom, t)
                };
                let c = with_alpha(c, alpha);
                if self.edge_aa {
                    let cov = Self::round_coverage(r, x, y, rad);
                    self.set_covered(x, y, c, cov);
                } else if !Self::outside_round(r, x, y, rad) {
                    self.set_quiet(x, y, c);
                }
            }
        }
    }

    /// Scatter the node's particle field over `r` and record one `particles`
    /// command. Dots are painted in index order and clipped to
    /// `rect ∩ clip ∩ viewport`; the display-list command evaluates the same
    /// centres, circle test and blend order per pixel, so the recorded stream
    /// reproduces the framebuffer byte for byte.
    pub fn fill_particles(&mut self, r: Rect, params: crate::particles::Particles, color: Rgba) {
        if r.w <= 0 || r.h <= 0 || params.count <= 0 || params.radius < 0 {
            return;
        }
        let color = Rgba {
            a: crate::particles::alpha_byte(params.alpha),
            ..color
        };
        if color.a == 0 {
            return;
        }
        let rad = params.radius.min(crate::particles::PARTICLE_MAX_RADIUS);
        let clip = self.effective_clip();
        if self.capture && self.visible_rect(r, color) {
            self.emit(Command::Particles {
                rect: r,
                clip,
                count: params.count,
                seed: params.seed,
                radius: rad,
                color,
            });
        }
        let x0 = r.x.max(clip.x).max(0);
        let y0 = r.y.max(clip.y).max(0);
        let x1 = (r.x + r.w).min(clip.x + clip.w).min(self.width as i64);
        let y1 = (r.y + r.h).min(clip.y + clip.h).min(self.height as i64);
        for i in 0..params.count as u32 {
            let (cx, cy) = crate::particles::dot_center(r, params.seed, i);
            for y in (cy - rad).max(y0)..=(cy + rad).min(y1 - 1) {
                for x in (cx - rad).max(x0)..=(cx + rad).min(x1 - 1) {
                    let dx = x - cx;
                    let dy = y - cy;
                    if dx * dx + dy * dy <= rad * rad {
                        self.set_quiet(x, y, color);
                    }
                }
            }
        }
    }

    /// Filled circle (integer distance test).
    pub fn fill_circle(&mut self, r: Rect, c: Rgba) {
        if r.w <= 0 || r.h <= 0 {
            return;
        }
        let rad = r.w.min(r.h) / 2;
        let cx = r.x + r.w / 2;
        let cy = r.y + r.h / 2;
        let limit = rad * rad;
        let x1 = (r.x + r.w).min(self.width as i64);
        let y1 = (r.y + r.h).min(self.height as i64);
        let clip = self.effective_clip();
        let y_from = r.y.max(clip.y).max(0);
        let y_to = y1.min(clip.y + clip.h);
        let x_from = r.x.max(clip.x).max(0);
        let x_to = x1.min(clip.x + clip.w);
        if self.edge_aa && rad > AAA_MIN_RADIUS {
            let path = crate::aaa::circle_path(cx, cy, rad);
            self.fill_aaa_path(&path, x_from, x_to, y_from, y_to, c);
            return;
        }
        for y in y_from..y_to {
            for x in x_from..x_to {
                if self.edge_aa {
                    let cov = Self::circle_coverage(cx, cy, limit * CORNER_AA * CORNER_AA, 0, x, y);
                    self.set_covered(x, y, c, cov);
                } else {
                    let dx = x - cx;
                    let dy = y - cy;
                    if dx * dx + dy * dy <= limit {
                        self.set(x, y, c);
                    }
                }
            }
        }
    }

    /// Circle outline (integer ring test).
    pub fn stroke_circle(&mut self, r: Rect, c: Rgba, width: i64) {
        if r.w <= 0 || r.h <= 0 {
            return;
        }
        let rad = r.w.min(r.h) / 2;
        let cx = r.x + r.w / 2;
        let cy = r.y + r.h / 2;
        let outer = rad * rad;
        let inner = (rad - width.max(1)).max(0);
        let inner = inner * inner;
        let x1 = (r.x + r.w).min(self.width as i64);
        let y1 = (r.y + r.h).min(self.height as i64);
        let clip = self.effective_clip();
        let y_from = r.y.max(clip.y).max(0);
        let y_to = y1.min(clip.y + clip.h);
        let x_from = r.x.max(clip.x).max(0);
        let x_to = x1.min(clip.x + clip.w);
        for y in y_from..y_to {
            for x in x_from..x_to {
                if self.edge_aa {
                    let s = CORNER_AA * CORNER_AA;
                    let cov = Self::circle_coverage(cx, cy, outer * s, inner * s, x, y);
                    self.set_covered(x, y, c, cov);
                } else {
                    let dx = x - cx;
                    let dy = y - cy;
                    let d = dx * dx + dy * dy;
                    if d <= outer && d > inner {
                        self.set(x, y, c);
                    }
                }
            }
        }
    }

    /// Opt-in ripple disc at `(cx,cy)` with the given radius and color. Delegates
    /// to `fill_circle`, so the viewport/parent clip and edge coverage apply
    /// unchanged; a zero radius or fully transparent color draws nothing.
    pub fn draw_ripple(&mut self, cx: i64, cy: i64, radius: i64, c: Rgba) {
        if radius <= 0 || c.a == 0 {
            return;
        }
        let r = Rect {
            x: cx - radius,
            y: cy - radius,
            w: radius.saturating_mul(2),
            h: radius.saturating_mul(2),
        };
        self.fill_circle(r, c);
    }

    /// Draw the press ripple for one node at `elapsed_ms`. The ripple is clipped
    /// to the node's own clip intersected with the current canvas clip, so it
    /// never spills past a parent viewport. `touch` is relative to the node; a
    /// `None` touch centres the circle in the node rect. Integer fixed-point
    /// only, driven entirely by the caller clock.
    pub fn draw_node_ripple(
        &mut self,
        n: &LaidOut,
        press: &interaction::PressPhase,
        elapsed_ms: i64,
        tint: Rgba,
    ) {
        let max_r = interaction::ripple_target_radius(n.rect.w, n.rect.h);
        let Some((radius, alpha)) =
            interaction::ripple_frame(press, elapsed_ms, max_r, interaction::RIPPLE_PEAK_ALPHA)
        else {
            return;
        };
        let color = with_alpha(tint, alpha);
        let (cx, cy) = match press.touch {
            Some((tx, ty)) => (n.rect.x + tx, n.rect.y + ty),
            None => (n.rect.x + n.rect.w / 2, n.rect.y + n.rect.h / 2),
        };
        let outer = self.clip;
        // Flutter's InkRipple clips the expanding disc to the Material box
        // (referenceBox / rectCallback). Intersect with the node's own rect so
        // the splash never spills outside the button and stays a small layer.
        let node_clip = match n.clip {
            Some(c) => intersect(c, n.rect),
            None => n.rect,
        };
        self.clip = match (outer, node_clip) {
            (Some(a), b) => Some(intersect(a, b)),
            (None, b) => Some(b),
        };
        self.draw_ripple(cx, cy, radius, color);
        self.clip = outer;
    }

    /// Two diagonal strokes (a check mark), integer Bresenham-ish stepping.
    pub fn draw_check(&mut self, r: Rect, c: Rgba, width: i64) {
        let w = width.max(1);
        let (x0, y0) = (r.x, r.y + r.h / 2);
        let (x1, y1) = (r.x + r.w / 3, r.y + r.h - 1);
        let (x2, y2) = (r.x + r.w - 1, r.y);
        line(self, x0, y0, x1, y1, c, w);
        line(self, x1, y1, x2, y2, c, w);
    }
}

/// 4x4 coverage of a thick straight segment at one pixel. Sub-sample centres
/// live in an 8x integer grid (half-subpixel resolution); a squared
/// point-to-segment distance keeps the whole test in integer arithmetic. The
/// pen half-width is at least half a pixel so a 1px line stays visible.
fn line_coverage(x0: i64, y0: i64, x1: i64, y1: i64, x: i64, y: i64, width: i64) -> u8 {
    let (ax, ay, bx, by) = (x0 * 8, y0 * 8, x1 * 8, y1 * 8);
    let (dx, dy) = (bx - ax, by - ay);
    let l2 = dx * dx + dy * dy;
    let half = (width.max(1) / 2) * 8;
    let half = half.max(4);
    let h2 = half * half;
    let mut hits = 0i64;
    for sy in 0..EDGE_AA {
        for sx in 0..EDGE_AA {
            let px = x * 8 + 2 * sx + 1;
            let py = y * 8 + 2 * sy + 1;
            let (apx, apy) = (px - ax, py - ay);
            let tnum = apx * dx + apy * dy;
            let hit = if l2 == 0 || tnum <= 0 {
                apx * apx + apy * apy <= h2
            } else if tnum >= l2 {
                let (bpx, bpy) = (px - bx, py - by);
                bpx * bpx + bpy * bpy <= h2
            } else {
                let cross = apx * dy - apy * dx;
                cross * cross <= h2 * l2
            };
            if hit {
                hits += 1;
            }
        }
    }
    ((255 * hits + (EDGE_AA * EDGE_AA) / 2) / (EDGE_AA * EDGE_AA)) as u8
}

/// Integer line with a square pen.
fn line(canvas: &mut Canvas, x0: i64, y0: i64, x1: i64, y1: i64, c: Rgba, width: i64) {
    if canvas.edge_aa && width > 2 {
        // Thick segments use the analytic quad path, sharing coverage with
        // corners. Thin lines keep the 4x fast path below. Capture never
        // reaches either branch.
        let half_fp = ((width.max(1) as i128 * crate::aaa::ONE as i128 / 2)
            .max(crate::aaa::ONE as i128 / 2)) as i64;
        let pad = (half_fp >> 16) + 1;
        let xlo = x0.min(x1) - pad;
        let xhi = x0.max(x1) + pad;
        let ylo = y0.min(y1) - pad;
        let yhi = y0.max(y1) + pad;
        let path = crate::aaa::line_path(x0, y0, x1, y1, half_fp);
        let mut row = vec![0i64; canvas.width as usize];
        for y in ylo..=yhi {
            for a in row.iter_mut() {
                *a = 0;
            }
            if y < 0 || y >= canvas.height as i64 {
                continue;
            }
            crate::aaa::rasterize_convex_row(&path, &mut row, 0, y);
            for x in xlo..=xhi {
                if x < 0 || x >= canvas.width as i64 {
                    continue;
                }
                let cov = row[x as usize].clamp(0, 255) as u8;
                canvas.set_covered(x, y, c, cov);
            }
        }
        return;
    }
    if canvas.edge_aa {
        // Thin lines keep the analytic 4x subsample fast path; the coverage
        // test already accounts for pen width. Capture never reaches here.
        let pad = width.max(1) / 2 + 1;
        let xlo = x0.min(x1) - pad;
        let xhi = x0.max(x1) + pad;
        let ylo = y0.min(y1) - pad;
        let yhi = y0.max(y1) + pad;
        for y in ylo..=yhi {
            for x in xlo..=xhi {
                let cov = line_coverage(x0, y0, x1, y1, x, y, width);
                canvas.set_covered(x, y, c, cov);
            }
        }
        return;
    }
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx - dy;
    let (mut x, mut y) = (x0, y0);
    let half = width.max(1) / 2;
    loop {
        for oy in -half..=half {
            for ox in -half..=half {
                canvas.set(x + ox, y + oy, c);
            }
        }
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x += sx;
        }
        if e2 < dx {
            err += dx;
            y += sy;
        }
    }
}

/// Apply a fixed-point alpha to a colour.
fn with_alpha(c: Rgba, alpha: i64) -> Rgba {
    Rgba {
        r: c.r,
        g: c.g,
        b: c.b,
        a: ((alpha.clamp(0, UNIT) * 255) / UNIT) as u8,
    }
}

/// Soft shadow: three rounded layers, offset down-right, darkening towards
/// the box. The alpha is derived from the strength (0..8) only.
fn draw_shadow(canvas: &mut Canvas, n: &LaidOut) {
    // Per-node material elevation wins over the legacy strength shadow. The
    // level is already resolved/projected in layout; capture records it for
    // replay instead of rasterizing pixels.
    let level = n.elevation.clamp(0, 6);
    if matches!(level, 1 | 2 | 3 | 4 | 6) {
        if canvas.is_capturing() {
            crate::effects::emit_elevation_command(canvas, n, level);
        } else {
            crate::effects::draw_elevation(canvas, n, level as usize);
        }
        return;
    }
    let s = n.effects.shadow.clamp(0, 8);
    if s == 0 {
        return;
    }
    let dx = s / 2 + 1;
    let dy = s / 2 + 1;
    for i in 0..3i64 {
        let grow = s - i;
        if grow <= 0 {
            continue;
        }
        let alpha = (255 * (i + 1)) / 12;
        let r = Rect {
            x: n.rect.x - grow + dx,
            y: n.rect.y - grow + dy,
            w: n.rect.w + grow * 2,
            h: n.rect.h + grow * 2,
        };
        let grown = RoundingRadii {
            top_left: CornerRadii {
                x: n.radii.top_left.x + grow,
                y: n.radii.top_left.y + grow,
            },
            top_right: CornerRadii {
                x: n.radii.top_right.x + grow,
                y: n.radii.top_right.y + grow,
            },
            bottom_left: CornerRadii {
                x: n.radii.bottom_left.x + grow,
                y: n.radii.bottom_left.y + grow,
            },
            bottom_right: CornerRadii {
                x: n.radii.bottom_right.x + grow,
                y: n.radii.bottom_right.y + grow,
            },
        };
        canvas.fill_rounded_radii(r, with_alpha(Rgba::rgb(0, 0, 0), alpha), grown);
    }
}

/// Draw one laid-out node. Box first, glyphs on top; children are drawn later
/// because `layout()` emits pre-order. Local effects use a frame snapshot
/// taken before this node; whole-frame blur passes run after all nodes.
pub fn draw(canvas: &mut Canvas, n: &LaidOut) {
    // A node with an active state-paint transition carries its blended paint
    // from layout. Everything else uses the original discrete transform, and
    // default-state nodes keep the unmodified LaidOut byte-for-byte.
    let styled = match n.state_paint.as_ref() {
        Some(paint) => {
            let mut blended = n.clone();
            paint.apply(&mut blended);
            Some(blended)
        }
        None => (n.state != NodeState::Default).then(|| interaction::style_node(n)),
    };
    let n = styled.as_ref().unwrap_or(n);
    // An outer clip (the incremental dirty region) narrows the node's own
    // clip. With no outer clip the intersection is the node clip, so the full
    // reference path is unchanged.
    let outer = canvas.clip;
    canvas.clip = match (outer, n.clip) {
        (Some(o), Some(p)) => Some(intersect(o, p)),
        (Some(o), None) => Some(o),
        (None, p) => p,
    };
    // Closure A: only content drawn inside a rounded surface gets the
    // container's coverage. Capture ignores edge_aa, so command bytes are safe.
    let outer_shape = canvas.clip_shape;
    canvas.clip_shape = if canvas.edge_aa {
        n.clip_shape.as_ref().copied()
    } else {
        None
    };
    let backdrop = if !canvas.is_capturing() && crate::effects::has_local_effects(n) {
        Some(canvas.pixels.clone())
    } else {
        None
    };
    draw_shadow(canvas, n);
    draw_focus_ring(canvas, n);
    draw_box(canvas, n);
    draw_active_inset(canvas, n);
    if n.kind == Kind::Image {
        draw_image(canvas, n);
    }
    draw_control(canvas, n);
    if n.kind == Kind::Icon {
        draw_icon(canvas, n);
    } else {
        draw_text(canvas, n);
    }
    draw_tone(canvas, n);
    // The particle field is part of the node's ordinary drawing, so local
    // effects see it in their silhouette snapshot.
    if let Some(particles) = n.particles {
        let color = particles.color.unwrap_or(n.accent);
        canvas.fill_particles(n.rect, particles, color);
    }
    if let Some(backdrop) = backdrop {
        crate::effects::apply_node(canvas, n, &backdrop);
    }
    canvas.clip = outer;
    canvas.clip_shape = outer_shape;
}

/// Brand-coloured 2 px stroke confined to the two pixel bands outside the
/// node rect. The four strips never touch the rect interior.
fn draw_focus_ring(canvas: &mut Canvas, n: &LaidOut) {
    if n.state != NodeState::Focus || n.rect.w <= 0 || n.rect.h <= 0 {
        return;
    }
    let r = n.rect;
    let color = with_alpha(n.accent, n.effects.opacity);
    let width = r.w.saturating_add(FOCUS_RING * 2);
    canvas.fill_rect(
        Rect {
            x: r.x.saturating_sub(FOCUS_RING),
            y: r.y.saturating_sub(FOCUS_RING),
            w: width,
            h: FOCUS_RING,
        },
        color,
    );
    canvas.fill_rect(
        Rect {
            x: r.x.saturating_sub(FOCUS_RING),
            y: r.y.saturating_add(r.h),
            w: width,
            h: FOCUS_RING,
        },
        color,
    );
    canvas.fill_rect(
        Rect {
            x: r.x.saturating_sub(FOCUS_RING),
            y: r.y,
            w: FOCUS_RING,
            h: r.h,
        },
        color,
    );
    canvas.fill_rect(
        Rect {
            x: r.x.saturating_add(r.w),
            y: r.y,
            w: FOCUS_RING,
            h: r.h,
        },
        color,
    );
}

/// Pressed appearance: a 1 px top/left inset stroke over the darkened box.
fn draw_active_inset(canvas: &mut Canvas, n: &LaidOut) {
    if n.state != NodeState::Active || n.rect.w <= 0 || n.rect.h <= 0 {
        return;
    }
    let color = with_alpha(interaction::press_color(n.accent), n.effects.opacity);
    canvas.fill_rect(
        Rect {
            x: n.rect.x,
            y: n.rect.y,
            w: n.rect.w,
            h: 1,
        },
        color,
    );
    canvas.fill_rect(
        Rect {
            x: n.rect.x,
            y: n.rect.y,
            w: 1,
            h: n.rect.h,
        },
        color,
    );
}

/// Semantic-tone dot at the upper-right inside edge of the node rect.
fn draw_tone(canvas: &mut Canvas, n: &LaidOut) {
    if n.tone.is_none() {
        return;
    }
    let Some(base) = n.tone_color else {
        return;
    };
    if n.rect.w < 10 || n.rect.h < 10 {
        return;
    }
    let d = 4i64.min(n.rect.w.min(n.rect.h) / 2);
    let color = with_alpha(n.state.color(base), n.effects.opacity);
    canvas.fill_circle(
        Rect {
            x: n.rect.x + n.rect.w - d - 2,
            y: n.rect.y + 2,
            w: d,
            h: d,
        },
        color,
    );
}

/// One-pixel outline that preserves a transparent interior.
fn draw_outline(canvas: &mut Canvas, r: Rect, width: i64, color: Rgba, round: bool) {
    if r.w <= 0 || r.h <= 0 {
        return;
    }
    if round {
        canvas.stroke_circle(r, color, width);
        return;
    }
    let w = width.max(1).min(r.w.min(r.h) / 2);
    canvas.fill_rect(
        Rect {
            x: r.x,
            y: r.y,
            w: r.w,
            h: w,
        },
        color,
    );
    canvas.fill_rect(
        Rect {
            x: r.x,
            y: r.y + r.h - w,
            w: r.w,
            h: w,
        },
        color,
    );
    canvas.fill_rect(
        Rect {
            x: r.x,
            y: r.y + w,
            w,
            h: r.h - w * 2,
        },
        color,
    );
    canvas.fill_rect(
        Rect {
            x: r.x + r.w - w,
            y: r.y + w,
            w,
            h: r.h - w * 2,
        },
        color,
    );
}

fn draw_box(canvas: &mut Canvas, n: &LaidOut) {
    if n.fill.is_none() && n.border.is_none() {
        return;
    }
    let fill = n.fill.unwrap_or(Rgba {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    });
    let alpha = n.effects.opacity;
    // A radio button and a FAB are circles, not rounded squares.
    let round = n.kind == Kind::Radio || n.variant == Some(Variant::Fab);
    if n.border_w > 0 {
        if let Some(border) = n.border {
            if fill.a == 0 {
                if round {
                    draw_outline(canvas, n.rect, n.border_w, with_alpha(border, alpha), true);
                } else {
                    canvas.stroke_rounded_radii(
                        n.rect,
                        n.border_w,
                        with_alpha(border, alpha),
                        n.radii,
                    );
                }
            } else if round {
                canvas.fill_circle(n.rect, with_alpha(border, alpha));
            } else {
                canvas.fill_rounded_radii(n.rect, with_alpha(border, alpha), n.radii);
            }
        }
        if fill.a > 0 {
            let inner = inset(n.rect, n.border_w);
            match n.effects.gradient {
                // A circle cannot hold a gradient: the stops collapse to their mean.
                Some((a, b)) if round => {
                    let mid = crate::layout::lerp_color(a, b, UNIT / 2);
                    canvas.fill_circle(inner, with_alpha(mid, alpha))
                }
                Some((a, b)) => canvas.fill_rounded_gradient_radii(
                    inner,
                    a,
                    b,
                    Canvas::radii_inset(n.radii, n.border_w).scaled(inner.w, inner.h),
                    alpha,
                    n.effects.gradient_horizontal,
                ),
                None if round => canvas.fill_circle(inner, with_alpha(fill, alpha)),
                None => canvas.fill_rounded_radii(
                    inner,
                    with_alpha(fill, alpha),
                    Canvas::radii_inset(n.radii, n.border_w).scaled(inner.w, inner.h),
                ),
            }
        }
    } else if fill.a > 0 {
        match n.effects.gradient {
            Some((a, b)) => canvas.fill_rounded_gradient_radii(
                n.rect,
                a,
                b,
                n.radii,
                alpha,
                n.effects.gradient_horizontal,
            ),
            None => canvas.fill_rounded_radii(n.rect, with_alpha(fill, alpha), n.radii),
        }
    }
}

/// Pixel bounds of a node rect after the effective canvas clip and viewport.
fn image_bounds(canvas: &Canvas, r: Rect) -> Rect {
    let clip = canvas.effective_clip();
    let x0 = r.x.max(clip.x).max(0);
    let y0 = r.y.max(clip.y).max(0);
    let x1 =
        r.x.saturating_add(r.w)
            .min(clip.x.saturating_add(clip.w))
            .min(canvas.width as i64);
    let y1 =
        r.y.saturating_add(r.h)
            .min(clip.y.saturating_add(clip.h))
            .min(canvas.height as i64);
    Rect {
        x: x0,
        y: y0,
        w: (x1 - x0).max(0),
        h: (y1 - y0).max(0),
    }
}

fn image_radius(n: &LaidOut, inner: Rect) -> i64 {
    (n.radius - n.border_w)
        .max(0)
        .min(inner.w.min(inner.h).max(0) / 2)
}

/// Per-corner radii of the image content box, inset by the border width.
fn image_radii(n: &LaidOut, inner: Rect) -> RoundingRadii {
    let inset = Canvas::radii_inset(n.radii, n.border_w);
    inset.scaled(inner.w, inner.h)
}

/// Draw the cached PNG into the inner border rect with integer nearest-neighbour
/// sampling. Missing or invalid resources get a deterministic rounded X.
fn draw_image(canvas: &mut Canvas, n: &LaidOut) {
    let Some(src) = n.image_src.as_deref() else {
        return;
    };
    let inner = inset(n.rect, n.border_w);
    if inner.w <= 0 || inner.h <= 0 {
        return;
    }
    let Some(image) = image::cached(src) else {
        draw_image_placeholder(canvas, n, inner);
        return;
    };
    let fit = image::fit_rect(
        image.width,
        image.height,
        Rect {
            x: 0,
            y: 0,
            w: inner.w,
            h: inner.h,
        },
        n.image_fit,
    );
    if fit.w <= 0 || fit.h <= 0 {
        return;
    }
    let bounds = image_bounds(canvas, inner);
    let image_radii = image_radii(n, inner);
    let radius = image_radius(n, inner);
    let opacity = n.effects.opacity.clamp(0, UNIT);
    for y in bounds.y..bounds.y.saturating_add(bounds.h) {
        let v = y - inner.y - fit.y;
        if v < 0 || v >= fit.h {
            continue;
        }
        let sy = ((v as i128 * image.height as i128) / fit.h as i128)
            .clamp(0, image.height as i128 - 1) as usize;
        for x in bounds.x..bounds.x.saturating_add(bounds.w) {
            let u = x - inner.x - fit.x;
            if u < 0 || u >= fit.w {
                continue;
            }
            let coverage = if canvas.edge_aa {
                let cov = if Canvas::uniform_radius(image_radii).is_some() {
                    Canvas::round_coverage(inner, x, y, radius)
                } else {
                    Canvas::radii_coverage(inner, x, y, image_radii)
                };
                if cov == 0 {
                    continue;
                }
                cov
            } else {
                if !round_rect_contains(x, y, inner.x, inner.y, inner.w, inner.h, image_radii) {
                    continue;
                }
                255
            };
            let sx = ((u as i128 * image.width as i128) / fit.w as i128)
                .clamp(0, image.width as i128 - 1) as usize;
            let Some(index) = (sy * image.width + sx).checked_mul(4) else {
                continue;
            };
            let Some(px) = image.pixels.get(index..index + 4) else {
                continue;
            };
            // Source alpha carries edge coverage into the blend. `set` keeps
            // the capture command stream intact; capture rejects edge_aa so
            // coverage is always 255 there and no command changes.
            let alpha = (i64::from(px[3]) * opacity + UNIT / 2) / UNIT;
            let alpha = ((alpha * i64::from(coverage)) / 255).clamp(0, 255) as u8;
            if alpha == 0 {
                continue;
            }
            canvas.set(
                x,
                y,
                Rgba {
                    r: px[0],
                    g: px[1],
                    b: px[2],
                    a: alpha,
                },
            );
        }
    }
}

/// Neutral rounded box with a diagonal cross for missing/unreadable PNGs.
fn draw_image_placeholder(canvas: &mut Canvas, n: &LaidOut, inner: Rect) {
    let bounds = image_bounds(canvas, inner);
    let image_radii = image_radii(n, inner);
    let bg = with_alpha(Rgba::rgb(0x86, 0x8e, 0x98), n.effects.opacity);
    let cross = with_alpha(Rgba::rgb(0xf1, 0xf3, 0xf5), n.effects.opacity);
    let w = inner.w as i128;
    let h = inner.h as i128;
    let limit = ((w * w + h * h) / 4).max(1);
    for y in bounds.y..bounds.y.saturating_add(bounds.h) {
        let v = (y - inner.y) as i128;
        for x in bounds.x..bounds.x.saturating_add(bounds.w) {
            if !round_rect_contains(x, y, inner.x, inner.y, inner.w, inner.h, image_radii) {
                continue;
            }
            let u = (x - inner.x) as i128;
            let d1 = (u * h - v * w).abs();
            let d2 = (u * h + v * w - w * h).abs();
            canvas.set(
                x,
                y,
                if d1 <= limit || d2 <= limit {
                    cross
                } else {
                    bg
                },
            );
        }
    }
}

/// Solid right-pointing triangle: vertical left edge, apex at right-middle.
fn triangle_right(canvas: &mut Canvas, r: Rect, c: Rgba) {
    if r.w <= 0 || r.h <= 0 {
        return;
    }
    let half = r.h / 2;
    for y in 0..r.h {
        let d = (y - half).abs();
        let w = (r.w.saturating_mul((half - d).max(0)) * 2) / r.h.max(1);
        canvas.fill_rect(
            Rect {
                x: r.x,
                y: r.y + y,
                w,
                h: 1,
            },
            c,
        );
    }
}

/// Solid down-pointing triangle: horizontal top edge, apex at bottom-middle.
fn triangle_down(canvas: &mut Canvas, r: Rect, c: Rgba) {
    if r.w <= 0 || r.h <= 0 {
        return;
    }
    let half = r.w / 2;
    for x in 0..r.w {
        let d = (x - half).abs();
        let h = (r.h.saturating_mul((half - d).max(0)) * 2) / r.w.max(1);
        canvas.fill_rect(
            Rect {
                x: r.x + x,
                y: r.y,
                w: 1,
                h,
            },
            c,
        );
    }
}

/// Greedy word wrap at `max` built-in glyph columns; overlong words are chunked.
fn wrap_text(text: &str, max: usize) -> Vec<String> {
    let max = max.max(1);
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let mut rest = word.to_string();
        while rest.chars().count() > max {
            if !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
            }
            let chunk: String = rest.chars().take(max).collect();
            lines.push(chunk);
            rest = rest.chars().skip(max).collect();
        }
        if rest.is_empty() {
            continue;
        }
        if cur.is_empty() {
            cur = rest;
        } else if cur.chars().count() + 1 + rest.chars().count() <= max {
            cur.push(' ');
            cur.push_str(&rest);
        } else {
            lines.push(std::mem::take(&mut cur));
            cur = rest;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// Wrapped lines with an optional `maxLines` cap: the last retained line is
/// hard-truncated to the same integer glyph width and ends in ASCII dots.
fn wrap_text_limited(text: &str, max: usize, max_lines: Option<usize>) -> Vec<String> {
    let mut lines = wrap_text(text, max);
    let Some(limit) = max_lines else {
        return lines;
    };
    if lines.len() <= limit {
        return lines;
    }
    lines.truncate(limit);
    if let Some(last) = lines.last_mut() {
        let width = max.max(1);
        let dots: String = "...".chars().take(width).collect();
        let keep = width.saturating_sub(dots.chars().count());
        let mut shortened: String = last.trim_end().chars().take(keep).collect();
        shortened.push_str(&dots);
        *last = shortened;
    }
    lines
}

/// Kind-specific widget drawing.
fn draw_control(canvas: &mut Canvas, n: &LaidOut) {
    let alpha = n.effects.opacity;
    let accent = with_alpha(n.accent, alpha);
    let metrics = n.font_metrics;
    match (&n.control, n.kind) {
        (Some(Control::Progress { value }), _) => {
            let w = (n.rect.w * (*value).clamp(0, UNIT)) / UNIT;
            if w > 0 {
                canvas.fill_rect(
                    Rect {
                        x: n.rect.x,
                        y: n.rect.y,
                        w,
                        h: n.rect.h,
                    },
                    accent,
                );
            }
        }
        (Some(Control::Slider { value, min, max }), _) => {
            let span = max.saturating_sub(*min).max(1);
            let t = ((value.saturating_sub(*min)) * UNIT) / span;
            let w = (n.rect.w * t.clamp(0, UNIT)) / UNIT;
            if w > 0 {
                canvas.fill_rect(
                    Rect {
                        x: n.rect.x,
                        y: n.rect.y,
                        w,
                        h: n.rect.h,
                    },
                    accent,
                );
            }
            let knob = n.rect.h.max(8);
            let cx = n.rect.x + w;
            canvas.fill_circle(
                Rect {
                    x: cx - knob / 2,
                    y: n.rect.y - (knob - n.rect.h) / 2,
                    w: knob,
                    h: knob,
                },
                accent,
            );
        }
        (Some(Control::Switch { on }), _) => {
            // The pill box is drawn by `draw_box`; this is the knob.
            let inset = 3i64;
            let d = n.rect.h.saturating_sub(inset * 2).max(2);
            let cx = if *on {
                n.rect.x + n.rect.w - d - inset
            } else {
                n.rect.x + inset
            };
            canvas.fill_circle(
                Rect {
                    x: cx,
                    y: n.rect.y + inset,
                    w: d,
                    h: d,
                },
                accent,
            );
        }
        (Some(Control::Checkbox { checked }), _) => {
            // The square box is drawn by `draw_box`; this is the check mark.
            if *checked {
                let r = inset(n.rect, 3);
                canvas.draw_check(r, accent, 2);
            }
        }
        (Some(Control::Radio { selected }), _) => {
            // The round box is drawn by `draw_box`; this is the selected dot.
            if *selected {
                let d = n.rect.w.min(n.rect.h) / 3;
                canvas.fill_circle(
                    Rect {
                        x: n.rect.x + (n.rect.w - d) / 2,
                        y: n.rect.y + (n.rect.h - d) / 2,
                        w: d,
                        h: d,
                    },
                    accent,
                );
            }
        }
        (Some(Control::Tabs { labels, selected }), _) => {
            let scale = n.text_scale.clamp(1, 8);
            let th = font::GLYPH_H * scale;
            // High-contrast mode sets text_color even for nodes with no text.
            let label_color = match n.text_color {
                Some(c) => with_alpha(c, alpha),
                None => accent,
            };
            let mut x = n.rect.x + 2;
            for (i, label) in labels.iter().enumerate() {
                let w = font::text_width(label, scale, metrics);
                if i == *selected {
                    canvas.fill_rect(
                        Rect {
                            x,
                            y: n.rect.y + n.rect.h - 3,
                            w,
                            h: 2,
                        },
                        accent,
                    );
                }
                draw_glyphs(canvas, label, x, n.rect.y + 2, scale, label_color, metrics);
                x += w + 8;
            }
            let _ = th;
        }
        (Some(Control::Spinner), _) => {
            // Deterministic ring with a rotating gap (phase comes from the
            // animation clock, never from the wall clock).
            let phase = (n.phase / 100) % 8;
            let ring = inset(n.rect, 1);
            canvas.stroke_circle(ring, with_alpha(n.accent, alpha), 2);
            let cx = ring.x + ring.w / 2;
            let cy = ring.y + ring.h / 2;
            let rad = ring.w.min(ring.h) / 2;
            for k in 0..8i64 {
                if k == phase {
                    continue;
                }
                let a = k * 45;
                let (dx, dy) = (sin45(a), cos45(a));
                canvas.fill_rect(
                    Rect {
                        x: cx + dx * rad / 2 - 2,
                        y: cy + dy * rad / 2 - 2,
                        w: 4,
                        h: 4,
                    },
                    accent,
                );
            }
        }
        (
            Some(Control::Table {
                columns,
                rows,
                headers,
            }),
            _,
        ) => {
            let cols = (*columns).max(1);
            let rows = (*rows).max(1);
            let cw = n.rect.w / cols;
            let rh = n.rect.h / (rows + 1);
            if cw > 0 && rh > 0 {
                let ink = with_alpha(INK, alpha);
                let line = with_alpha(LINE, alpha);
                let head = with_alpha(n.accent, alpha);
                let on = with_alpha(n.on_accent, alpha);
                canvas.fill_rect(
                    Rect {
                        x: n.rect.x,
                        y: n.rect.y,
                        w: n.rect.w,
                        h: rh,
                    },
                    head,
                );
                for c in 0..cols {
                    if let Some(label) = headers.get(c as usize) {
                        let ty = n.rect.y + ((rh - font::GLYPH_H) / 2).max(0);
                        draw_glyphs(canvas, label, n.rect.x + c * cw + 2, ty, 1, on, metrics);
                    }
                }
                for c in 1..cols {
                    canvas.fill_rect(
                        Rect {
                            x: n.rect.x + c * cw,
                            y: n.rect.y,
                            w: 1,
                            h: n.rect.h,
                        },
                        line,
                    );
                }
                for r in 1..=rows {
                    canvas.fill_rect(
                        Rect {
                            x: n.rect.x,
                            y: n.rect.y + r * rh,
                            w: n.rect.w,
                            h: 1,
                        },
                        line,
                    );
                }
                let mut num = 1i64;
                for r in 0..rows {
                    for c in 0..cols {
                        let s = num.to_string();
                        draw_glyphs(
                            canvas,
                            &s,
                            n.rect.x + c * cw + 2,
                            n.rect.y + (r + 1) * rh + 2,
                            1,
                            ink,
                            metrics,
                        );
                        num += 1;
                    }
                }
            }
        }
        (Some(Control::Chart { values }), _) => {
            let count = values.len() as i64;
            if count > 0 && n.rect.w > 0 && n.rect.h > 0 {
                let max = values.iter().map(|v| (*v).max(0)).max().unwrap_or(1).max(1);
                let bw = (n.rect.w / count).max(1);
                let base = n.rect.y + n.rect.h - 1;
                let axis = with_alpha(INK, alpha);
                let a2 = with_alpha(crate::layout::lerp_color(n.accent, LIGHT, UNIT / 2), alpha);
                canvas.fill_rect(
                    Rect {
                        x: n.rect.x,
                        y: base,
                        w: n.rect.w,
                        h: 1,
                    },
                    axis,
                );
                for (i, v) in values.iter().enumerate() {
                    let h = n.rect.h.saturating_sub(2) * (*v).max(0) / max;
                    if h > 0 {
                        let col = if i % 2 == 0 { accent } else { a2 };
                        canvas.fill_rect(
                            Rect {
                                x: n.rect.x + i as i64 * bw,
                                y: base - h,
                                w: (bw - 1).max(1),
                                h,
                            },
                            col,
                        );
                    }
                }
            }
        }
        (Some(Control::Calendar { columns, rows }), _) => {
            let cols = (*columns).max(1);
            let rows = (*rows).max(1);
            let cw = n.rect.w / cols;
            let rh = n.rect.h / (rows + 1);
            if cw > 0 && rh > 0 {
                let ink = with_alpha(INK, alpha);
                let line = with_alpha(LINE, alpha);
                let head = with_alpha(n.accent, alpha);
                let on = with_alpha(n.on_accent, alpha);
                let wd = ["S", "M", "T", "W", "T", "F", "S"];
                canvas.fill_rect(
                    Rect {
                        x: n.rect.x,
                        y: n.rect.y,
                        w: n.rect.w,
                        h: rh,
                    },
                    head,
                );
                for c in 0..cols {
                    draw_glyphs(
                        canvas,
                        wd[(c as usize) % 7],
                        n.rect.x + c * cw + (cw / 2 - 3).max(0),
                        n.rect.y + 2,
                        1,
                        on,
                        metrics,
                    );
                }
                for c in 1..cols {
                    canvas.fill_rect(
                        Rect {
                            x: n.rect.x + c * cw,
                            y: n.rect.y,
                            w: 1,
                            h: n.rect.h,
                        },
                        line,
                    );
                }
                for r in 1..=rows {
                    canvas.fill_rect(
                        Rect {
                            x: n.rect.x,
                            y: n.rect.y + r * rh,
                            w: n.rect.w,
                            h: 1,
                        },
                        line,
                    );
                }
                // A deterministic "today" highlight, drawn before the day numbers.
                let (hc, hr) = (1 % cols, 2 % rows);
                canvas.fill_circle(
                    inset(
                        Rect {
                            x: n.rect.x + hc * cw,
                            y: n.rect.y + (hr + 1) * rh,
                            w: cw,
                            h: rh,
                        },
                        1,
                    ),
                    with_alpha(n.accent, alpha),
                );
                let mut day = 1i64;
                for r in 0..rows {
                    for c in 0..cols {
                        let s = day.to_string();
                        let col = if c == hc && r == hr { on } else { ink };
                        draw_glyphs(
                            canvas,
                            &s,
                            n.rect.x + c * cw + 2,
                            n.rect.y + (r + 1) * rh + 2,
                            1,
                            col,
                            metrics,
                        );
                        day += 1;
                    }
                }
            }
        }
        (Some(Control::RichText { text }), _) => {
            let scale = n.text_scale.clamp(1, 8);
            let line_h = font::GLYPH_H * scale + 2;
            let lines = match metrics {
                font::FontMetrics::Fixed => {
                    let max_chars =
                        (n.rect.w.saturating_sub(4) / font::advance(scale).max(1)).max(1) as usize;
                    wrap_text(text, max_chars)
                }
                font::FontMetrics::Proportional => {
                    font::wrap_width(text, n.rect.w.saturating_sub(4).max(1), scale)
                }
            };
            let ink = with_alpha(n.text_color.unwrap_or(INK), alpha);
            for (i, wrapped) in lines.iter().enumerate() {
                let y = n.rect.y + 2 + i as i64 * line_h;
                if y >= n.rect.y + n.rect.h {
                    break;
                }
                draw_glyphs(canvas, wrapped, n.rect.x + 2, y, scale, ink, metrics);
            }
        }
        (Some(Control::Player), _) => {
            let line = with_alpha(LINE, alpha);
            let cy = n.rect.y + n.rect.h / 2;
            let track = Rect {
                x: n.rect.x + 4,
                y: cy - 1,
                w: (n.rect.w - 8).max(0),
                h: 2,
            };
            canvas.fill_rect(track, line);
            let progress = track.w * 2 / 5;
            canvas.fill_rect(
                Rect {
                    x: track.x,
                    y: cy - 1,
                    w: progress,
                    h: 2,
                },
                accent,
            );
            let t = n.rect.h.min(n.rect.w) / 2;
            triangle_right(
                canvas,
                Rect {
                    x: n.rect.x + 6,
                    y: n.rect.y + (n.rect.h - t) / 2,
                    w: (t / 2).max(1),
                    h: t,
                },
                accent,
            );
            canvas.fill_circle(
                Rect {
                    x: track.x + progress - 3,
                    y: cy - 3,
                    w: 6,
                    h: 6,
                },
                accent,
            );
        }
        (Some(Control::Map), _) => {
            let road = with_alpha(LINE, alpha);
            canvas.fill_rect(n.rect, with_alpha(LAND, alpha));
            for f in 1..4 {
                canvas.fill_rect(
                    Rect {
                        x: n.rect.x + n.rect.w * f / 4,
                        y: n.rect.y,
                        w: 2,
                        h: n.rect.h,
                    },
                    road,
                );
                canvas.fill_rect(
                    Rect {
                        x: n.rect.x,
                        y: n.rect.y + n.rect.h * f / 4,
                        w: n.rect.w,
                        h: 2,
                    },
                    road,
                );
            }
            line(
                canvas,
                n.rect.x,
                n.rect.y + n.rect.h / 3,
                n.rect.x + n.rect.w,
                n.rect.y + 2 * n.rect.h / 3,
                accent,
                2,
            );
            let d = (n.rect.h.min(n.rect.w) / 5).max(2);
            let marker = Rect {
                x: n.rect.x + n.rect.w * 5 / 8 - d / 2,
                y: n.rect.y + n.rect.h * 3 / 8 - d / 2,
                w: d,
                h: d,
            };
            canvas.fill_circle(marker, accent);
            canvas.stroke_circle(marker, with_alpha(LIGHT, alpha), 2);
        }
        (Some(Control::Menu { items }), _) => {
            let count = items.len() as i64;
            let ih = if count > 0 { n.rect.h / count } else { 0 };
            if ih > 0 {
                let ink = with_alpha(INK, alpha);
                let line = with_alpha(LINE, alpha);
                let hi = with_alpha(n.accent, alpha);
                let on = with_alpha(n.on_accent, alpha);
                canvas.fill_rect(
                    Rect {
                        x: n.rect.x,
                        y: n.rect.y,
                        w: n.rect.w,
                        h: ih,
                    },
                    hi,
                );
                for (i, item) in items.iter().enumerate() {
                    let y = n.rect.y + i as i64 * ih;
                    let col = if i == 0 { on } else { ink };
                    draw_glyphs(
                        canvas,
                        item,
                        n.rect.x + 4,
                        y + ((ih - font::GLYPH_H) / 2).max(0),
                        1,
                        col,
                        metrics,
                    );
                    if i + 1 < items.len() {
                        canvas.fill_rect(
                            Rect {
                                x: n.rect.x,
                                y: y + ih,
                                w: n.rect.w,
                                h: 1,
                            },
                            line,
                        );
                    }
                }
            }
        }
        (Some(Control::Tooltip { text }), _) => {
            let bubble = with_alpha(BUBBLE, alpha);
            let on = with_alpha(LIGHT, alpha);
            let tail_h = (n.rect.h / 5).max(2);
            let body = Rect {
                x: n.rect.x,
                y: n.rect.y,
                w: n.rect.w,
                h: (n.rect.h - tail_h).max(1),
            };
            canvas.fill_rounded_radii(
                body,
                bubble,
                Canvas::radii_inset(n.radii, 0).scaled(body.w, body.h),
            );
            let tw = (n.rect.w / 6).max(4);
            triangle_down(
                canvas,
                Rect {
                    x: n.rect.x + (n.rect.w - tw) / 2,
                    y: body.y + body.h - 1,
                    w: tw,
                    h: tail_h,
                },
                bubble,
            );
            let scale = n.text_scale.clamp(1, 8);
            let tlen = font::text_width(text, scale, metrics);
            let tx = body.x + ((body.w - tlen) / 2).max(2);
            let ty = body.y + ((body.h - font::GLYPH_H * scale) / 2).max(0);
            draw_glyphs(canvas, text, tx, ty, scale, on, metrics);
        }
        (Some(Control::Extended { code, values }), _) => {
            crate::primitives::draw(canvas, *code, values, n.rect, accent, alpha);
        }
        // Hand-built LaidOut nodes may carry no control; still draw the kind.
        (_, Kind::Extended(code)) => {
            crate::primitives::draw(canvas, code, &[], n.rect, accent, alpha);
        }
        _ => {}
    }
}

/// 45-degree step on the unit circle, integer table (no trigonometry).
fn sin45(deg: i64) -> i64 {
    // (sin, cos) for multiples of 45 degrees, scaled by 1000.
    const TABLE: [(i64, i64); 8] = [
        (0, 1000),
        (707, 707),
        (1000, 0),
        (707, -707),
        (0, -1000),
        (-707, -707),
        (-1000, 0),
        (-707, 707),
    ];
    TABLE[((deg / 45).rem_euclid(8)) as usize].0
}

fn cos45(deg: i64) -> i64 {
    const TABLE: [(i64, i64); 8] = [
        (0, 1000),
        (707, 707),
        (1000, 0),
        (707, -707),
        (0, -1000),
        (-707, -707),
        (-1000, 0),
        (-707, 707),
    ];
    TABLE[((deg / 45).rem_euclid(8)) as usize].1
}

/// Draw a built-in icon glyph, centred and scaled to fit the node rect. The
/// glyph is a 24x24 integer vector path sampled to the same fixed-point
/// coverage as the text font; unknown names fall back to a neutral disc.
fn draw_icon(canvas: &mut Canvas, n: &LaidOut) {
    let Some(name) = n.icon_name.as_deref() else {
        return;
    };
    let box_size = n.rect.w.min(n.rect.h);
    if box_size <= 0 {
        return;
    }
    let color = with_alpha(n.text_color.unwrap_or(n.accent), n.effects.opacity);
    let x0 = n.rect.x + (n.rect.w - box_size) / 2;
    let y0 = n.rect.y + (n.rect.h - box_size) / 2;
    // A package icon from the IR `icons` table wins over the built-in glyph set;
    // a name only in the built-in set uses that; neither falls back to the disc.
    let package = n.icon_shape.as_ref();
    for py in 0..box_size {
        for px in 0..box_size {
            let cov = match package {
                Some(icon) => icons::path_coverage(icon, box_size, px, py),
                None => icons::icon_coverage(name, box_size, px, py),
            };
            let a = ((color.a as i64 * cov + UNIT / 2) / UNIT).clamp(0, 255) as u8;
            if a != 0 {
                canvas.set(x0 + px, y0 + py, Rgba { a, ..color });
            }
        }
    }
}

fn draw_text(canvas: &mut Canvas, n: &LaidOut) {
    if canvas.suppress_text {
        return;
    }
    let text = match &n.text {
        Some(t) if !t.is_empty() => t,
        _ => return,
    };
    let color = with_alpha(
        n.text_color.unwrap_or(Rgba::rgb(0, 0, 0)),
        n.effects.opacity,
    );
    let scale = n.text_scale.clamp(1, 8);
    let metrics = n.font_metrics;
    let tw = font::text_width(text, scale, metrics);
    let th = font::GLYPH_H * scale;
    let pad = n.border_w.max(0) + 2;
    let (x0, y0) = match n.text_origin {
        Some((ox, oy)) => (ox, oy),
        None => {
            let x0 = match n.text_align {
                Align::Start => n.rect.x + pad,
                Align::Center => n.rect.x + ((n.rect.w - tw) / 2).max(pad),
                Align::End => n.rect.x + (n.rect.w - tw - pad).max(pad),
            };
            let y0 = match n.text_baseline {
                // Opt-in baseline: seat the bitmap glyph band on the real
                // baseline (top = baseline - glyph height). Default path keeps
                // the historical vertical centering byte-for-byte.
                Some(baseline) => baseline - th,
                None => n.rect.y + ((n.rect.h - th) / 2).max(pad),
            };
            (x0, y0)
        }
    };
    // Text that already fits keeps the historical single-line placement, so
    // committed frames are byte for byte unchanged. A zero line cap is opt-in.
    if n.max_lines == Some(0) {
        return;
    }
    if tw <= n.rect.w {
        draw_glyphs(canvas, text, x0, y0, scale, color, metrics);
        return;
    }
    // Overflowing text is wrapped into lines that fit the node and then painted
    // through the canvas clip, so no glyph pixel spills onto a neighbour or the
    // next node; lines past the bottom edge are clipped the same way. Fixed
    // metrics keep the historical char-count wrap; proportional metrics wrap on
    // the same measured pixel width used above.
    let line_h = font::GLYPH_H * scale + 2;
    let lines = match metrics {
        font::FontMetrics::Fixed => {
            let adv = font::advance(scale).max(1);
            let max_chars = ((n.rect.w - 2 * pad).max(adv) / adv) as usize;
            wrap_text_limited(text, max_chars, n.max_lines)
        }
        font::FontMetrics::Proportional => {
            let max_w = (n.rect.w - 2 * pad).max(1);
            font::wrap_width_limited(text, max_w, scale, n.max_lines)
        }
    };
    let total_h = line_h.saturating_mul(lines.len() as i64);
    let (x_base, y_base) = match n.text_origin {
        Some((ox, oy)) => (ox, oy),
        None => {
            let y_base = n.rect.y + ((n.rect.h - total_h) / 2).max(pad);
            (n.rect.x + pad, y_base)
        }
    };
    let prev_clip = canvas.clip;
    canvas.clip = Some(intersect(canvas.effective_clip(), n.rect));
    for (i, line) in lines.iter().enumerate() {
        let lw = font::text_width(line, scale, metrics);
        let x = match n.text_origin {
            Some((ox, _)) => ox,
            None => match n.text_align {
                Align::Start => x_base,
                Align::Center => n.rect.x + ((n.rect.w - lw) / 2).max(pad),
                Align::End => n.rect.x + (n.rect.w - lw - pad).max(pad),
            },
        };
        draw_glyphs(
            canvas,
            line,
            x,
            y_base + (i as i64) * line_h,
            scale,
            color,
            metrics,
        );
    }
    canvas.clip = prev_clip;
}

/// Overlap of two rectangles, empty when they do not meet.
fn intersect(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = a.x.saturating_add(a.w).min(b.x.saturating_add(b.w));
    let y1 = a.y.saturating_add(a.h).min(b.y.saturating_add(b.h));
    Rect {
        x: x0,
        y: y0,
        w: (x1 - x0).max(0),
        h: (y1 - y0).max(0),
    }
}

/// Supersampling grid per output pixel for glyph coverage.
const AA_GRID: i64 = 4;
/// Fine units per output pixel: sample centres stay integral.
const AA_FINE: i64 = AA_GRID * 2;

/// Sub-samples per output pixel per axis for the high-resolution glyph set.
/// A finer grid than the block font keeps curved edges smooth at large sizes.
const SMOOTH_AA: i64 = 6;

/// Coverage of one pixel of a scaled glyph in fixed point (0..=UNIT).
///
/// The 5x7 pattern is bilinearly interpolated on an `AA_GRID x AA_GRID`
/// supersampling grid, then sharpened to a ~1 px feather so only edge pixels
/// soften; the glyph box and advance are untouched.
fn glyph_coverage(rows: &[u8; 7], scale: i64, px: i64, py: i64) -> i64 {
    let cell = scale * AA_FINE;
    let lit = |c: i64, r: i64| -> i64 {
        if (0..font::GLYPH_W).contains(&c) && (0..font::GLYPH_H).contains(&r) {
            i64::from(rows[r as usize] & (1u8 << (4 - c as u32)) != 0)
        } else {
            0
        }
    };
    let mut sum = 0i64;
    for sy in 0..AA_GRID {
        let y2 = py * AA_FINE + sy * 2 + 1;
        let (cy, fy) = (y2 / cell, y2 % cell);
        let dy = fy - cell / 2;
        let ny = cy + if dy < 0 { -1 } else { 1 };
        let wy = 128 * dy.abs() / scale;
        for sx in 0..AA_GRID {
            let x2 = px * AA_FINE + sx * 2 + 1;
            let (cx, fx) = (x2 / cell, x2 % cell);
            let dx = fx - cell / 2;
            let nx = cx + if dx < 0 { -1 } else { 1 };
            let wx = 128 * dx.abs() / scale;
            let (ax, ay) = (UNIT - wx, UNIT - wy);
            let g = (ay * (ax * lit(cx, cy) + wx * lit(nx, cy))
                + wy * (ax * lit(cx, ny) + wx * lit(nx, ny)))
                / UNIT;
            sum += ((g - UNIT / 2) * scale + UNIT / 2).clamp(0, UNIT);
        }
    }
    (sum + AA_GRID * AA_GRID / 2) / (AA_GRID * AA_GRID)
}

/// Draw a text run, blending partial coverage over whatever is on the canvas.
fn draw_glyphs(
    canvas: &mut Canvas,
    text: &str,
    x0: i64,
    y0: i64,
    scale: i64,
    color: Rgba,
    metrics: font::FontMetrics,
) {
    if scale >= font::SMOOTH_MIN_SCALE {
        draw_glyphs_smooth(canvas, text, x0, y0, scale, color, metrics);
        return;
    }
    // Block path is unreachable for the clamped scale (>= 1 uses the smooth
    // set); keep a running pen so every glyph consumes its selected advance.
    let mut pen = x0;
    for ch in text.chars() {
        let adv = font::glyph_advance(ch, scale, metrics);
        if let Some(rows) = font::glyph(ch) {
            if rows != [0; 7] {
                for py in 0..font::GLYPH_H * scale {
                    for px in 0..font::GLYPH_W * scale {
                        let cov = glyph_coverage(&rows, scale, px, py);
                        let a = ((color.a as i64 * cov + UNIT / 2) / UNIT).clamp(0, 255) as u8;
                        if a != 0 {
                            canvas.set(pen + px, y0 + py, Rgba { a, ..color });
                        }
                    }
                }
            }
        }
        pen = pen.saturating_add(adv);
    }
}

/// Fixed-point coverage of one pixel of a high-resolution glyph.
///
/// Each `SMOOTH_W x SMOOTH_H` source cell holds a 4-bit coverage value. The
/// glyph is drawn into a `w_out x h_out` box (`w_out` is the shared mono
/// advance, `h_out` the scaled cell height); pixel `(px, py)` is sampled by
/// `SMOOTH_AA x SMOOTH_AA` sub-points that bilinearly read the source grid.
/// The result blends smoothly with integer maths only and no external data.
fn smooth_coverage(
    rows: &[u64; font::SMOOTH_H as usize],
    w_out: i64,
    h_out: i64,
    px: i64,
    py: i64,
    first: i64,
    cells: i64,
) -> i64 {
    let level = font::SMOOTH_LEVEL;
    let cell = |c: i64, r: i64| -> i64 {
        if (0..font::SMOOTH_W).contains(&c) && (0..font::SMOOTH_H).contains(&r) {
            let shift = (font::SMOOTH_W - 1 - c) * 4;
            ((rows[r as usize] >> shift) & 0xF) as i64
        } else {
            0
        }
    };
    // Fixed passes `(0, SMOOTH_W)`: this reproduces the historical full-cell
    // mapping exactly. Proportional starts at the first inked column and maps
    // only `cells` source columns, cropping the left bearing without distortion.
    let src_h = font::SMOOTH_H;
    let in_w = (w_out * SMOOTH_AA).max(1);
    let in_h = (h_out * SMOOTH_AA).max(1);
    let mut sum = 0i64;
    for sy in 0..SMOOTH_AA {
        let sub_y = py * SMOOTH_AA + sy;
        let cy = sub_y * src_h / in_h;
        let fy = (sub_y * src_h * 16 / in_h) % 16;
        let ny = cy + 1;
        for sx in 0..SMOOTH_AA {
            let sub_x = px * SMOOTH_AA + sx;
            let cpos = sub_x * cells;
            let cx = first + cpos / in_w;
            let fx = (cpos * 16 / in_w) % 16;
            let nx = cx + 1;
            let v00 = cell(cx, cy);
            let v10 = cell(nx, cy);
            let v01 = cell(cx, ny);
            let v11 = cell(nx, ny);
            let top = v00 * (16 - fx) + v10 * fx;
            let bot = v01 * (16 - fx) + v11 * fx;
            let val = (top * (16 - fy) + bot * fy) / 256;
            sum += val * UNIT / level;
        }
    }
    (sum + SMOOTH_AA * SMOOTH_AA / 2) / (SMOOTH_AA * SMOOTH_AA)
}

/// Draw a run with the high-resolution glyph set. Characters outside the set
/// fall back to the 5x7 bitmap rasterised at the same scale so a mixed run
/// still has no gaps.
fn draw_glyphs_smooth(
    canvas: &mut Canvas,
    text: &str,
    x0: i64,
    y0: i64,
    scale: i64,
    color: Rgba,
    metrics: font::FontMetrics,
) {
    // Each glyph is drawn into a box as wide as its pen advance. Fixed maps the
    // whole 12x14 source cell into that box (byte-identical to the historical
    // mono layout). Proportional crops the left bearing and maps only the ink
    // plus a trailing gap into the box, so ink is never distorted.
    let band_h = font::GLYPH_H * scale;
    let mut pen = x0;
    for ch in text.chars() {
        let adv = font::glyph_advance(ch, scale, metrics);
        let w_out = adv;
        let (first, cells) = font::coverage_window(ch, metrics);
        match font::smooth_glyph(ch) {
            Some(rows) => {
                if !rows.iter().all(|&r| r == 0) {
                    for py in 0..band_h {
                        for px in 0..w_out {
                            let cov = smooth_coverage(&rows, w_out, band_h, px, py, first, cells);
                            let a = ((color.a as i64 * cov + UNIT / 2) / UNIT).clamp(0, 255) as u8;
                            if a != 0 {
                                canvas.set(pen + px, y0 + py, Rgba { a, ..color });
                            }
                        }
                    }
                }
            }
            None => {
                if let Some(rows) = font::glyph(ch) {
                    if rows != [0; 7] {
                        for py in 0..band_h {
                            for px in 0..font::GLYPH_W * scale {
                                let cov = glyph_coverage(&rows, scale, px, py);
                                let a =
                                    ((color.a as i64 * cov + UNIT / 2) / UNIT).clamp(0, 255) as u8;
                                if a != 0 {
                                    canvas.set(pen + px, y0 + py, Rgba { a, ..color });
                                }
                            }
                        }
                    }
                }
            }
        }
        pen = pen.saturating_add(adv);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordered_dither_thresholds_and_quantization_match_hand_values() {
        // Bayer recursion at four-bit levels: each (x,y) low bit selects a
        // 2x2 base value shifted by the remaining high-bit weight.
        assert_eq!(bayer256(0, 0), 0);
        assert_eq!(bayer256(1, 0), 32768);
        assert_eq!(bayer256(0, 1), 49152);
        assert_eq!(bayer256(1, 1), 16384);
        assert_eq!(bayer256(2, 0), 8192);
        assert_eq!(bayer256(0, 255), 65535);
        assert_eq!(bayer256(255, 255), 21845);

        let mid = [32768; 3];
        assert_eq!(dither_rgb8(mid, 0, 0), [127; 3]);
        assert_eq!(dither_rgb8(mid, 1, 0), [128; 3]);
        assert_eq!(dither_rgb8([0; 3], 1, 0), [0; 3]);
        assert_eq!(dither_rgb8([65535; 3], 1, 0), [255; 3]);
    }

    #[test]
    fn line_coverage_matches_analytic_4x_subsample_oracle() {
        // Independent f64 point-to-segment reference: sub-samples sit at
        // (sx+0.5)/4 inside the pixel, exactly the integer 8x-grid centres.
        fn reference(x0: i64, y0: i64, x1: i64, y1: i64, x: i64, y: i64, width: i64) -> u8 {
            let (ax, ay, bx, by) = (x0 as f64, y0 as f64, x1 as f64, y1 as f64);
            let (dx, dy) = (bx - ax, by - ay);
            let l2 = dx * dx + dy * dy;
            let half = ((width.max(1) / 2) as f64).max(0.5);
            let mut hits = 0i64;
            for sy in 0..4 {
                for sx in 0..4 {
                    let px = x as f64 + (sx as f64 + 0.5) / 4.0;
                    let py = y as f64 + (sy as f64 + 0.5) / 4.0;
                    let (apx, apy) = (px - ax, py - ay);
                    let t = if l2 == 0.0 {
                        0.0
                    } else {
                        ((apx * dx + apy * dy) / l2).clamp(0.0, 1.0)
                    };
                    let (qx, qy) = (ax + t * dx, ay + t * dy);
                    if (px - qx) * (px - qx) + (py - qy) * (py - qy) <= half * half {
                        hits += 1;
                    }
                }
            }
            ((255 * hits + 8) / 16) as u8
        }

        let cases: &[(i64, i64, i64, i64, i64, i64, i64)] = &[
            (0, 0, 8, 8, 4, 4, 1),
            (0, 0, 8, 8, 4, 5, 1),
            (0, 0, 8, 8, 5, 4, 1),
            (0, 0, 8, 0, 4, 0, 1),
            (0, 0, 8, 0, 4, -1, 1),
            (0, 0, 8, 0, 4, 1, 1),
            (2, 0, 2, 8, 2, 4, 1),
            (2, 0, 2, 8, 1, 4, 1),
            (0, 0, 8, 8, 4, 4, 3),
            (0, 0, 8, 8, 4, 5, 3),
            (0, 0, 0, 0, 0, 0, 1),
            (0, 0, 0, 0, 1, 1, 1),
        ];
        let mut partial = 0usize;
        for &(x0, y0, x1, y1, x, y, w) in cases {
            let got = line_coverage(x0, y0, x1, y1, x, y, w);
            let want = reference(x0, y0, x1, y1, x, y, w);
            assert_eq!(
                got, want,
                "coverage at ({x},{y}) on ({x0},{y0})->({x1},{y1})"
            );
            if got > 0 && got < 255 {
                partial += 1;
            }
        }
        assert!(partial >= 6, "the oracle must exercise partial coverage");
    }

    #[test]
    fn smooth_glyph_softens_edges_without_moving_ink() {
        // Every printable ASCII glyph lives in the high-resolution atlas, so a
        // real character at scale 3 exercises the smooth raster path.
        let ch = 'M';
        let _rows = font::smooth_glyph(ch).unwrap();
        let scale = 3;
        assert_eq!(font::advance(scale), (font::GLYPH_W + 1) * scale);

        let bg = Rgba::rgb(255, 255, 255);
        let mut canvas = Canvas::new(60, 40, bg);
        draw_glyphs(
            &mut canvas,
            "MM",
            2,
            2,
            scale,
            Rgba::rgb(0, 0, 0),
            font::FontMetrics::Fixed,
        );
        let pixel = |x: i64, y: i64| -> [u8; 3] {
            let i = (y as usize * canvas.width as usize + x as usize) * 4;
            [canvas.pixels[i], canvas.pixels[i + 1], canvas.pixels[i + 2]]
        };
        // The glyph cell softens: some inked interior pixels are darker than
        // the background but lighter than solid black, which proves AA runs.
        let band_h = font::GLYPH_H * scale;
        let w = font::advance(scale);
        let mut soft = false;
        for py in 0..band_h {
            for px in 0..w {
                let p = pixel(2 + px, 2 + py);
                if p[0] > 0 && p[0] < 255 {
                    soft = true;
                }
            }
        }
        assert!(soft, "smooth glyph must produce anti-aliased edge pixels");
        // Nothing is drawn left of the pen origin.
        assert_eq!(pixel(1, 6), [255, 255, 255]);
        let _ = w;
    }

    #[test]
    fn wrap_text_breaks_at_spaces_and_hard_breaks_long_words() {
        assert_eq!(wrap_text("", 4), Vec::<String>::new());
        assert_eq!(wrap_text("Ok", 4), vec!["Ok"]);
        assert_eq!(wrap_text("a b c", 3), vec!["a b", "c"]);
        // A word wider than the line is hard-broken.
        assert_eq!(wrap_text("abcdef", 3), vec!["abc", "def"]);
        // No space fits: the earlier word moves to the next line.
        assert_eq!(wrap_text("ab cd", 2), vec!["ab", "cd"]);
        assert_eq!(wrap_text("a b c d", 1), vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn max_lines_truncates_with_ascii_ellipsis() {
        let unlimited = wrap_text("alpha beta gamma delta", 10);
        assert_eq!(unlimited, vec!["alpha beta", "gamma", "delta"]);
        assert_eq!(
            wrap_text_limited("alpha beta gamma delta", 10, Some(2)),
            vec!["alpha beta", "gamma..."]
        );
        assert_eq!(wrap_text_limited("abcdef", 2, Some(1)), vec![".."]);
        assert_eq!(
            wrap_text_limited("any text", 4, Some(0)),
            Vec::<String>::new()
        );
        assert_eq!(
            wrap_text_limited("alpha beta gamma delta", 10, None),
            unlimited
        );
    }
}

pub fn inset(r: Rect, p: i64) -> Rect {
    let p = p.max(0);
    Rect {
        x: r.x + p,
        y: r.y + p,
        w: (r.w - p.saturating_mul(2)).max(0),
        h: (r.h - p.saturating_mul(2)).max(0),
    }
}
