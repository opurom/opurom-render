//! `opurom-render`: deterministic IR v0.1 layout + RGBA8 software raster.
//!
//! Pipeline: `parse_ir` -> `StateView` -> `layout` -> RGBA frame -> dirty rects.
//! Same IR + same viewport + same state = byte-identical pixels on every host:
//! integer-only arithmetic, sorted state map, no wall-clock time, no unseeded
//! randomness, no host queries (`theme.mode == "system"` resolves to light).
//!
//! R2 adds theme tokens, canonical style effects, conditional `when` visibility,
//! sixteen new node kinds and a fixed-timestep animation clock. R3 adds optional
//! accessibility props and a data-only accessibility tree. R4 adds eight more
//! distinct primitives and a deterministic SVG 1.1 vector backend (a second
//! software backend, not a GPU backend). R5 adds deterministic page transitions,
//! gesture progress, fixed-seed particles and caller-supplied quality tiers.
//! R6 adds an exact display-list snapshot of the software raster stage. All of
//! them are opt-in: a v0.1 IR renders byte-identically to round 1.

pub mod a11y;
pub mod anim;
pub mod display;
pub mod font;
pub mod ir;
pub mod layout;
pub mod raster;
pub mod svg;

pub use a11y::{a11y_json, a11y_json_at, a11y_tree, a11y_tree_at, A11yNode};
pub use anim::{
    choose_quality, Animator, Curve, GestureAnim, Particle, ParticleSystem, Quality, Transition,
    TransitionKind, ONE, PARTICLE_FIELD, PARTICLE_MAX_LIFE, TRANSITION_MS,
};
pub use display::{Command, DisplayList, MAX_COMMANDS, MAX_DIMENSION};
pub use ir::{
    parse_ir, Action, Align, Contrast, Event, Ir, IrError, Kind, Node, Op, Props, Role, StateDecl,
    StateValue, StateView, Theme, ThemeMode, ThemeTokens, ValueType,
};
pub use layout::{
    layout, layout_at, Breakpoint, Control, Direction, Effects, LaidOut, Palette, Rect, SETTLED_MS,
};
pub use svg::{svg, svg_at};

/// One RGBA8 pixel. The software frame is always opaque (a = 255).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Rgba {
        Rgba { r, g, b, a: 255 }
    }

    /// Strict `#RRGGBB` parse; no named colours, no shorthand.
    pub fn from_hex(s: &str) -> Option<Rgba> {
        let b = s.as_bytes();
        if b.len() != 7 || b[0] != b'#' {
            return None;
        }
        let digit = |c: u8| -> Option<u8> {
            match c {
                b'0'..=b'9' => Some(c - b'0'),
                b'a'..=b'f' => Some(c - b'a' + 10),
                b'A'..=b'F' => Some(c - b'A' + 10),
                _ => None,
            }
        };
        let pair = |hi: u8, lo: u8| -> Option<u8> { Some(digit(hi)? * 16 + digit(lo)?) };
        Some(Rgba::rgb(
            pair(b[1], b[2])?,
            pair(b[3], b[4])?,
            pair(b[5], b[6])?,
        ))
    }
}

/// FNV-1a 64-bit over RGBA bytes, implemented inline so the CLI has no extra deps.
pub fn digest(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

pub fn digest_hex(bytes: &[u8]) -> String {
    format!("{:016x}", digest(bytes))
}

/// Axis-aligned changed region in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirtyRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub dirty: Vec<DirtyRect>,
}

/// Borrowed pixel view with the exact field layout of ADAPTER_CONTRACT.md's
/// `FrameView` (a local struct: the renderer has no cross-repo dependency).
#[derive(Clone, Copy, Debug)]
pub struct FrameView<'a> {
    pub width: u32,
    pub height: u32,
    pub pixels: &'a [u8],
}

impl Frame {
    pub fn view(&self) -> FrameView<'_> {
        FrameView {
            width: self.width,
            height: self.height,
            pixels: &self.pixels,
        }
    }
}

const MAX_PIXELS: u64 = 16_777_216; // 4096x4096 RGBA = 64 MiB
const MAX_DIRTY: usize = 64;

/// Keeps the previous RGBA buffer so successive renders can be diffed.
pub struct Renderer {
    width: u32,
    height: u32,
    prev: Option<Vec<u8>>,
}

