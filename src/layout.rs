//! Pure, deterministic adaptive layout. All arithmetic is integer i64/i128:
//! no floats, no time, no randomness, no host queries.

use crate::anim::{self, Curve};
use crate::effects::VisualEffect;
use crate::ir::Contrast;
use crate::particles::Particles;
use crate::{font, Align, Ir, Kind, Node, Props, Rgba, StateView, Theme, ThemeMode};

/// Elapsed time at which every animation curve has settled.
pub const SETTLED_MS: i64 = 4000;

/// Fixed-point unit: 1024 == fully opaque, 0 == fully transparent.
pub const UNIT: i64 = 1024;

/// Viewport width tiers, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Breakpoint {
    Compact,
    Medium,
    Expanded,
    Wide,
    Ultra,
}

impl Breakpoint {
    /// Logical-pixel breakpoints: compact <600, medium <900, expanded <1200,
    /// wide <1600, ultra >=1600.
    pub fn from_width(width: u32) -> Breakpoint {
        if width < 600 {
            Breakpoint::Compact
        } else if width < 900 {
            Breakpoint::Medium
        } else if width < 1200 {
            Breakpoint::Expanded
        } else if width < 1600 {
            Breakpoint::Wide
        } else {
            Breakpoint::Ultra
        }
    }
}

/// Narrowest column the fluid grid allows, in logical pixels.
pub const MIN_COLUMN_WIDTH: i64 = 120;

/// Largest column count the fluid grid produces.
pub const MAX_GRID_COLUMNS: i64 = 12;

/// Fluid grid metrics for one container width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FluidGrid {
    /// Column count.
    pub columns: i64,
    /// Space between two columns, in logical pixels.
    pub gutter: i64,
    /// Space around the whole grid, in logical pixels.
    pub margin: i64,
}

/// Integer-only fluid grid: columns, gutter and margin from a container width.
///
/// The formula is `columns = clamp(width / 120, 1, 12)`,
/// `margin = clamp(width / 40, 8, 32)` and `gutter = clamp(width / 60, 8, 24)`.
///
/// Every term is an integer division of a non-negative width, so the metrics
/// are identical on every host: no float, no rounding rule, no clock.
pub fn fluid_grid(width: i64) -> FluidGrid {
    let width = width.max(0);
    FluidGrid {
        columns: (width / MIN_COLUMN_WIDTH).clamp(1, MAX_GRID_COLUMNS),
        margin: (width / 40).clamp(8, 32),
        gutter: (width / 60).clamp(8, 24),
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

/// Style effects. Every value is integer-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Effects {
    /// Two-stop linear gradient (top to bottom).
    pub gradient: Option<(Rgba, Rgba)>,
    /// Frosted fill (surface blended with accent).
    pub glass: bool,
    /// Soft shadow strength, 0..=8.
    pub shadow: i64,
    /// Alpha in fixed point (0..=1024).
    pub opacity: i64,
}

impl Default for Effects {
    fn default() -> Self {
        Effects {
            gradient: None,
            glass: false,
            shadow: 0,
            opacity: UNIT,
        }
    }
}

/// Widget state the raster needs for the interactive kinds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Control {
    Progress {
        value: i64,
    },
    Slider {
        value: i64,
        min: i64,
        max: i64,
    },
    Switch {
        on: bool,
    },
    Checkbox {
        checked: bool,
    },
    Radio {
        selected: bool,
    },
    Tabs {
        labels: Vec<String>,
        selected: usize,
    },
    Spinner,
    // Data primitives: each carries the data its raster routine needs.
    Table {
        columns: i64,
        rows: i64,
        headers: Vec<String>,
    },
    Chart {
        values: Vec<i64>,
    },
    Calendar {
        columns: i64,
        rows: i64,
    },
    RichText {
        text: String,
    },
    Player,
    Map,
    Menu {
        items: Vec<String>,
    },
    Tooltip {
        text: String,
    },
    // Extended primitive: its code and integer series drive the drawing.
    Extended {
        code: u16,
        values: Vec<i64>,
    },
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
    /// Built-in icon name for `Kind::Icon`, or `None` when it has none.
    pub icon_name: Option<String>,
    pub text_color: Option<Rgba>,
    pub text_scale: i64,
    pub text_align: Align,
    pub effects: Effects,
    /// Typed software effects, in IR array order.
    pub visual_effects: Vec<VisualEffect>,
    /// Opt-in deterministic particle field for this node's rect.
    pub particles: Option<Particles>,
    pub control: Option<Control>,
    /// Clip rectangle inherited from a `Scroll` viewport.
    pub clip: Option<Rect>,
    /// Absolute glyph origin, used when the text is not centred in the box.
    pub text_origin: Option<(i64, i64)>,
    /// Animation clock phase in milliseconds (drives the spinner).
    pub phase: i64,
    pub accent: Rgba,
    pub on_accent: Rgba,
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
    pub high_contrast: bool,
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
        // Tokens override the mode defaults; missing tokens fall back to them.
        Palette {
            bg,
            fg: theme.tokens.on_surface.unwrap_or(fg),
            surface: theme.tokens.surface.unwrap_or(surface),
            border,
            accent: theme.accent,
            on_accent: Rgba::rgb(0xff, 0xff, 0xff),
            radius: theme.radius,
            high_contrast: theme.tokens.contrast == Contrast::High,
        }
    }
}

