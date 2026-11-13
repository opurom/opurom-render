//! R10 extended primitive library: 68 additional DISTINCT drawn widget kinds.
//!
//! Each [`Spec`] in [`SPECS`] names one primitive kind, its catalog family, a
//! documented prop schema, an a11y default role and one of 25 distinct
//! geometric motifs. [`draw`] renders the motif plus a unique identity band
//! derived from the kind's code. Codes are unique, so no two kinds coincide.
//!
//! Honest scope: one deterministic software drawing per kind, integer-only
//! (`i64`/`i128`), no wall clock, no randomness, no new crate. There is no real
//! font shaping, no image decoding, no platform-native widget and no frame-rate
//! claim. Props are validated by the compiler and parsed into `ir::Props`; the
//! geometry uses the `values` series when present and otherwise a deterministic
//! integer sequence seeded by the kind code.

use crate::ir::Role;
use crate::raster::Canvas;
use crate::{Rect, Rgba};

/// Number of R10 extended primitive kinds.
pub const COUNT: usize = 68;

const INK: Rgba = Rgba::rgb(0x1b, 0x1f, 0x27);
const LINE: Rgba = Rgba::rgb(0x90, 0x98, 0xa8);
const MID: Rgba = Rgba::rgb(0x4b, 0x55, 0x68);
const LIGHT: Rgba = Rgba::rgb(0xff, 0xff, 0xff);
const UNIT: i64 = 1024;

/// A geometric construction shared by a family of kinds; the kind code sets
/// its parameters, so two kinds using the same motif still draw differently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motif {
    Bars,
    HBars,
    LineChart,
    AreaChart,
    Pie,
    Donut,
    Scatter,
    Stacked,
    Rings,
    Spokes,
    GridCells,
    Heat,
    Steps,
    Wave,
    Ruler,
    Crosshair,
    Chevrons,
    Gauge,
    Blocks,
    Brackets,
    Spectrum,
    Ladder,
    DotMatrix,
    Checker,
    Pyramid,
}

/// Static description of one extended kind.
#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub name: &'static str,
    pub family: &'static str,
    pub role: Role,
    pub props: &'static str,
    pub motif: Motif,
}

mod specs;
pub use specs::SPECS;

/// Resolve a kind name to its code. [`crate::ir::Kind::parse`] uses this.
pub fn parse(name: &str) -> Option<u16> {
    SPECS.iter().position(|s| s.name == name).map(|i| i as u16)
}

pub fn name_of(code: u16) -> &'static str {
    SPECS.get(code as usize).map_or("Other", |s| s.name)
}

pub fn role_of(code: u16) -> Role {
    SPECS.get(code as usize).map_or(Role::None, |s| s.role)
}

pub fn prop_schema(code: u16) -> &'static str {
    SPECS.get(code as usize).map_or("", |s| s.props)
}

pub fn family_of(code: u16) -> &'static str {
    SPECS.get(code as usize).map_or("", |s| s.family)
}

fn with_alpha(c: Rgba, alpha: i64) -> Rgba {
    Rgba {
        r: c.r,
        g: c.g,
        b: c.b,
        a: ((alpha.clamp(0, UNIT) * 255) / UNIT) as u8,
    }
}

/// Integer cos/sin table in 22.5-degree steps, scaled by 1000.
const TRIG: [(i64, i64); 16] = [
    (1000, 0),
    (924, 383),
    (707, 707),
    (383, 924),
    (0, 1000),
    (-383, 924),
    (-707, 707),
    (-924, 383),
    (-1000, 0),
    (-924, -383),
    (-707, -707),
    (-383, -924),
    (0, -1000),
    (383, -924),
    (707, -707),
    (924, -383),
];

fn trig(i: usize) -> (i64, i64) {
    TRIG[i % 16]
}

