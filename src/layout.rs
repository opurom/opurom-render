//! Pure, deterministic adaptive layout. All arithmetic is integer i64/i128:
//! no floats, no time, no randomness, no host queries.

use crate::{font, Align, Ir, Kind, Node, Props, Rgba, StateView, Theme, ThemeMode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Breakpoint {
    Compact,
    Medium,
    Expanded,
}

impl Breakpoint {
    /// Logical-pixel breakpoints: compact <600, medium <900, expanded >=900.
    pub fn from_width(width: u32) -> Breakpoint {
        if width < 600 {
            Breakpoint::Compact
        } else if width < 900 {
            Breakpoint::Medium
        } else {
            Breakpoint::Expanded
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Vertical,
    Horizontal,
}

/// One laid-out node; the raster draws these in pre-order with no further decisions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaidOut {
    pub id: u32,
    pub kind: Kind,
    pub rect: Rect,
    pub direction: Direction,
    pub radius: i64,
    pub fill: Option<Rgba>,
    pub border: Option<Rgba>,
    pub border_w: i64,
    pub text: Option<String>,
    pub text_color: Option<Rgba>,
    pub text_scale: i64,
    pub text_align: Align,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub bg: Rgba,
    pub fg: Rgba,
    pub surface: Rgba,
    pub border: Rgba,
    pub accent: Rgba,
    pub on_accent: Rgba,
    pub radius: i64,
}

impl Palette {
    pub fn from_theme(theme: &Theme) -> Palette {
        // `system` resolves to light: querying host appearance would break pixel determinism.
        let (bg, fg, surface, border) = match theme.mode {
            ThemeMode::Dark => (
                Rgba::rgb(0x11, 0x14, 0x1b),
                Rgba::rgb(0xee, 0xf1, 0xf7),
                Rgba::rgb(0x1e, 0x23, 0x2e),
                Rgba::rgb(0x38, 0x40, 0x50),
            ),
            ThemeMode::Light | ThemeMode::System => (
                Rgba::rgb(0xf7, 0xf8, 0xfa),
                Rgba::rgb(0x1b, 0x1f, 0x27),
                Rgba::rgb(0xff, 0xff, 0xff),
                Rgba::rgb(0xd8, 0xdc, 0xe4),
            ),
        };
        Palette {
            bg,
            fg,
            surface,
            border,
            accent: theme.accent,
            on_accent: Rgba::rgb(0xff, 0xff, 0xff),
            radius: theme.radius,
        }
    }
}

/// Row collapses into a vertical stack in the compact breakpoint.
fn direction(kind: Kind, bp: Breakpoint) -> Direction {
    match kind {
        Kind::Row if bp == Breakpoint::Compact => Direction::Vertical,
        Kind::Row => Direction::Horizontal,
        _ => Direction::Vertical,
    }
}

fn font_scale(p: &Props, default_size: i64) -> i64 {
    ((p.size.unwrap_or(default_size) + 3) / 7).clamp(1, 8)
}

fn text_width(text: &str, scale: i64) -> i64 {
    (text.chars().count() as i64).saturating_mul(font::advance(scale))
}

fn node_text(n: &Node, state: &StateView) -> String {
    match n.kind {
        Kind::Text => state.interpolate(n.props.value.as_deref().unwrap_or("")),
        Kind::Button => state.interpolate(n.props.label.as_deref().unwrap_or("")),
        Kind::Input => match n.props.bind.as_deref().and_then(|b| state.get(b)) {
            Some(v) => v.as_text(),
            None => state.interpolate(n.props.placeholder.as_deref().unwrap_or("")),
        },
        _ => String::new(),
    }
}

fn image_color(src: &str) -> Rgba {
    // No resource I/O in the software raster: a stable hash of `src` picks the placeholder.
    let h = crate::digest(src.as_bytes());
    Rgba::rgb(
        0x40 + ((h >> 16) as u8 >> 2),
        0x40 + ((h >> 8) as u8 >> 2),
        0x40 + (h as u8 >> 2),
    )
}

fn container_extent(n: &Node, bp: Breakpoint, state: &StateView, want_w: bool) -> i64 {
    let dir = direction(n.kind, bp);
    let pad = n.props.padding.saturating_mul(2);
    let gap = n
        .props
        .gap
        .saturating_mul(n.children.len().saturating_sub(1) as i64);
    let along_main = if want_w {
        dir == Direction::Horizontal
    } else {
        dir == Direction::Vertical
    };
    let inner = if along_main {
        n.children
            .iter()
            .fold(0i64, |acc, c| {
                let v = if want_w {
                    natural_w(c, bp, state)
                } else {
                    natural_h(c, bp, state)
                };
                acc.saturating_add(v)
            })
            .saturating_add(gap)
    } else {
        n.children
            .iter()
            .map(|c| {
                if want_w {
                    natural_w(c, bp, state)
                } else {
                    natural_h(c, bp, state)
                }
            })
            .max()
            .unwrap_or(0)
    };
    pad.saturating_add(inner)
}

fn natural_w(n: &Node, bp: Breakpoint, state: &StateView) -> i64 {
    if let Some(w) = n.props.width {
        return w;
    }
    let scale = font_scale(&n.props, 16);
    match n.kind {
        Kind::Text => text_width(&node_text(n, state), scale),
        Kind::Button => text_width(&node_text(n, state), scale).saturating_add(24),
        Kind::Input => 120,
        Kind::Image => 64,
        Kind::Spacer => 8,
        _ => container_extent(n, bp, state, true),
    }
}

fn natural_h(n: &Node, bp: Breakpoint, state: &StateView) -> i64 {
    if let Some(h) = n.props.height {
        return h;
    }
    let scale = font_scale(&n.props, 16);
    match n.kind {
        Kind::Text => font::GLYPH_H.saturating_mul(scale),
        Kind::Button => font::GLYPH_H
            .saturating_mul(scale)
            .saturating_add(20),
        Kind::Input => font::GLYPH_H
            .saturating_mul(scale)
            .saturating_add(20),
        Kind::Image => 64,
        Kind::Spacer => 8,
        _ => container_extent(n, bp, state, false),
    }
}

fn inset(r: Rect, p: i64) -> Rect {
    let p = p.max(0);
    Rect {
        x: r.x + p,
        y: r.y + p,
        w: (r.w - p.saturating_mul(2)).max(0),
        h: (r.h - p.saturating_mul(2)).max(0),
    }
}

/// Compute the full pre-order layout for one viewport. Pure: same inputs, same output.
pub fn layout(ir: &Ir, state: &StateView, width: u32, height: u32) -> Vec<LaidOut> {
    let bp = Breakpoint::from_width(width);
    let pal = Palette::from_theme(&ir.theme);
    let mut out = Vec::new();
    let mut root = Rect {
        x: 0,
        y: 0,
        w: width as i64,
        h: height as i64,
    };
    if let Some(w) = ir.tree.props.width {
        root.w = root.w.min(w);
    }
    if let Some(h) = ir.tree.props.height {
        root.h = root.h.min(h);
    }
    place(&ir.tree, root, bp, &pal, state, Align::Start, &mut out);
    out
}

fn place(
    n: &Node,
    rect: Rect,
    bp: Breakpoint,
    pal: &Palette,
    state: &StateView,
    inherited_align: Align,
    out: &mut Vec<LaidOut>,
) {
    let dir = direction(n.kind, bp);
    let scale = font_scale(&n.props, 16);
    let radius = n
        .props
        .radius
        .unwrap_or(pal.radius)
        .max(0)
        .min(rect.w.min(rect.h).max(0) / 2);

    let (fill, border, border_w) = match n.kind {
        Kind::Column | Kind::Row | Kind::Container => (
            Some(n.props.color.unwrap_or(pal.surface)),
            Some(pal.border),
            1,
        ),
        Kind::Button => (Some(n.props.color.unwrap_or(pal.accent)), None, 0),
        Kind::Input => (
            Some(n.props.color.unwrap_or(pal.surface)),
            Some(pal.border),
            1,
        ),
        Kind::Image => (
            Some(
                n.props
                    .color
                    .unwrap_or_else(|| image_color(n.props.src.as_deref().unwrap_or(""))),
            ),
            Some(pal.border),
            1,
        ),
        Kind::Text | Kind::Spacer => (None, None, 0),
    };

    let (text, text_color, text_align) = match n.kind {
        Kind::Text => (
            Some(node_text(n, state)),
            Some(n.props.color.unwrap_or(pal.fg)),
            inherited_align,
        ),
        Kind::Button => (
            Some(node_text(n, state)),
            Some(pal.on_accent),
            Align::Center,
        ),
        Kind::Input => (Some(node_text(n, state)), Some(pal.fg), Align::Start),
        _ => (None, None, Align::Start),
    };

    out.push(LaidOut {
        id: n.id,
        kind: n.kind,
        rect,
        direction: dir,
        radius,
        fill,
        border,
        border_w,
        text,
        text_color,
        text_scale: scale,
        text_align,
    });

    if n.children.is_empty() || rect.w <= 0 || rect.h <= 0 {
        return;
    }

    let content = inset(rect, n.props.padding);
    let vertical = dir == Direction::Vertical;
    let content_main = if vertical { content.h } else { content.w };
    let cross_avail = if vertical { content.w } else { content.h };
    let count = n.children.len() as i64;
    let gap = n.props.gap.max(0);
    let gaps = gap.saturating_mul(count.saturating_sub(1));
    let avail = (content_main - gaps).max(0);

    let mut sizes: Vec<i64> = n
        .children
        .iter()
        .map(|c| {
            if vertical {
                natural_h(c, bp, state)
            } else {
                natural_w(c, bp, state)
            }
            .max(0)
        })
        .collect();

    // Fluid sizing: share the main axis in proportion to natural sizes, in i128
    // so the rounding is exact and identical on every host.
    let total: i128 = sizes.iter().map(|&s| s as i128).sum();
    if avail > 0 && count > 0 {
        if total > 0 {
            let mut assigned = 0i64;
            for s in sizes.iter_mut() {
                *s = ((*s as i128 * avail as i128) / total) as i64;
                assigned = assigned.saturating_add(*s);
            }
            // Integer remainder goes to the earliest children, deterministically.
            let mut rem = (avail - assigned).max(0);
            for s in sizes.iter_mut() {
                if rem == 0 {
                    break;
                }
                *s += 1;
                rem -= 1;
            }
        } else {
            let each = avail / count;
            let mut rem = avail % count;
            for s in sizes.iter_mut() {
                *s = each + if rem > 0 { 1 } else { 0 };
                rem -= 1;
            }
        }
    } else {
        sizes.iter_mut().for_each(|s| *s = 0);
    }

    let used = sizes
        .iter()
        .fold(0i64, |a, &s| a.saturating_add(s))
        .saturating_add(gaps);
    let main_offset = match n.props.align {
        Align::Start => 0,
        Align::Center => (content_main - used) / 2,
        Align::End => content_main - used,
    }
    .max(0);

    let mut cursor = 0i64;
    for (i, c) in n.children.iter().enumerate() {
        // Explicit width/height props win over the fluid share on their own axis.
        let (main_override, cross_override) = if vertical {
            (c.props.height, c.props.width)
        } else {
            (c.props.width, c.props.height)
        };
        let main = main_override
            .unwrap_or(sizes[i])
            .min(content_main)
            .max(0);
        let cross = cross_override
            .unwrap_or_else(|| {
                if c.kind.is_leaf() {
                    let natural = if vertical {
                        natural_w(c, bp, state)
                    } else {
                        natural_h(c, bp, state)
                    };
                    natural.min(cross_avail).max(0)
                } else {
                    cross_avail.max(0)
                }
            })
            .min(cross_avail)
            .max(0);
        let cross_offset = match n.props.align {
            Align::Start => 0,
            Align::Center => (cross_avail - cross) / 2,
            Align::End => cross_avail - cross,
        }
        .max(0);
        let child_rect = if vertical {
            Rect {
                x: content.x + cross_offset,
                y: content.y + main_offset + cursor,
                w: cross,
                h: main,
            }
        } else {
            Rect {
                x: content.x + main_offset + cursor,
                y: content.y + cross_offset,
                w: main,
                h: cross,
            }
        };
        place(c, child_rect, bp, pal, state, n.props.align, out);
        cursor = cursor.saturating_add(main.saturating_add(gap));
    }
}