/// Integer lerp between two colours; `t` is 0..=1024.
pub fn lerp_color(a: Rgba, b: Rgba, t: i64) -> Rgba {
    let t = t.clamp(0, UNIT);
    let mix = |x: u8, y: u8| -> u8 {
        (((y as i64 - x as i64) * t + (UNIT / 2)) / UNIT + x as i64).clamp(0, 255) as u8
    };
    Rgba::rgb(mix(a.r, b.r), mix(a.g, b.g), mix(a.b, b.b))
}

/// High-contrast ink: pure black or pure white from the integer luminance of
/// the background (no float, no host colour profile).
fn contrast_text(bg: Rgba) -> Rgba {
    let l = (bg.r as i64 * 299 + bg.g as i64 * 587 + bg.b as i64 * 114) / 1000;
    if l < 128 {
        Rgba::rgb(255, 255, 255)
    } else {
        Rgba::rgb(0, 0, 0)
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

/// Effective font size in integer logical pixels after `fontScale` (fixed point).
pub(crate) fn text_size(p: &Props, default_size: i64) -> i64 {
    let base = p.size.unwrap_or(default_size);
    match p.font_scale {
        Some(f) => ((base as i128 * f as i128 + 512) / 1024).clamp(0, i64::MAX as i128) as i64,
        None => base,
    }
}

/// Global `theme.fontScale` is the default for nodes without their own
/// `fontScale` prop: clone the tree once and fill the effective value in, so
/// text measurement, glyph sizing and the accessibility tree all agree.
pub(crate) fn with_theme_font_scale(root: &Node, global: Option<i64>) -> Node {
    fn fill(n: &mut Node, global: Option<i64>) {
        if n.props.font_scale.is_none() {
            n.props.font_scale = global;
        }
        for c in &mut n.children {
            fill(c, global);
        }
    }
    let mut n = root.clone();
    fill(&mut n, global);
    n
}

fn font_scale(p: &Props, default_size: i64) -> i64 {
    (text_size(p, default_size).saturating_add(3) / 7).clamp(1, 8)
}

fn text_width(text: &str, scale: i64) -> i64 {
    (text.chars().count() as i64).saturating_mul(font::advance(scale))
}

/// A node is visible only while every condition it declares holds.
pub fn visible(n: &Node, state: &StateView) -> bool {
    let holds = |name: &str, expected: bool| {
        state.get(name).and_then(|v| v.as_bool()).unwrap_or(false) == expected
    };
    // `unless` selects the opposite branch of a bool state.
    n.when.as_deref().map_or(true, |name| holds(name, true))
        && n.unless.as_deref().map_or(true, |name| holds(name, false))
}

/// Fallback text for a kind this renderer does not know: label, value, title, name.
fn fallback_text(n: &Node, state: &StateView) -> String {
    let candidates = [
        n.props.label.as_deref(),
        n.props.value.as_deref(),
        n.props.title.as_deref(),
        n.props.name.as_deref(),
    ];
    for c in candidates {
        if let Some(s) = c {
            if !s.is_empty() {
                return state.interpolate(s);
            }
        }
    }
    String::new()
}

pub(crate) fn node_text(n: &Node, state: &StateView) -> String {
    match n.kind {
        Kind::Text => state.interpolate(n.props.value.as_deref().unwrap_or("")),
        Kind::Button => state.interpolate(n.props.label.as_deref().unwrap_or("")),
        Kind::Input => match n.props.bind.as_deref().and_then(|b| state.get(b)) {
            Some(v) => v.as_text(),
            None => state.interpolate(n.props.placeholder.as_deref().unwrap_or("")),
        },
        Kind::Badge => state.interpolate(n.props.label.as_deref().unwrap_or("")),
        Kind::Avatar => state.interpolate(n.props.label.as_deref().unwrap_or("")),
        Kind::Dialog => state.interpolate(n.props.title.as_deref().unwrap_or("")),
        Kind::Icon => n
            .props
            .name
            .as_deref()
            .unwrap_or("")
            .chars()
            .next()
            .map(|c| c.to_string())
            .unwrap_or_default(),
        Kind::RichText => state.interpolate(n.props.value.as_deref().unwrap_or("")),
        Kind::Tooltip => state.interpolate(
            n.props
                .label
                .as_deref()
                .or(n.props.value.as_deref())
                .unwrap_or(""),
        ),
        Kind::Other => fallback_text(n, state),
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

/// A node behaves like a leaf for sizing when it has no children of its own.
fn leaf_like(n: &Node) -> bool {
    n.children.is_empty() && (n.kind.is_leaf() || n.kind == Kind::Other)
}

fn container_extent(n: &Node, bp: Breakpoint, state: &StateView, want_w: bool) -> i64 {
    let dir = direction(n.kind, bp);
    let pad = n.props.padding.saturating_mul(2);
    let visible_kids = n.children.iter().filter(|c| visible(c, state)).count() as i64;
    let gap = n.props.gap.saturating_mul(visible_kids.saturating_sub(1));
    let along_main = if want_w {
        dir == Direction::Horizontal
    } else {
        dir == Direction::Vertical
    };
    let inner = if along_main {
        n.children
            .iter()
            .filter(|c| visible(c, state))
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
            .filter(|c| visible(c, state))
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

fn tabs_width(n: &Node, scale: i64) -> i64 {
    let labels: i64 = n.props.labels.iter().map(|l| text_width(l, scale)).sum();
    let gaps = 8i64.saturating_mul(n.props.labels.len().saturating_sub(1) as i64);
    labels.saturating_add(gaps)
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
        Kind::Divider => 0, // spans the parent width
        Kind::Icon => 16,
        Kind::Badge => text_width(&node_text(n, state), scale).saturating_add(12),
        Kind::Progress => 120,
        Kind::Switch => 36,
        Kind::Slider => 120,
        Kind::Checkbox => 16,
        Kind::Radio => 16,
        Kind::Tabs => tabs_width(n, scale),
        Kind::Avatar => 24,
        Kind::Spinner => 16,
        Kind::Table | Kind::Chart | Kind::Map => 160,
        Kind::Calendar => 140,
        Kind::RichText | Kind::Player => 160,
        Kind::Menu | Kind::Tooltip => 120,
        Kind::Extended(_) => 160,
        Kind::Other => {
            if leaf_like(n) {
                text_width(&node_text(n, state), scale)
            } else {
                container_extent(n, bp, state, true)
            }
        }
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
        Kind::Button => font::GLYPH_H.saturating_mul(scale).saturating_add(20),
        Kind::Input => font::GLYPH_H.saturating_mul(scale).saturating_add(20),
        Kind::Image => 64,
        Kind::Spacer => 8,
        Kind::Divider => 1,
        Kind::Icon => 16,
        Kind::Badge => font::GLYPH_H.saturating_mul(scale).saturating_add(8),
        Kind::Progress => 8,
        Kind::Switch => 20,
        Kind::Slider => 8,
        Kind::Checkbox => 16,
        Kind::Radio => 16,
        Kind::Tabs => font::GLYPH_H.saturating_mul(scale).saturating_add(8),
        Kind::Avatar => 24,
        Kind::Spinner => 16,
        Kind::Table | Kind::Chart | Kind::Map => 100,
        Kind::Calendar => 120,
        Kind::RichText | Kind::Menu => 100,
        Kind::Player | Kind::Tooltip => 40,
        Kind::Extended(_) => 100,
        Kind::Other => {
            if leaf_like(n) {
                font::GLYPH_H.saturating_mul(scale)
            } else {
                container_extent(n, bp, state, false)
            }
        }
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
    layout_at(ir, state, width, height, SETTLED_MS)
}

/// Layout at a given animation clock position.
pub fn layout_at(
    ir: &Ir,
    state: &StateView,
    width: u32,
    height: u32,
    elapsed_ms: i64,
) -> Vec<LaidOut> {
    let bp = Breakpoint::from_width(width);
    let pal = Palette::from_theme(&ir.theme);
    let tree = with_theme_font_scale(&ir.tree, ir.theme.font_scale);
    let mut out = Vec::new();
    let mut root = Rect {
        x: 0,
        y: 0,
        w: width as i64,
        h: height as i64,
    };
    if let Some(w) = tree.props.width {
        root.w = root.w.min(w);
    }
    if let Some(h) = tree.props.height {
        root.h = root.h.min(h);
    }
    place(
        &tree,
        root,
        bp,
        &pal,
        state,
        Align::Start,
        elapsed_ms,
        None,
        &mut out,
    );
    out
}

/// Entry animation: the box grows from its centre and fades in, both in fixed point.
fn entry_transform(n: &Node, rect: Rect, elapsed_ms: i64) -> (Rect, i64) {
    if n.props.animate == Curve::None {
        return (rect, UNIT);
    }
    let p = anim::progress(n.props.animate, elapsed_ms).clamp(0, UNIT);
    let w = rect.w.saturating_mul(p) / UNIT;
    let h = rect.h.saturating_mul(p) / UNIT;
    (
        Rect {
            x: rect.x + (rect.w - w) / 2,
            y: rect.y + (rect.h - h) / 2,
            w,
            h,
        },
        p,
    )
}

fn control_of(n: &Node, state: &StateView) -> Option<Control> {
    let bound = |name: Option<&String>| name.and_then(|b| state.get(b));
    match n.kind {
        Kind::Progress => Some(Control::Progress {
            value: n.props.value_frac.unwrap_or(0).clamp(0, UNIT),
        }),
        Kind::Slider => {
            let min = n.props.min.unwrap_or(0);
            let max = n.props.max.unwrap_or(100).max(min.saturating_add(1));
            let value = bound(n.props.bind.as_ref())
                .and_then(|v| v.as_int())
                .unwrap_or(min)
                .clamp(min, max);
            Some(Control::Slider { value, min, max })
        }
        Kind::Switch => Some(Control::Switch {
            on: bound(n.props.bind.as_ref())
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        }),
        Kind::Checkbox => Some(Control::Checkbox {
            checked: bound(n.props.bind.as_ref())
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        }),
        Kind::Radio => {
            let selected = match (
                bound(n.props.bind.as_ref()).and_then(|v| v.as_str()),
                n.props.value.as_deref(),
            ) {
                (Some(cur), Some(val)) => cur == val,
                _ => false,
            };
            Some(Control::Radio { selected })
        }
        Kind::Tabs => {
            let labels = n.props.labels.clone();
            let current = bound(n.props.bind.as_ref()).and_then(|v| v.as_str());
            let selected = current
                .and_then(|c| labels.iter().position(|l| l == c))
                .unwrap_or(0);
            Some(Control::Tabs { labels, selected })
        }
        Kind::Spinner => Some(Control::Spinner),
        Kind::Table => Some(Control::Table {
            columns: n.props.columns.unwrap_or(3).clamp(1, 16),
            rows: n.props.rows.unwrap_or(3).clamp(1, 16),
            headers: n.props.labels.clone(),
        }),
        Kind::Chart => {
            let mut values = n.props.values.clone();
            if values.is_empty() {
                // shortcut: deterministic stand-in bars, upgrade when a data source exists
                values = vec![3, 8, 5, 10, 6, 4];
            }
            Some(Control::Chart { values })
        }
        Kind::Calendar => Some(Control::Calendar {
            columns: n.props.columns.unwrap_or(7).clamp(1, 9),
            rows: n.props.rows.unwrap_or(5).clamp(1, 9),
        }),
        Kind::RichText => Some(Control::RichText {
            text: state.interpolate(n.props.value.as_deref().unwrap_or("")),
        }),
        Kind::Player => Some(Control::Player),
        Kind::Map => Some(Control::Map),
        Kind::Menu => {
            let items = if n.props.labels.is_empty() {
                vec!["File".into(), "Edit".into(), "View".into()]
            } else {
                n.props.labels.clone()
            };
            Some(Control::Menu { items })
        }
        Kind::Tooltip => Some(Control::Tooltip {
            text: state.interpolate(
                n.props
                    .label
                    .as_deref()
                    .or(n.props.value.as_deref())
                    .unwrap_or(""),
            ),
        }),
        Kind::Extended(code) => Some(Control::Extended {
            code,
            values: n.props.values.clone(),
        }),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn place(
    n: &Node,
    rect: Rect,
    bp: Breakpoint,
    pal: &Palette,
    state: &StateView,
    inherited_align: Align,
    elapsed_ms: i64,
    clip: Option<Rect>,
    out: &mut Vec<LaidOut>,
) {
    if !visible(n, state) {
        return; // hidden subtree: no layout, no raster
    }
    let dir = direction(n.kind, bp);
    let scale = font_scale(&n.props, 16);
    let (rect, anim_p) = entry_transform(n, rect, elapsed_ms);

    let radius = match n.kind {
        Kind::Icon | Kind::Avatar => rect.w.min(rect.h) / 2,
        Kind::Badge => rect.h / 2,
        _ => n.props.radius.unwrap_or(pal.radius),
    }
    .max(0)
    .min(rect.w.min(rect.h).max(0) / 2);

    let (mut fill, mut border, mut border_w) = match n.kind {
        Kind::Column
        | Kind::Row
        | Kind::Container
        | Kind::Card
        | Kind::List
        | Kind::Grid
        | Kind::Scroll
        | Kind::Dialog => (
            Some(n.props.color.unwrap_or(pal.surface)),
            Some(n.props.border.unwrap_or(pal.border)),
            1,
        ),
        Kind::Button => (Some(n.props.color.unwrap_or(pal.accent)), n.props.border, 0),
        Kind::Input => (
            Some(n.props.color.unwrap_or(pal.surface)),
            Some(n.props.border.unwrap_or(pal.border)),
            1,
        ),
        Kind::Image => (
            Some(
                n.props
                    .color
                    .unwrap_or_else(|| image_color(n.props.src.as_deref().unwrap_or(""))),
            ),
            Some(n.props.border.unwrap_or(pal.border)),
            1,
        ),
        Kind::Text | Kind::Spacer => (None, None, 0),
        Kind::Divider => (Some(n.props.border.unwrap_or(pal.border)), None, 0),
        Kind::Icon => (Some(n.props.color.unwrap_or(pal.accent)), n.props.border, 0),
        Kind::Badge => (Some(n.props.color.unwrap_or(pal.accent)), n.props.border, 0),
        Kind::Progress | Kind::Slider => (Some(n.props.color.unwrap_or(pal.border)), None, 0),
        Kind::Switch | Kind::Checkbox | Kind::Radio => (
            Some(n.props.color.unwrap_or(pal.surface)),
            Some(n.props.border.unwrap_or(pal.border)),
            1,
        ),
        Kind::Tabs => (
            Some(n.props.color.unwrap_or(pal.surface)),
            Some(n.props.border.unwrap_or(pal.border)),
            1,
        ),
        Kind::Avatar => (Some(n.props.color.unwrap_or(pal.accent)), n.props.border, 0),
        Kind::Spinner => (None, None, 0),
        Kind::Table
        | Kind::Chart
        | Kind::Calendar
        | Kind::RichText
        | Kind::Player
        | Kind::Map
        | Kind::Menu
        | Kind::Tooltip
        | Kind::Extended(_) => (
            Some(n.props.color.unwrap_or(pal.surface)),
            Some(n.props.border.unwrap_or(pal.border)),
            1,
        ),
        Kind::Other => {
            if leaf_like(n) {
                (None, None, 0)
            } else {
                (
                    Some(n.props.color.unwrap_or(pal.surface)),
                    Some(n.props.border.unwrap_or(pal.border)),
                    1,
                )
            }
        }
    };
    // Glass is a frosted fill: the base colour blended with the accent.
    fill = match (fill, n.props.glass) {
        (Some(c), true) => Some(lerp_color(c, pal.accent, UNIT / 4)),
        (f, _) => f,
    };
    let contrast = n.props.high_contrast || pal.high_contrast;
    if contrast && fill.is_some() {
        border = Some(contrast_text(pal.bg));
        border_w = 2;
    }

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
        Kind::Badge => (
            Some(node_text(n, state)),
            Some(pal.on_accent),
            Align::Center,
        ),
        Kind::Avatar => (
            Some(node_text(n, state)),
            Some(pal.on_accent),
            Align::Center,
        ),
        Kind::Icon => (
            Some(node_text(n, state)),
            Some(pal.on_accent),
            Align::Center,
        ),
        Kind::Dialog => (
            Some(node_text(n, state)),
            Some(n.props.color.unwrap_or(pal.fg)),
            Align::Start,
        ),
        Kind::Other if leaf_like(n) => (
            Some(node_text(n, state)),
            Some(n.props.color.unwrap_or(pal.fg)),
            inherited_align,
        ),
        _ => (None, None, Align::Start),
    };
    let text_color = if contrast {
        Some(contrast_text(fill.unwrap_or(pal.bg)))
    } else {
        text_color
    };

    let effects = Effects {
        gradient: n.props.gradient,
        glass: n.props.glass,
        shadow: n.props.shadow.unwrap_or(0).clamp(0, 8),
        opacity: (n.props.opacity.unwrap_or(UNIT) * anim_p) / UNIT,
    };

    // The icon glyph is drawn from the declared name; the fallback glyph is
    // only a safety net for a name the language layer failed to reject.
    let icon_name = if n.kind == Kind::Icon {
        n.props.name.clone()
    } else {
        None
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
        icon_name,
        text_color,
        text_scale: scale,
        text_align,
        effects,
        visual_effects: n.props.effects.clone(),
        particles: n.props.particles,
        control: control_of(n, state),
        clip,
        text_origin: None,
        phase: elapsed_ms,
        accent: pal.accent,
        on_accent: pal.on_accent,
    });

    if n.children.is_empty() || rect.w <= 0 || rect.h <= 0 {
        return;
    }

    match n.kind {
        Kind::Grid => place_grid(n, rect, bp, pal, state, elapsed_ms, clip, out),
        Kind::Scroll => place_scroll(n, rect, bp, pal, state, elapsed_ms, out),
        Kind::Dialog => place_dialog(n, rect, bp, pal, state, elapsed_ms, clip, out),
        _ => place_children(n, rect, bp, pal, state, elapsed_ms, clip, out),
    }
}

/// Generic container layout: children share the main axis in proportion to
/// their natural size (fluid sizing), honouring explicit `width`/`height`.
#[allow(clippy::too_many_arguments)]
fn place_children(
    n: &Node,
    rect: Rect,
    bp: Breakpoint,
    pal: &Palette,
    state: &StateView,
    elapsed_ms: i64,
    clip: Option<Rect>,
    out: &mut Vec<LaidOut>,
) {
    let dir = direction(n.kind, bp);
    let content = inset(rect, n.props.padding);
    let vertical = dir == Direction::Vertical;
    let content_main = if vertical { content.h } else { content.w };
    let cross_avail = if vertical { content.w } else { content.h };
    let kids: Vec<&Node> = n.children.iter().filter(|c| visible(c, state)).collect();
    let count = kids.len() as i64;
    let gap = n.props.gap.max(0);
    let gaps = gap.saturating_mul(count.saturating_sub(1));
    let avail = (content_main - gaps).max(0);

    let mut sizes: Vec<i64> = kids
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
    for (i, c) in kids.iter().enumerate() {
        // Explicit width/height props win over the fluid share on their own axis.
        let (main_override, cross_override) = if vertical {
            (c.props.height, c.props.width)
        } else {
            (c.props.width, c.props.height)
        };
        let main = if main_override.is_none() && c.kind == Kind::Icon && count > 1 {
            // A shared Icon keeps its intrinsic size instead of absorbing the
            // fluid share, so a Row of icons paints 16x16 glyphs rather than
            // stretched bars; a sole Icon still fills its fixed-size Container.
            let natural = if vertical {
                natural_h(c, bp, state)
            } else {
                natural_w(c, bp, state)
            };
            natural.min(content_main).max(0)
        } else {
            main_override.unwrap_or(sizes[i]).min(content_main).max(0)
        };
        let cross = cross_override
            .unwrap_or_else(|| {
                if c.kind == Kind::Divider {
                    // A divider spans the parent's cross axis.
                    cross_avail.max(0)
                } else if leaf_like(c) {
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
        let child_rect = inset(child_rect, c.props.margin);
        place(
            c,
            child_rect,
            bp,
            pal,
            state,
            n.props.align,
            elapsed_ms,
            clip,
            out,
        );
        cursor = cursor.saturating_add(main.saturating_add(gap));
    }
}

/// Fluid grid: `columns` columns, row-major, each row as tall as its tallest cell.
///
/// An explicit `columns` prop keeps the fixed grid at every tier. Without it,
/// the expanded tiers and above size themselves from the container width with
/// the integer fluid grid: column count, gutter and margin.
#[allow(clippy::too_many_arguments)]
fn place_grid(
    n: &Node,
    rect: Rect,
    bp: Breakpoint,
    pal: &Palette,
    state: &StateView,
    elapsed_ms: i64,
    clip: Option<Rect>,
    out: &mut Vec<LaidOut>,
) {
    let kids: Vec<&Node> = n.children.iter().filter(|c| visible(c, state)).collect();
    if kids.is_empty() {
        return;
    }
    let base = inset(rect, n.props.padding);
    // The compact and medium tiers keep the fixed grid; the expanded tiers and
    // above derive the column count, gutter and margin from the width.
    let fluid = if bp >= Breakpoint::Expanded && n.props.columns.is_none() {
        Some(fluid_grid(base.w))
    } else {
        None
    };
    let cols = fluid.map_or_else(|| n.props.columns.unwrap_or(2).max(1), |g| g.columns);
    let gutter = fluid.map_or(0, |g| g.gutter);
    let content = match fluid {
        Some(g) => inset(base, g.margin),
        None => base,
    };
    // Integer division: the remainder pixels go to the leftmost columns.
    let col_total = (content.w - gutter.saturating_mul(cols.saturating_sub(1))).max(0);
    let col_w = col_total / cols;
    let col_rem = col_total % cols;
    let rows = (kids.len() as i64 + cols - 1) / cols;

    // Row heights: the tallest natural height in each row.
    let mut row_h = vec![0i64; rows as usize];
    for (i, c) in kids.iter().enumerate() {
        let row = (i as i64 / cols) as usize;
        row_h[row] = row_h[row].max(natural_h(c, bp, state));
    }

    let mut y = content.y;
    for row in 0..rows as usize {
        if y >= content.y + content.h {
            break;
        }
        let h = row_h[row].min((content.y + content.h - y).max(0));
        let mut x = content.x;
        for col in 0..cols {
            let idx = row * cols as usize + col as usize;
            if idx >= kids.len() {
                break;
            }
            let c = kids[idx];
            let w = col_w + if (col as i64) < col_rem { 1 } else { 0 };
            let cw = c.props.width.unwrap_or(w).min(w).max(0);
            let ch = c.props.height.unwrap_or(h).min(h).max(0);
            let child_rect = Rect {
                x: x + (w - cw) / 2,
                y: y + (h - ch) / 2,
                w: cw,
                h: ch,
            };
            let child_rect = inset(child_rect, c.props.margin);
            place(
                c,
                child_rect,
                bp,
                pal,
                state,
                n.props.align,
                elapsed_ms,
                clip,
                out,
            );
            x += w;
            // The gutter separates two columns; the last one adds no trailing gap.
            if col + 1 < cols {
                x += gutter;
            }
        }
        y += h;
    }
}

/// Scroll: children keep their natural size inside a clipped viewport.
#[allow(clippy::too_many_arguments)]
fn place_scroll(
    n: &Node,
    rect: Rect,
    bp: Breakpoint,
    pal: &Palette,
    state: &StateView,
    elapsed_ms: i64,
    out: &mut Vec<LaidOut>,
) {
    let content = inset(rect, n.props.padding);
    let clip = Some(rect);
    let gap = n.props.gap.max(0);
    let mut y = content.y;
    for c in n.children.iter().filter(|c| visible(c, state)) {
        let h = c.props.height.unwrap_or_else(|| natural_h(c, bp, state));
        let w = c
            .props
            .width
            .unwrap_or_else(|| natural_w(c, bp, state))
            .min(content.w)
            .max(0);
        let child_rect = Rect {
            x: content.x,
            y,
            w,
            h,
        };
        let child_rect = inset(child_rect, c.props.margin);
        place(
            c,
            child_rect,
            bp,
            pal,
            state,
            n.props.align,
            elapsed_ms,
            clip,
            out,
        );
        y = y.saturating_add(h.saturating_add(gap));
    }
}

/// Dialog: a title band on top, children below it.
#[allow(clippy::too_many_arguments)]
fn place_dialog(
    n: &Node,
    rect: Rect,
    bp: Breakpoint,
    pal: &Palette,
    state: &StateView,
    elapsed_ms: i64,
    clip: Option<Rect>,
    out: &mut Vec<LaidOut>,
) {
    let content = inset(rect, n.props.padding);
    let title_h = font::GLYPH_H.saturating_mul(font_scale(&n.props, 16));
    // The title band belongs to the dialog box itself.
    if let Some(last) = out.last_mut() {
        if last.id == n.id {
            last.text_origin = Some((content.x, content.y));
        }
    }
    let body = Rect {
        x: content.x,
        y: content.y + title_h + 4,
        w: content.w,
        h: (content.h - title_h - 4).max(0),
    };
    let gap = n.props.gap.max(0);
    let mut y = body.y;
    for c in n.children.iter().filter(|c| visible(c, state)) {
        let h = c.props.height.unwrap_or_else(|| natural_h(c, bp, state));
        let w = c
            .props
            .width
            .unwrap_or_else(|| natural_w(c, bp, state))
            .min(body.w)
            .max(0);
        let child_rect = Rect { x: body.x, y, w, h };
        let child_rect = inset(child_rect, c.props.margin);
        place(
            c,
            child_rect,
            bp,
            pal,
            state,
            n.props.align,
            elapsed_ms,
            clip,
            out,
        );
        y = y.saturating_add(h.saturating_add(gap));
    }
}
