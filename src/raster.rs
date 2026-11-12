//! Deterministic RGBA8 software raster: rounded fills/borders, R2 effects and
//! bitmap text. Every pixel write is integer arithmetic; the frame is opaque.

use crate::layout::Control;
use crate::{font, Align, Kind, LaidOut, Rect, Rgba};

/// Fixed-point unit: 1024 == fully opaque.
const UNIT: i64 = 1024;

pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    /// Optional clip rectangle (R2 `Scroll` viewport).
    pub clip: Option<Rect>,
}

impl Canvas {
    pub fn new(width: u32, height: u32, bg: Rgba) -> Canvas {
        let n = width as usize * height as usize * 4;
        let mut pixels = Vec::with_capacity(n);
        for _ in 0..width as usize * height as usize {
            pixels.extend_from_slice(&[bg.r, bg.g, bg.b, 255]);
        }
        Canvas {
            width,
            height,
            pixels,
            clip: None,
        }
    }

    pub fn set(&mut self, x: i64, y: i64, c: Rgba) {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return;
        }
        if let Some(clip) = self.clip {
            if x < clip.x || y < clip.y || x >= clip.x + clip.w || y >= clip.y + clip.h {
                return;
            }
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        if c.a == 0 {
            return;
        }
        if c.a == 255 {
            self.pixels[i] = c.r;
            self.pixels[i + 1] = c.g;
            self.pixels[i + 2] = c.b;
            self.pixels[i + 3] = 255;
            return;
        }
        let a = c.a as u32;
        let inv = 255 - a;
        let src = [c.r, c.g, c.b];
        for k in 0..3 {
            let dst = self.pixels[i + k] as u32;
            self.pixels[i + k] = ((src[k] as u32 * a + dst * inv) / 255) as u8;
        }
        self.pixels[i + 3] = 255;
    }

    pub fn fill_rect(&mut self, r: Rect, c: Rgba) {
        let x1 = (r.x + r.w).min(self.width as i64);
        let y1 = (r.y + r.h).min(self.height as i64);
        for y in r.y.max(0)..y1 {
            for x in r.x.max(0)..x1 {
                self.set(x, y, c);
            }
        }
    }

    /// Integer corner test: `true` when the pixel is outside the rounded box.
    fn outside_round(r: Rect, x: i64, y: i64, rad: i64) -> bool {
        if rad <= 0 {
            return false;
        }
        let cr = rad - 1;
        let limit = cr * cr;
        let dx = x - r.x;
        let cx = if dx < rad {
            cr - dx
        } else if dx >= r.w - rad {
            dx - (r.w - rad)
        } else {
            -1
        };
        let dy = y - r.y;
        let cy = if dy < rad {
            cr - dy
        } else if dy >= r.h - rad {
            dy - (r.h - rad)
        } else {
            -1
        };
        cx >= 0 && cy >= 0 && cx * cx + cy * cy > limit
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
        for y in r.y.max(0)..y1 {
            for x in r.x.max(0)..x1 {
                if Self::outside_round(r, x, y, rad) {
                    continue;
                }
                self.set(x, y, c);
            }
        }
    }

