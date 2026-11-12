//! Deterministic RGBA8 software raster: rounded fills/borders, R2 effects,
//! bitmap text and R8 framebuffer effects. Every pixel write is integer
//! arithmetic; the frame is opaque.

use crate::display::Command;
use crate::layout::Control;
use crate::{font, Align, Kind, LaidOut, Rect, Rgba};

/// Fixed-point unit: 1024 == fully opaque.
const UNIT: i64 = 1024;

// R4 primitives use theme-independent, deterministic ink colours. They are
// only ever drawn for the eight new kinds, so they cannot affect v0.1 goldens.
const INK: Rgba = Rgba::rgb(0x1b, 0x1f, 0x27);
const LINE: Rgba = Rgba::rgb(0x90, 0x98, 0xa8);
const LIGHT: Rgba = Rgba::rgb(0xff, 0xff, 0xff);
const LAND: Rgba = Rgba::rgb(0xdf, 0xe6, 0xd8);
const BUBBLE: Rgba = Rgba::rgb(0x24, 0x2a, 0x38);

pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    /// Optional clip rectangle (R2 `Scroll` viewport).
    pub clip: Option<Rect>,
    capture: bool,
    commands: Vec<Command>,
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
            capture: false,
            commands: Vec::new(),
        }
    }

    /// Capture the exact high-level draw commands for the R6 display list.
    pub fn capturing(width: u32, height: u32, bg: Rgba) -> Canvas {
        let mut canvas = Canvas::new(width, height, bg);
        canvas.capture = true;
        canvas
    }

    pub fn into_commands(self) -> Vec<Command> {
        self.commands
    }

    pub(crate) fn is_capturing(&self) -> bool {
        self.capture
    }

    pub(crate) fn effective_clip(&self) -> Rect {
        let full = Rect {
            x: 0,
            y: 0,
            w: self.width as i64,
            h: self.height as i64,
        };
        match self.clip {
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

    fn emit(&mut self, command: Command) {
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
        let x1 = (r.x + r.w).min(self.width as i64);
        let y1 = (r.y + r.h).min(self.height as i64);
        for y in r.y.max(0)..y1 {
            for x in r.x.max(0)..x1 {
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
        for y in r.y.max(0)..y1 {
            for x in r.x.max(0)..x1 {
                if Self::outside_round(r, x, y, rad) {
                    continue;
                }
                self.set_quiet(x, y, c);
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
        let alpha_u8 = ((alpha.clamp(0, UNIT) * 255) / UNIT) as u8;
        if self.capture && alpha_u8 > 0 && self.visible_rect(r, top) {
            let clip = self.effective_clip();
            self.emit(Command::Gradient {
                rect: r,
                clip,
                radius: rad,
                top,
                bottom,
                alpha: alpha_u8,
            });
        }
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
                self.set_quiet(x, y, c);
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
/// because `layout()` emits pre-order. R8 local effects use a frame snapshot
/// taken before this node; whole-frame blur passes run after all nodes.
pub fn draw(canvas: &mut Canvas, n: &LaidOut) {
    let prev_clip = canvas.clip;
    canvas.clip = n.clip;
    let backdrop = if !canvas.is_capturing() && crate::effects::has_local_effects(n) {
        Some(canvas.pixels.clone())
    } else {
        None
    };
    draw_shadow(canvas, n);
    draw_box(canvas, n);
    draw_control(canvas, n);
    draw_text(canvas, n);
    if let Some(backdrop) = backdrop {
        crate::effects::apply_node(canvas, n, &backdrop);
    }
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
                        draw_glyphs(canvas, label, n.rect.x + c * cw + 2, ty, 1, on);
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
                        );
                        day += 1;
                    }
                }
            }
        }
        (Some(Control::RichText { text }), _) => {
            let scale = n.text_scale.clamp(1, 8);
            let line_h = font::GLYPH_H * scale + 2;
            let max_chars =
                (n.rect.w.saturating_sub(4) / font::advance(scale).max(1)).max(1) as usize;
            let ink = with_alpha(n.text_color.unwrap_or(INK), alpha);
            for (i, wrapped) in wrap_text(text, max_chars).iter().enumerate() {
                let y = n.rect.y + 2 + i as i64 * line_h;
                if y >= n.rect.y + n.rect.h {
                    break;
                }
                draw_glyphs(canvas, wrapped, n.rect.x + 2, y, scale, ink);
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
            canvas.fill_rounded(body, bubble, n.radius.min(body.h / 2).max(0));
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
            let tlen = (text.chars().count() as i64) * font::advance(scale);
            let tx = body.x + ((body.w - tlen) / 2).max(2);
            let ty = body.y + ((body.h - font::GLYPH_H * scale) / 2).max(0);
            draw_glyphs(canvas, text, tx, ty, scale, on);
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