/// Integer Bresenham stroke with a square pen (no floats).
fn stroke(canvas: &mut Canvas, x0: i64, y0: i64, x1: i64, y1: i64, col: Rgba, width: i64) {
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
                canvas.set(x + ox, y + oy, col);
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

struct Paint<'a, 'b> {
    c: &'a mut Canvas,
    values: &'b [i64],
    r: Rect,
    accent: Rgba,
    ink: Rgba,
    line: Rgba,
    mid: Rgba,
    light: Rgba,
    code: i64,
}

impl Paint<'_, '_> {
    /// Deterministic 3..99 magnitude: the `values` series when present, else an
    /// integer LCG seeded by the kind code. No floats, no randomness.
    fn series(&self, i: usize) -> i64 {
        if let Some(&v) = self.values.get(i) {
            return v.clamp(0, 100);
        }
        let seed = self.code as i128 * 1_103_515_245 + 12_345 + i as i128 * 2_654_435_761;
        (seed.rem_euclid(97) + 3) as i64
    }

    fn fill(&mut self, x: i64, y: i64, w: i64, h: i64, col: Rgba) {
        self.c.fill_rect(Rect { x, y, w, h }, col);
    }

    fn dot(&mut self, x: i64, y: i64, d: i64, col: Rgba) {
        let d = d.max(1);
        self.fill(x - d / 2, y - d / 2, d, d, col);
    }

    fn circle(&mut self, cx: i64, cy: i64, rad: i64, col: Rgba) {
        if rad > 0 {
            self.c.fill_circle(
                Rect {
                    x: cx - rad,
                    y: cy - rad,
                    w: rad * 2,
                    h: rad * 2,
                },
                col,
            );
        }
    }

    fn ring(&mut self, cx: i64, cy: i64, rad: i64, w: i64, col: Rgba) {
        if rad > 0 {
            self.c.stroke_circle(
                Rect {
                    x: cx - rad,
                    y: cy - rad,
                    w: rad * 2,
                    h: rad * 2,
                },
                col,
                w.max(1),
            );
        }
    }

    fn seg(&mut self, x0: i64, y0: i64, x1: i64, y1: i64, col: Rgba, t: i64) {
        stroke(self.c, x0, y0, x1, y1, col, t);
    }

    fn frame(&mut self, col: Rgba, t: i64) {
        let (x, y, w, h) = (self.r.x, self.r.y, self.r.w, self.r.h);
        self.fill(x, y, w, t, col);
        self.fill(x, y + h - t, w, t, col);
        self.fill(x, y, t, h, col);
        self.fill(x + w - t, y, t, h, col);
    }
}

/// Draw one extended kind. `accent` is already alpha-applied by the caller.
pub fn draw(canvas: &mut Canvas, code: u16, values: &[i64], rect: Rect, accent: Rgba, alpha: i64) {
    let Some(spec) = SPECS.get(code as usize) else {
        return;
    };
    let r = crate::raster::inset(rect, 3);
    if r.w < 12 || r.h < 12 {
        return;
    }
    let mut p = Paint {
        c: canvas,
        values,
        r,
        accent,
        ink: with_alpha(INK, alpha),
        line: with_alpha(LINE, alpha),
        mid: with_alpha(MID, alpha),
        light: with_alpha(LIGHT, alpha),
        code: code as i64,
    };
    draw_motif(&mut p, spec.motif);
    signature(&mut p);
}

/// Unique identity band: seven cells encode the seven low bits of the kind
/// code. Codes 0..67 are unique, so at least one cell differs for any pair.
fn signature(p: &mut Paint) {
    let (x, y, h) = (p.r.x, p.r.y, p.r.h);
    let (ink, accent, line, light, code) = (p.ink, p.accent, p.line, p.light, p.code);
    let cw = 7i64;
    let ch = 5i64;
    let bx = x + 2;
    let by = y + h - ch - 2;
    p.fill(bx - 1, by - 1, 8 * cw + 1, ch + 2, ink);
    for i in 0..7i64 {
        let col = if (code >> i) & 1 == 1 { accent } else { line };
        p.fill(bx + i * cw, by, cw - 1, ch, col);
    }
    p.fill(bx + 7 * cw, by, cw - 1, ch, light);
}

/// Dispatch to one of 25 genuinely distinct integer drawing routines.
fn draw_motif(p: &mut Paint, m: Motif) {
    let (x, y, w, h) = (p.r.x, p.r.y, p.r.w, p.r.h);
    let (accent, ink, line, mid, light) = (p.accent, p.ink, p.line, p.mid, p.light);
    let code = p.code;
    let cx = x + w / 2;
    let cy = y + h / 2;
    let mut s = [0i64; 64];
    for (i, slot) in s.iter_mut().enumerate() {
        *slot = p.series(i);
    }

    match m {
        Motif::Bars => {
            let n = 4 + (code % 6) as usize;
            let bw = w / n as i64;
            p.fill(x, y + h - 1, w, 1, ink);
            for i in 0..n {
                let bh = (h - 2) * s[i] / 100;
                let col = if i % 2 == 0 { accent } else { mid };
                p.fill(
                    x + i as i64 * bw + 1,
                    y + h - 1 - bh,
                    (bw - 2).max(1),
                    bh,
                    col,
                );
            }
        }
        Motif::HBars => {
            let n = 3 + (code % 5) as usize;
            let rh = h / n as i64;
            for i in 0..n {
                let bw = (w - 2) * s[i] / 100;
                let col = if i % 2 == 0 { accent } else { line };
                p.fill(
                    x + 1,
                    y + i as i64 * rh + 1,
                    bw.max(1),
                    (rh - 2).max(1),
                    col,
                );
            }
        }
        Motif::LineChart => {
            let n = 5 + (code % 5) as usize;
            let step = w / (n as i64 - 1);
            p.fill(x, y + h - 1, w, 1, line);
            let mut prev = (x, y + h - 1 - (h - 2) * s[0] / 100);
            for i in 1..n {
                let cur = (x + i as i64 * step, y + h - 1 - (h - 2) * s[i] / 100);
                p.seg(prev.0, prev.1, cur.0, cur.1, accent, 2);
                p.dot(cur.0, cur.1, 4, ink);
                prev = cur;
            }
        }
        Motif::AreaChart => {
            let n = 6 + (code % 4) as usize;
            let bw = w / n as i64;
            for i in 0..n {
                let bh = (h - 2) * s[i] / 100;
                let col = if i % 2 == 0 { accent } else { mid };
                p.fill(x + i as i64 * bw, y + h - 1 - bh, bw.max(1), bh, col);
                p.fill(x + i as i64 * bw, y + h - 1 - bh, bw.max(1), 1, ink);
            }
        }
        Motif::Pie => {
            let rad = (w.min(h) / 2 - 2).max(2);
            p.circle(cx, cy, rad, mid);
            let spokes = 3 + code % 6;
            for k in 0..spokes {
                let (dx, dy) = trig((k * 16 / spokes) as usize);
                p.seg(cx, cy, cx + dx * rad / 1000, cy + dy * rad / 1000, light, 1);
                p.dot(cx + dx * rad / 1400, cy + dy * rad / 1400, 3, accent);
            }
            p.dot(cx, cy, 5, ink);
        }
        Motif::Donut => {
            let rad = (w.min(h) / 2 - 2).max(3);
            let thick = (rad / 3).max(2);
            p.ring(cx, cy, rad, thick, accent);
            let k = 3 + code % 5;
            for j in 0..k {
                let (dx, dy) = trig((j * 16 / k) as usize);
                p.seg(
                    cx + dx * (rad - thick) / 1000,
                    cy + dy * (rad - thick) / 1000,
                    cx + dx * (rad + thick) / 1000,
                    cy + dy * (rad + thick) / 1000,
                    line,
                    1,
                );
            }
            p.ring(cx, cy, rad / 3, 1, ink);
            p.dot(cx, cy, 4, light);
        }
        Motif::Scatter => {
            let n = 8 + code % 8;
            for i in 0..n as usize {
                let px = x + 4 + (w - 8) * s[i] / 100;
                let py = y + 4 + (h - 8) * s[(i + 5) % 64] / 100;
                let d = 3 + s[(i + 9) % 64] % 6;
                p.dot(px, py, d, if i % 2 == 0 { accent } else { ink });
            }
            p.frame(line, 1);
        }
        Motif::Stacked => {
            let n = 3 + (code % 5) as usize;
            let total: i64 = (0..n).map(|i| s[i].max(1)).sum();
            let top = y + h / 3;
            let bh = h / 3;
            let cols = [accent, mid, line, ink];
            let mut cursor = x;
            for i in 0..n {
                let segw = w * s[i].max(1) / total.max(1);
                p.fill(cursor, top, segw.max(1), bh, cols[i % 4]);
                cursor += segw;
            }
            p.fill(x, y + h - 2, w, 1, ink);
        }
        Motif::Rings => {
            let k = 2 + code % 4;
            let rad = (w.min(h) / 2 - 2).max(3);
            for j in 0..k {
                let rr = rad * (k - j) / k;
                let col = if j % 2 == 0 { accent } else { line };
                p.ring(cx, cy, rr, 2, col);
            }
            p.dot(cx, cy, 5, ink);
        }
        Motif::Spokes => {
            let k = 5 + code % 7;
            let rad = (w.min(h) / 2 - 2).max(3);
            let phase = code % 4;
            for j in 0..k {
                let (dx, dy) = trig((j * 16 / k + phase) as usize);
                let len = rad * (4 + s[j as usize] % 6) / 10;
                let ex = cx + dx * len / 1000;
                let ey = cy + dy * len / 1000;
                p.seg(cx, cy, ex, ey, if j % 2 == 0 { accent } else { ink }, 1);
                p.dot(ex, ey, 3, mid);
            }
            p.dot(cx, cy, 6, light);
            p.frame(line, 1);
        }
        Motif::GridCells => {
            let cols = 3 + code % 4;
            let rows = 2 + code % 3;
            let cw = w / cols;
            let ch = h / rows;
            for rr in 0..rows {
                for cc in 0..cols {
                    let idx = (rr * cols + cc) as usize;
                    if s[idx] > 48 {
                        p.fill(
                            x + cc * cw + 1,
                            y + rr * ch + 1,
                            (cw - 2).max(1),
                            (ch - 2).max(1),
                            accent,
                        );
                    }
                    p.fill(x + cc * cw, y + rr * ch, cw, 1, line);
                    p.fill(x + cc * cw, y + rr * ch, 1, ch, line);
                }
            }
            p.frame(ink, 1);
        }
        Motif::Heat => {
            let cols = 4 + code % 3;
            let rows = 3 + code % 3;
            let cw = w / cols;
            let ch = h / rows;
            let shades = [light, line, mid, accent];
            for rr in 0..rows {
                for cc in 0..cols {
                    let idx = (rr * cols + cc) as usize;
                    let col = shades[(s[idx] as usize / 25) % 4];
                    p.fill(x + cc * cw, y + rr * ch, cw.max(1), ch.max(1), col);
                }
            }
            p.frame(ink, 1);
        }
        Motif::Steps => {
            let k = 3 + (code % 5) as usize;
            let bw = w / k as i64;
            for i in 0..k {
                let bh = (h - 2) * (i + 1) as i64 / k as i64;
                p.fill(
                    x + i as i64 * bw,
                    y + h - 1 - bh,
                    (bw - 1).max(1),
                    bh,
                    if i % 2 == 0 { accent } else { mid },
                );
            }
            p.fill(x, y + h - 1, w, 1, ink);
        }
        Motif::Wave => {
            let amp = (h / 3).max(2) * (4 + code % 5) / 8;
            let period = 8 + code % 10;
            p.fill(x, cy, w, 1, line);
            let mut prev = (x, cy);
            for step in 1..=w {
                let m = (step * period).rem_euclid(4000);
                let tri = if m < 1000 {
                    m
                } else if m < 3000 {
                    2000 - m
                } else {
                    m - 4000
                };
                let py = cy - tri * amp / 1000;
                p.seg(prev.0, prev.1, x + step, py, accent, 2);
                prev = (x + step, py);
            }
            p.dot(x, cy, 4, ink);
            p.dot(x + w, cy, 4, ink);
        }
        Motif::Ruler => {
            let base = y + h - 3;
            p.fill(x, base, w, 2, ink);
            let k = 6 + code % 6;
            let step = w / k;
            for j in 0..k {
                let th = (h - 6) * s[j as usize] / 100;
                let col = if j % 2 == 0 { accent } else { line };
                p.fill(x + j * step, base - th, 2, th.max(1), col);
            }
            p.fill(x, y + 2, w, 1, light);
        }
        Motif::Crosshair => {
            p.fill(x, cy, w, 1, accent);
            p.fill(cx, y, 1, h, accent);
            p.ring(cx, cy, (h / 4).max(3), 2, ink);
            let arm = (w / 8).max(4);
            p.fill(x, y, arm, 2, mid);
            p.fill(x, y, 2, arm, mid);
            p.fill(x + w - arm, y, arm, 2, mid);
            p.fill(x + w - 2, y, 2, arm, mid);
            p.fill(x, y + h - 2, arm, 2, mid);
            p.fill(x, y + h - arm, 2, arm, mid);
            p.fill(x + w - arm, y + h - 2, arm, 2, mid);
            p.fill(x + w - 2, y + h - arm, 2, arm, mid);
            p.dot(cx, cy, 4, light);
        }
        Motif::Chevrons => {
            let k = 2 + code % 3;
            for j in 0..k {
                let xx = x + j * (w / (k + 1));
                let hh = h / 2;
                let col = if j % 2 == 0 { accent } else { mid };
                p.seg(xx, cy - hh / 2, xx + hh / 2, cy, col, 2);
                p.seg(xx + hh / 2, cy, xx, cy + hh / 2, col, 2);
            }
            p.frame(line, 1);
        }
        Motif::Gauge => {
            let rad = (w.min(h) / 2 - 3).max(3);
            p.ring(cx, cy, rad, 2, line);
            let ticks = 7 + code % 5;
            for j in 0..ticks {
                let (dx, dy) = trig((j * 16 / ticks) as usize);
                p.seg(
                    cx + dx * (rad - 4) / 1000,
                    cy + dy * (rad - 4) / 1000,
                    cx + dx * rad / 1000,
                    cy + dy * rad / 1000,
                    ink,
                    1,
                );
            }
            let (ndx, ndy) = trig(((code * 3) % 16) as usize);
            p.seg(
                cx,
                cy,
                cx + ndx * (rad - 6) / 1000,
                cy + ndy * (rad - 6) / 1000,
                accent,
                3,
            );
            p.dot(cx, cy, 6, mid);
        }
        Motif::Blocks => {
            let mut r0 = (x, y, w, h);
            let cols = [accent, mid, line, ink];
            for i in 0..(4 + code % 4) as usize {
                let ratio = s[i].clamp(20, 80);
                let (a, b) = if i % 2 == 0 {
                    let cut = r0.2 * ratio / 100;
                    (
                        (r0.0, r0.1, cut, r0.3),
                        (r0.0 + cut, r0.1, r0.2 - cut, r0.3),
                    )
                } else {
                    let cut = r0.3 * ratio / 100;
                    (
                        (r0.0, r0.1, r0.2, cut),
                        (r0.0, r0.1 + cut, r0.2, r0.3 - cut),
                    )
                };
                p.fill(
                    a.0 + 1,
                    a.1 + 1,
                    (a.2 - 2).max(1),
                    (a.3 - 2).max(1),
                    cols[i % 4],
                );
                r0 = b;
            }
            p.fill(
                r0.0 + 1,
                r0.1 + 1,
                (r0.2 - 2).max(1),
                (r0.3 - 2).max(1),
                light,
            );
        }
        Motif::Brackets => {
            let arm = (w.min(h) / 4).max(4);
            let t = 2;
            p.fill(x, y, arm, t, accent);
            p.fill(x, y, t, arm, accent);
            p.fill(x + w - arm, y, arm, t, accent);
            p.fill(x + w - t, y, t, arm, accent);
            p.fill(x, y + h - t, arm, t, accent);
            p.fill(x, y + h - arm, t, arm, accent);
            p.fill(x + w - arm, y + h - t, arm, t, accent);
            p.fill(x + w - t, y + h - arm, t, arm, accent);
            p.ring(cx, cy, (h / 6).max(3), 2, mid);
            p.dot(cx, cy, 4, ink);
            p.fill(x + w / 4, y + h - 4, w / 2, 1, line);
        }
        Motif::Spectrum => {
            let k = 5 + (code % 5) as usize;
            let bw = w / k as i64;
            for i in 0..k {
                let bh = (h / 2 - 2).max(2) * s[i] / 100;
                let col = if i % 2 == 0 { accent } else { mid };
                p.fill(x + i as i64 * bw + 1, cy - bh, (bw - 2).max(1), bh, col);
                p.fill(x + i as i64 * bw + 1, cy, (bw - 2).max(1), bh, line);
            }
            p.fill(x, cy - 1, w, 2, ink);
        }
        Motif::Ladder => {
            let inset = (w / 6).max(4);
            p.fill(x + inset, y, 2, h, accent);
            p.fill(x + w - inset, y, 2, h, accent);
            let rungs = 3 + code % 6;
            for j in 0..rungs {
                let ry = y + (j + 1) * h / (rungs + 1);
                let col = if j % 2 == 0 { mid } else { line };
                p.fill(x + inset, ry, (w - 2 * inset).max(1), 2, col);
                p.dot(
                    x + inset + (w - 2 * inset) * s[j as usize] / 100,
                    ry,
                    4,
                    ink,
                );
            }
        }
        Motif::DotMatrix => {
            let n = 6 + (code % 3) as i64;
            let cw = w / n;
            let ch = h / n;
            for rr in 0..n {
                for cc in 0..n {
                    let idx = (rr * n + cc) as usize;
                    let on = (s[idx] + code) % 3 != 0;
                    let col = if on { accent } else { line };
                    p.fill(
                        x + cc * cw + 1,
                        y + rr * ch + 1,
                        (cw - 2).max(1),
                        (ch - 2).max(1),
                        col,
                    );
                }
            }
            p.frame(ink, 1);
        }
        Motif::Checker => {
            let cell = 5 + code % 8;
            let cols = (w / cell).max(1);
            let rows = (h / cell).max(1);
            for rr in 0..rows {
                for cc in 0..cols {
                    let on = (rr + cc + code) % 2 == 0;
                    let col = if on { accent } else { light };
                    p.fill(x + cc * cell, y + rr * cell, cell, cell, col);
                }
            }
            p.frame(ink, 1);
        }
        Motif::Pyramid => {
            let k = 3 + (code % 4) as i64;
            let rh = h / k;
            for j in 0..k {
                let shrink = (j + 1) * w / (2 * k + 2);
                let col = if j % 2 == 0 { accent } else { mid };
                p.fill(
                    x + shrink,
                    y + j * rh,
                    (w - 2 * shrink).max(1),
                    (rh - 1).max(1),
                    col,
                );
            }
            p.frame(line, 1);
        }
    }
}
