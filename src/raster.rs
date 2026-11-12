//! Deterministic RGBA8 software raster: rounded fills/borders and bitmap text.
//! Every pixel write is integer arithmetic; the frame is fully opaque (A=255).

use crate::{font, Align, LaidOut, Rect, Rgba};

pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
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
        }
    }

    pub fn set(&mut self, x: i64, y: i64, c: Rgba) {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return;
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
        let cr = rad - 1;
        let limit = cr * cr;
        for y in r.y.max(0)..y1 {
            let dy = y - r.y;
            let cy = if dy < rad {
                cr - dy
            } else if dy >= r.h - rad {
                dy - (r.h - rad)
            } else {
                -1
            };
            for x in r.x.max(0)..x1 {
                let dx = x - r.x;
                let cx = if dx < rad {
                    cr - dx
                } else if dx >= r.w - rad {
                    dx - (r.w - rad)
                } else {
                    -1
                };
                if cx >= 0 && cy >= 0 && cx * cx + cy * cy > limit {
                    continue;
                }
                self.set(x, y, c);
            }
        }
    }
}

/// Draw one laid-out node. Box first, glyphs on top; children are drawn later
/// because `layout()` emits pre-order.
pub fn draw(canvas: &mut Canvas, n: &LaidOut) {
    if let Some(fill) = n.fill {
        if n.border_w > 0 {
            if let Some(border) = n.border {
                canvas.fill_rounded(n.rect, border, n.radius);
            }
            canvas.fill_rounded(
                crate::raster::inset(n.rect, n.border_w),
                fill,
                (n.radius - n.border_w).max(0),
            );
        } else {
            canvas.fill_rounded(n.rect, fill, n.radius);
        }
    }
    let text = match &n.text {
        Some(t) if !t.is_empty() => t,
        _ => return,
    };
    let color = n.text_color.unwrap_or(Rgba::rgb(0, 0, 0));
    let scale = n.text_scale.clamp(1, 8);
    let tw = (text.chars().count() as i64).saturating_mul(font::advance(scale));
    let th = font::GLYPH_H * scale;
    let pad = n.border_w.max(0) + 2;
    let x0 = match n.text_align {
        Align::Start => n.rect.x + pad,
        Align::Center => n.rect.x + ((n.rect.w - tw) / 2).max(pad),
        Align::End => n.rect.x + (n.rect.w - tw - pad).max(pad),
    };
    let y0 = n.rect.y + ((n.rect.h - th) / 2).max(pad);
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