    /// Two-stop vertical gradient inside a rounded rectangle.
    pub fn fill_rounded_gradient(
        &mut self,
        r: Rect,
        top: Rgba,
        bottom: Rgba,
        radius: i64,
        alpha: i64,
    ) {
        if r.w <= 0 || r.h <= 0 {
            return;
        }
        let rad = radius.max(0).min(r.w.min(r.h) / 2);
        let x1 = (r.x + r.w).min(self.width as i64);
        let y1 = (r.y + r.h).min(self.height as i64);
        let last = r.h - 1;
        for y in r.y.max(0)..y1 {
            let t = if last <= 0 {
                UNIT
            } else {
                ((y - r.y) * UNIT) / last
            };
            let c = crate::layout::lerp_color(top, bottom, t);
            let c = with_alpha(c, alpha);
            for x in r.x.max(0)..x1 {
                if Self::outside_round(r, x, y, rad) {
                    continue;
                }
                self.set(x, y, c);
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
        for y in r.y.max(0)..y1 {
            for x in r.x.max(0)..x1 {
                let dx = x - cx;
                let dy = y - cy;
                if dx * dx + dy * dy <= limit {
                    self.set(x, y, c);
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
        for y in r.y.max(0)..y1 {
            for x in r.x.max(0)..x1 {
                let dx = x - cx;
                let dy = y - cy;
                let d = dx * dx + dy * dy;
                if d <= outer && d > inner {
                    self.set(x, y, c);
                }
            }
        }
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

/// Integer line with a square pen.
fn line(canvas: &mut Canvas, x0: i64, y0: i64, x1: i64, y1: i64, c: Rgba, width: i64) {
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
        canvas.fill_rounded(r, with_alpha(Rgba::rgb(0, 0, 0), alpha), n.radius + grow);
    }
}

/// Draw one laid-out node. Box first, glyphs on top; children are drawn later
/// because `layout()` emits pre-order.
pub fn draw(canvas: &mut Canvas, n: &LaidOut) {
    let prev_clip = canvas.clip;
    canvas.clip = n.clip;
    draw_shadow(canvas, n);
    draw_box(canvas, n);
    draw_control(canvas, n);
    draw_text(canvas, n);
    canvas.clip = prev_clip;
}

fn draw_box(canvas: &mut Canvas, n: &LaidOut) {
    let Some(fill) = n.fill else {
        return;
    };
    let alpha = n.effects.opacity;
    let inner_radius = (n.radius - n.border_w).max(0);
    // A radio button is a circle, not a rounded square.
    let round = n.kind == Kind::Radio;
    if n.border_w > 0 {
        if let Some(border) = n.border {
            if round {
                canvas.fill_circle(n.rect, with_alpha(border, alpha));
            } else {
                canvas.fill_rounded(n.rect, with_alpha(border, alpha), n.radius);
            }
        }
        let inner = inset(n.rect, n.border_w);
        match n.effects.gradient {
            // A circle cannot hold a gradient: the stops collapse to their mean.
            Some((a, b)) if round => {
                let mid = crate::layout::lerp_color(a, b, UNIT / 2);
                canvas.fill_circle(inner, with_alpha(mid, alpha))
            }
            Some((a, b)) => canvas.fill_rounded_gradient(inner, a, b, inner_radius, alpha),
            None if round => canvas.fill_circle(inner, with_alpha(fill, alpha)),
            None => canvas.fill_rounded(inner, with_alpha(fill, alpha), inner_radius),
        }
    } else {
        match n.effects.gradient {
            Some((a, b)) => canvas.fill_rounded_gradient(n.rect, a, b, n.radius, alpha),
            None => canvas.fill_rounded(n.rect, with_alpha(fill, alpha), n.radius),
        }
    }
}

/// Kind-specific widget drawing.
fn draw_control(canvas: &mut Canvas, n: &LaidOut) {
    let alpha = n.effects.opacity;
    let accent = with_alpha(n.accent, alpha);
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
            let adv = font::advance(scale);
            let th = font::GLYPH_H * scale;
            // High-contrast mode sets text_color even for nodes with no text.
            let label_color = match n.text_color {
                Some(c) => with_alpha(c, alpha),
                None => accent,
            };
            let mut x = n.rect.x + 2;
            for (i, label) in labels.iter().enumerate() {
                let w = (label.chars().count() as i64) * adv;
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
                draw_glyphs(canvas, label, x, n.rect.y + 2, scale, label_color);
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

fn draw_text(canvas: &mut Canvas, n: &LaidOut) {
    let text = match &n.text {
        Some(t) if !t.is_empty() => t,
        _ => return,
    };
    let color = with_alpha(
        n.text_color.unwrap_or(Rgba::rgb(0, 0, 0)),
        n.effects.opacity,
    );
    let scale = n.text_scale.clamp(1, 8);
    let tw = (text.chars().count() as i64).saturating_mul(font::advance(scale));
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
            let y0 = n.rect.y + ((n.rect.h - th) / 2).max(pad);
            (x0, y0)
        }
    };
    draw_glyphs(canvas, text, x0, y0, scale, color);
}

/// Draw a text run with the built-in bitmap font.
fn draw_glyphs(canvas: &mut Canvas, text: &str, x0: i64, y0: i64, scale: i64, color: Rgba) {
    for (idx, ch) in text.chars().enumerate() {
        let rows = match font::glyph(ch) {
            Some(r) => r,
            None => continue, // unsupported codepoint still consumes its advance
        };
        let gx = x0 + (idx as i64).saturating_mul(font::advance(scale));
        for (ry, mask) in rows.iter().enumerate() {
            for col in 0..5i64 {
                if mask & (1u8 << ((4 - col) as u32)) != 0 {
                    canvas.fill_rect(
                        Rect {
                            x: gx + col * scale,
                            y: y0 + ry as i64 * scale,
                            w: scale,
                            h: scale,
                        },
                        color,
                    );
                }
            }
        }
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