impl Renderer {
    pub fn try_new(width: u32, height: u32) -> Result<Renderer, IrError> {
        if width == 0 || height == 0 || width as u64 * height as u64 > MAX_PIXELS {
            return Err(IrError(format!(
                "invalid viewport {width}x{height}: both dimensions must be non-zero and at most {MAX_PIXELS} pixels"
            )));
        }
        Ok(Renderer {
            width,
            height,
            prev: None,
        })
    }

    /// Convenience constructor; use `try_new` for untrusted CLI input.
    pub fn new(width: u32, height: u32) -> Renderer {
        Renderer::try_new(width, height).unwrap_or_else(|e| panic!("{e}"))
    }

    pub fn viewport(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn compute_layout(&self, ir: &Ir, state: &StateView) -> Vec<LaidOut> {
        crate::layout::layout(ir, state, self.width, self.height)
    }

    /// Layout at a given animation clock position (milliseconds).
    pub fn compute_layout_at(&self, ir: &Ir, state: &StateView, elapsed_ms: i64) -> Vec<LaidOut> {
        crate::layout::layout_at(ir, state, self.width, self.height, elapsed_ms)
    }

    /// Pure raster: no mutation, no hidden state.
    pub fn pixels(&self, ir: &Ir, state: &StateView) -> Vec<u8> {
        self.pixels_at(ir, state, SETTLED_MS)
    }

    /// Pure raster at a given animation clock position.
    pub fn pixels_at(&self, ir: &Ir, state: &StateView, elapsed_ms: i64) -> Vec<u8> {
        let pal = Palette::from_theme(&ir.theme);
        let laid = crate::layout::layout_at(ir, state, self.width, self.height, elapsed_ms);
        let mut canvas = raster::Canvas::new(self.width, self.height, pal.bg);
        for n in &laid {
            raster::draw(&mut canvas, n);
        }
        canvas.pixels
    }

    /// Pure display list at the settled animation clock position.
    pub fn display_list(&self, ir: &Ir, state: &StateView) -> DisplayList {
        self.display_list_at(ir, state, SETTLED_MS)
    }

    /// Record the exact raster commands, including CPU-decomposed glyph blocks,
    /// without changing the software frame or the incremental renderer state.
    pub fn display_list_at(&self, ir: &Ir, state: &StateView, elapsed_ms: i64) -> DisplayList {
        let pal = Palette::from_theme(&ir.theme);
        let laid = crate::layout::layout_at(ir, state, self.width, self.height, elapsed_ms);
        let mut canvas = raster::Canvas::capturing(self.width, self.height, pal.bg);
        for n in &laid {
            raster::draw(&mut canvas, n);
        }
        DisplayList {
            width: self.width,
            height: self.height,
            background: pal.bg,
            commands: canvas.into_commands(),
        }
    }

    /// Render and diff against the previous frame. Unchanged pixels => empty
    /// dirty list; changed pixels => a bounded, covering rect list.
    pub fn render(&mut self, ir: &Ir, state: &StateView) -> Frame {
        self.render_at(ir, state, SETTLED_MS)
    }

    /// Render at a given animation clock position and diff against the previous
    /// frame. The clock is the only time source; no wall clock is consulted.
    pub fn render_at(&mut self, ir: &Ir, state: &StateView, elapsed_ms: i64) -> Frame {
        let pixels = self.pixels_at(ir, state, elapsed_ms);
        let dirty = match self.prev.as_deref() {
            Some(prev) => diff_rects(prev, &pixels, self.width, self.height),
            None => vec![DirtyRect {
                x: 0,
                y: 0,
                w: self.width,
                h: self.height,
            }],
        };
        self.prev = Some(pixels.clone());
        Frame {
            width: self.width,
            height: self.height,
            pixels,
            dirty,
        }
    }
}

/// Diff rows and columns, then coalesce into a bounded list of rects that
/// covers every changed pixel. Fallback to one full-frame rect keeps the list
/// bounded for pathological frames.
fn diff_rects(prev: &[u8], next: &[u8], width: u32, height: u32) -> Vec<DirtyRect> {
    let w = width as usize;
    let h = height as usize;
    let full = || {
        vec![DirtyRect {
            x: 0,
            y: 0,
            w: width,
            h: height,
        }]
    };
    if prev.len() != next.len() || next.len() != w * h * 4 {
        return full();
    }

    let mut row_min = vec![u32::MAX; h];
    let mut row_max = vec![0u32; h];
    let mut row_any = vec![false; h];
    let mut col_min = vec![u32::MAX; w];
    let mut col_max = vec![0u32; w];
    let mut col_any = vec![false; w];

    for y in 0..h {
        let base = y * w * 4;
        for x in 0..w {
            let i = base + x * 4;
            if next[i..i + 4] != prev[i..i + 4] {
                let (xu, yu) = (x as u32, y as u32);
                if !row_any[y] {
                    row_any[y] = true;
                    row_min[y] = xu;
                    row_max[y] = xu;
                } else {
                    row_min[y] = row_min[y].min(xu);
                    row_max[y] = row_max[y].max(xu);
                }
                if !col_any[x] {
                    col_any[x] = true;
                    col_min[x] = yu;
                    col_max[x] = yu;
                } else {
                    col_min[x] = col_min[x].min(yu);
                    col_max[x] = col_max[x].max(yu);
                }
            }
        }
    }

    let mut rects = Vec::new();
    let mut y = 0usize;
    while y < h {
        if !row_any[y] {
            y += 1;
            continue;
        }
        let band_start = y;
        let mut xmin = usize::MAX;
        let mut xmax = 0usize;
        while y < h && row_any[y] {
            xmin = xmin.min(row_min[y] as usize);
            xmax = xmax.max(row_max[y] as usize);
            y += 1;
        }
        let band_end = y; // exclusive
        let bs = band_start as u32;
        let be = (band_end - 1) as u32;
        let band_h = (band_end - band_start) as u32;

        let active = |x: usize| col_any[x] && col_min[x] <= be && col_max[x] >= bs;
        let mut x = xmin;
        while x <= xmax {
            while x <= xmax && !active(x) {
                x += 1;
            }
            if x > xmax {
                break;
            }
            let run_start = x;
            let mut run_end = x;
            let mut gap = 0usize;
            while x <= xmax {
                if active(x) {
                    run_end = x;
                    gap = 0;
                } else {
                    gap += 1;
                    if gap > 8 {
                        break; // merge small gaps, split large empty stretches
                    }
                }
                x += 1;
            }
            rects.push(DirtyRect {
                x: run_start as u32,
                y: bs,
                w: (run_end - run_start + 1) as u32,
                h: band_h,
            });
        }
    }

    if rects.len() > MAX_DIRTY {
        return full();
    }
    rects
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv_vectors() {
        assert_eq!(digest(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(digest_hex(b"foobar"), "85944171f73967e8");
        assert_eq!(digest_hex(b"foobar"), digest_hex(b"foobar"));
        assert_ne!(digest(b"foobar"), digest(b"foobas"));
    }

    #[test]
    fn hex_color_parsing() {
        assert_eq!(Rgba::from_hex("#4F46E5"), Some(Rgba::rgb(0x4f, 0x46, 0xe5)));
        assert_eq!(Rgba::from_hex("#4f46e5"), Some(Rgba::rgb(0x4f, 0x46, 0xe5)));
        assert_eq!(Rgba::from_hex("#4F46E5FF"), None);
        assert_eq!(Rgba::from_hex("4F46E5"), None);
        assert_eq!(Rgba::from_hex("#GGGGGG"), None);
    }

    #[test]
    fn dirty_rects_cover_changes_and_vanish_when_equal() {
        let (w, h) = (16u32, 8u32);
        let a = vec![0u8; w as usize * h as usize * 4];
        let mut b = a.clone();
        for y in 2..5 {
            for x in 3..7 {
                let i = (y * w as usize + x) * 4;
                b[i] = 200;
            }
        }
        assert!(diff_rects(&a, &a, w, h).is_empty());
        let rects = diff_rects(&a, &b, w, h);
        assert!(!rects.is_empty());
        assert!(rects.len() <= MAX_DIRTY);
        for y in 0..h {
            for x in 0..w {
                let i = (y as usize * w as usize + x as usize) * 4;
                if a[i..i + 4] != b[i..i + 4] {
                    assert!(
                        rects
                            .iter()
                            .any(|r| { x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h }),
                        "changed pixel ({x},{y}) not covered by {rects:?}"
                    );
                }
            }
        }
    }
}
