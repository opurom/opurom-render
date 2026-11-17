//! Pure, deterministic adaptive layout. All arithmetic is integer i64/i128:
//! no floats, no time, no randomness, no host queries.

use crate::anim::{default_duration_ms, AnimSpec, AnimTable, AnimValue, Curve};
use crate::effects::{VisualEffect, VisualEffectKind, EFFECT_UNIT};
use crate::geometry::{CornerRadii, RoundingRadii};
use crate::image::ImageFit;
use crate::interaction::{NodeState, Tone, Variant};
use crate::ir::{Contrast, Directionality};
use crate::particles::Particles;
use crate::{font, Align, Ir, Kind, Node, Props, Rgba, StateValue, StateView, Theme, ThemeMode};

/// Elapsed time at which every animation curve has settled.
pub const SETTLED_MS: i64 = 4000;

/// Fixed-point unit: 1024 == fully opaque, 0 == fully transparent.
pub const UNIT: i64 = 1024;

/// Rows placed beyond each side of the scroll clip for a lazy `For` list.
const SLIVER_OVERSCAN: usize = 2;

/// Fixed positions of the animated props inside an `AnimTable` entry vector.
const ANIMATED_RADIUS: usize = 0;
const ANIMATED_PADDING: usize = 1;
const ANIMATED_SHADOW: usize = 2;
const ANIMATED_COLOR: usize = 3;
const ANIMATED_OPACITY: usize = 4;
const ANIMATED_COUNT: usize = 5;

/// The animated prop values of one node, in the fixed order above. The colour
/// slot is a fully transparent `AnimValue::Color` when the node has no fill,
/// so a missing fill never draws a box and a first-layout colour fades in from
/// transparent.
type AnimProps = [AnimValue; ANIMATED_COUNT];

fn anim_props_default() -> AnimProps {
    [
        AnimValue::scalar(0),
        AnimValue::scalar(0),
        AnimValue::scalar(0),
        AnimValue::color(Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        }),
        AnimValue::scalar(0),
    ]
}

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
    /// Two-stop linear gradient (top to bottom, or along the horizontal axis
    /// when `gradient_horizontal` is set).
    pub gradient: Option<(Rgba, Rgba)>,
    /// Sweep the gradient along the horizontal axis (`gradientDirection`).
    pub gradient_horizontal: bool,
    /// Frosted fill (surface blended with accent).
    pub glass: bool,
    /// Soft shadow strength, 0..=8.
    pub shadow: i64,
    /// Alpha in fixed point (0..=1024).
    pub opacity: i64,
}

/// A `glass: true` surface is a liquid-glass lamina, not the flat frosted mix
/// applied above. Expand it into the native effect pipeline so the software
/// framebuffer and the GPU display-list commands both refract the backdrop and
/// carry a specular edge, matching the DOM emitter. Explicit `effects` glass or
/// liquid entries win, and a node painting its own text keeps its flat fill so
/// the backdrop pass cannot overwrite those glyphs (child text is safe: it is
/// drawn later in pre-order).
fn glass_lamina(n: &Node, pal: &Palette, own_text: bool, out: &mut Vec<VisualEffect>) {
    if !n.props.glass || own_text {
        return;
    }
    let declared = out
        .iter()
        .any(|e| matches!(e.kind, VisualEffectKind::Glass | VisualEffectKind::Liquid));
    if declared {
        return;
    }
    out.push(VisualEffect {
        kind: VisualEffectKind::Glass,
        radius: 2,
        dx: 0,
        dy: 0,
        alpha: EFFECT_UNIT * 42 / 100,
        color: Some(pal.surface),
    });
    out.push(VisualEffect {
        kind: VisualEffectKind::Liquid,
        radius: 0,
        dx: 0,
        dy: 0,
        alpha: EFFECT_UNIT * 16 / 100,
        color: Some(Rgba::rgb(0xff, 0xff, 0xff)),
    });
}

impl Default for Effects {
    fn default() -> Self {
        Effects {
            gradient: None,
            gradient_horizontal: false,
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
    /// Normalized per-corner radii in render order TL, TR, BL, BR.
    pub radii: RoundingRadii,
    pub fill: Option<Rgba>,
    pub border: Option<Rgba>,
    pub border_w: i64,
    pub text: Option<String>,
    /// Built-in icon name for `Kind::Icon`, or `None` when it has none.
    pub icon_name: Option<String>,
    /// Package icon geometry resolved from the IR `icons` table for this
    /// node's name. `None` falls back to the built-in glyph set at raster time.
    pub icon_shape: Option<crate::icons::IconPaths>,
    pub text_color: Option<Rgba>,
    pub text_scale: i64,
    pub text_align: Align,
    /// Resolved text metrics mode; `Fixed` keeps committed frames byte-identical.
    pub font_metrics: font::FontMetrics,
    /// Optional wrapped line limit for Text nodes; `None` keeps legacy clipping.
    pub max_lines: Option<usize>,
    /// PNG path for an Image node; absent means the historical placeholder box.
    pub image_src: Option<String>,
    pub image_fit: ImageFit,
    pub effects: Effects,
    /// Resolved M3 material elevation level (0/1/2/3/4/6, 0 = none).
    /// Authored `elevation` for the default path; M-projected for state-driven
    /// and in-flight values. Draws the material shadow when > 0.
    pub elevation: i64,
    /// Typed software effects, in IR array order.
    pub visual_effects: Vec<VisualEffect>,
    /// Opt-in deterministic particle field for this node's rect.
    pub particles: Option<Particles>,
    pub control: Option<Control>,
    /// Clip rectangle inherited from a `Scroll` viewport.
    pub clip: Option<Rect>,
    /// Rounded boundary for opt-in content clip coverage.
    pub clip_shape: crate::raster::ClipShapeSlot,
    /// Absolute glyph origin, used when the text is not centred in the box.
    pub text_origin: Option<(i64, i64)>,
    /// Exact text typography for the system-font span layer; none of these
    /// feed the quantized bitmap raster path.
    pub text_weight: Option<String>,
    pub text_letter_spacing: Option<i64>,
    pub text_font_px: Option<i64>,
    /// Absolute baseline from vertical metrics, when activated (opt-in).
    pub text_baseline: Option<i64>,
    /// Node line-height multiplier (fixed 1024==1.0), carried for the baseline pass.
    pub text_line_height: Option<i64>,
    /// Animation clock phase in milliseconds (drives the spinner).
    pub phase: i64,
    pub accent: Rgba,
    pub on_accent: Rgba,
    /// Resolved interaction state, design-system variant and semantic tone.
    pub state: NodeState,
    /// Blended state paint during a state transition; `None` on the default
    /// path, where `raster::draw` applies the discrete transform as before.
    pub state_paint: crate::interaction::StatePaintSlot,
    pub variant: Option<Variant>,
    pub tone: Option<Tone>,
    /// Resolved tone colour (theme token or measured constant) for the status
    /// dot; `None` when the node has no tone.
    pub tone_color: Option<Rgba>,
    /// Additive incremental-render metadata (identity + hashes). Excluded from
    /// `PartialEq` so frame-diff semantics stay byte-identical.
    pub incremental: crate::incremental::IncrementalMeta,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub bg: Rgba,
    /// Page backdrop actually painted on the canvas. Equals `bg` unless a
    /// `stage` token declares a tinted backdrop for glass refraction.
    pub stage: Rgba,
    pub fg: Rgba,
    pub surface: Rgba,
    pub border: Rgba,
    pub accent: Rgba,
    pub on_accent: Rgba,
    pub radius: i64,
    pub high_contrast: bool,
    /// Theme token overrides for semantic tones; `None` keeps the measured
    /// `interaction` constant for that tone.
    pub success: Option<Rgba>,
    pub warning: Option<Rgba>,
    pub error: Option<Rgba>,
    pub info: Option<Rgba>,
    pub primary: Option<Rgba>,
    pub secondary: Option<Rgba>,
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
            stage: theme.tokens.stage.unwrap_or(bg),
            fg: theme.tokens.on_surface.unwrap_or(fg),
            surface: theme.tokens.surface.unwrap_or(surface),
            border,
            accent: theme.accent,
            on_accent: Rgba::rgb(0xff, 0xff, 0xff),
            radius: theme.radius,
            high_contrast: theme.tokens.contrast == Contrast::High,
            success: theme.tokens.success,
            warning: theme.tokens.warning,
            error: theme.tokens.error,
            info: theme.tokens.info,
            primary: theme.tokens.primary,
            secondary: theme.tokens.secondary,
        }
    }

    /// Resolved colour for a semantic tone: the theme token when present, else
    /// the measured interaction constant.
    pub fn tone_color(&self, tone: Tone) -> Rgba {
        match tone {
            Tone::Success => self.success.unwrap_or(crate::interaction::SUCCESS),
            Tone::Warning => self.warning.unwrap_or(crate::interaction::WARNING),
            Tone::Error => self.error.unwrap_or(crate::interaction::ERROR),
            Tone::Info => self.info.unwrap_or(crate::interaction::INFO),
            Tone::Neutral => crate::interaction::DEEP_INK,
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

/// Logical `start`/`end` swap under RTL (IR_ADDITIONS §15.2). `Center` and every
/// non-alignment value pass through unchanged; this is axis-independent, like
/// the web painter's `opuMirrorAlign`.
fn mirror_align(a: Align, rtl: bool) -> Align {
    if !rtl {
        return a;
    }
    match a {
        Align::Start => Align::End,
        Align::End => Align::Start,
        Align::Center => Align::Center,
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

/// Resolve `theme.fontMetrics` into every node that has no explicit
/// `fontMetrics` prop, so text measurement, glyph sizing, layout and the
/// accessibility tree all agree on one metrics mode.
fn fill_font_metrics(n: &mut Node, global: font::FontMetrics) {
    if n.props.font_metrics.is_none() {
        n.props.font_metrics = Some(global);
    }
    for c in &mut n.children {
        fill_font_metrics(c, global);
    }
}

fn font_scale(p: &Props, default_size: i64) -> i64 {
    (text_size(p, default_size).saturating_add(3) / 7).clamp(1, 8)
}

/// Quantized scale derived from an explicit `fontPx` (SYSTEM_FONT_DISPLAY.md).
/// `ceil(fontPx/7)` with a floor of 1, i128 integer-only. `None` keeps the
/// legacy quantized scale, so default paths are byte-identical.
fn font_scale_from_px(p: &Props) -> Option<i64> {
    p.font_px.map(|px| {
        let ceil = (px as i128 + 6) / 7;
        ceil.max(1) as i64
    })
}

/// Natural text height from an explicit `fontPx`: a 1.5 line-height box
/// (`ceil(3*fontPx/2)`) that leaves room for glyph descenders. i128 integer
/// ceiling, no floats.
fn font_height_from_px(px: i64) -> i64 {
    ((3 * px as i128 + 1) / 2) as i64
}

fn text_width(text: &str, scale: i64, metrics: font::FontMetrics) -> i64 {
    font::text_width(text, scale, metrics)
}

/// Effective text metrics for a node. A node built outside the theme-aware
/// layout (e.g. a hand-made test tree) falls back to `Fixed`, so committed
/// frames stay byte-identical.
fn node_metrics(n: &Node) -> font::FontMetrics {
    n.props.font_metrics.unwrap_or(font::FontMetrics::Fixed)
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

/// Glyph drawn per bound value character when an `Input` is `secure`.
const SECURE_MASK: char = '*';

pub(crate) fn node_text(n: &Node, state: &StateView) -> String {
    match n.kind {
        Kind::Text => state.interpolate(n.props.value.as_deref().unwrap_or("")),
        Kind::Button => state.interpolate(n.props.label.as_deref().unwrap_or("")),
        Kind::Input => match n.props.bind.as_deref().and_then(|b| state.get(b)) {
            // Secure: mask one glyph per bound character; an empty value has no
            // masked characters, so the placeholder shows. The raw value never
            // reaches the display string.
            Some(v) if n.props.secure => {
                let raw = v.as_text();
                if raw.is_empty() {
                    state.interpolate(n.props.placeholder.as_deref().unwrap_or(""))
                } else {
                    SECURE_MASK.to_string().repeat(raw.chars().count())
                }
            }
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
    let m = node_metrics(n);
    let labels: i64 = n.props.labels.iter().map(|l| text_width(l, scale, m)).sum();
    let gaps = 8i64.saturating_mul(n.props.labels.len().saturating_sub(1) as i64);
    labels.saturating_add(gaps)
}

fn natural_w(n: &Node, bp: Breakpoint, state: &StateView) -> i64 {
    if let Some(w) = n.props.width {
        return w;
    }
    let scale = font_scale_from_px(&n.props).unwrap_or_else(|| font_scale(&n.props, 16));
    let m = node_metrics(n);
    match n.kind {
        Kind::Text => text_width(&node_text(n, state), scale, m),
        Kind::Button => text_width(&node_text(n, state), scale, m).saturating_add(24),
        Kind::Input => 120,
        Kind::Image => 64,
        Kind::Spacer => 8,
        Kind::Divider => 0, // spans the parent width
        Kind::Icon => 16,
        Kind::Badge => text_width(&node_text(n, state), scale, m).saturating_add(12),
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
                text_width(&node_text(n, state), scale, m)
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
        // The body height times the current element count, plus the node gap
        // between generated rows; no elements means no rows and no height.
        Kind::For => {
            let len = list_elements(n, state).len() as i64;
            if len == 0 {
                0
            } else {
                let body = container_extent(n, bp, state, false);
                let gaps = n.props.gap.max(0).saturating_mul(len - 1);
                body.saturating_mul(len).saturating_add(gaps)
            }
        }
        Kind::Text => n
            .props
            .font_px
            .map(font_height_from_px)
            .unwrap_or_else(|| font::GLYPH_H.saturating_mul(scale)),
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

/// Greedy word wrap at `max` built-in glyph columns; overlong words are chunked.
fn wrap_text_count(text: &str, max: usize, max_lines: Option<usize>) -> usize {
    let max = max.max(1);
    let mut lines = 0usize;
    let mut cur = 0usize;
    for word in text.split_whitespace() {
        let mut rest = word.chars().count();
        while rest > max {
            if cur > 0 {
                lines += 1;
                cur = 0;
            }
            lines += 1;
            rest -= max;
        }
        if rest == 0 {
            continue;
        }
        if cur == 0 {
            cur = rest;
        } else if cur + 1 + rest <= max {
            cur += 1 + rest;
        } else {
            lines += 1;
            cur = rest;
        }
    }
    if cur > 0 {
        lines += 1;
    }
    let lines = lines.max(1);
    match max_lines {
        Some(limit) => lines.min(limit.max(1)),
        None => lines,
    }
}

/// Overlap of two clip/rect areas; an empty (zero-size) result clips everything.
fn clip_intersect(a: Rect, b: Rect) -> Rect {
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

/// Compute the full pre-order layout for one viewport. Pure: same inputs, same output.
pub fn layout(ir: &Ir, state: &StateView, width: u32, height: u32) -> Vec<LaidOut> {
    layout_at(ir, state, width, height, SETTLED_MS)
}

/// Layout at a given animation clock position with a fresh animation table.
///
/// This is the pure, stateless path: a node animates only its entry appearance
/// (the first layout installs the zeroed `from` values). Renderers that track
/// prop changes across ticks use [`layout_animated_at`] with a persistent table.
pub fn layout_at(
    ir: &Ir,
    state: &StateView,
    width: u32,
    height: u32,
    elapsed_ms: i64,
) -> Vec<LaidOut> {
    layout_animated_at(ir, state, width, height, elapsed_ms, &mut AnimTable::new())
}

/// Opt-in baseline pass: for nodes with an exact `fontPx` whose family has
/// vertical metrics in the theme, compute a real baseline and line box. Every
/// new field is absent by default, so this pass is a no-op on the legacy path.
fn apply_text_baselines(out: &mut [LaidOut], theme: &Theme) {
    let Some(family) = theme.font_family.as_deref() else {
        return;
    };
    let Some(m) = theme.vertical_metrics.get(family) else {
        return;
    };
    for n in out.iter_mut() {
        let Some(px) = n.text_font_px else {
            continue;
        };
        let round = |v: i64| -> i64 { ((v as i128 * px as i128 + 512) / 1024) as i64 };
        let ascent = round(m.ascent);
        let descent = round(m.descent);
        let leading = ((m.leading as i128 * px as i128) / 1024) as i64;
        let metric_box = ascent + descent + leading;
        // kTextHeightNone (0) and absent both use the metrics-derived box;
        // a positive multiplier overrides it with round(lineHeight * px).
        let line_box = match n.text_line_height {
            Some(v) if v > 0 => round(v),
            _ => metric_box,
        };
        if line_box > 0 {
            n.rect.h = line_box;
        }
        // A positive lineHeight overrides the metrics box: Flutter's
        // proportional leading distribution splits (line_box - unscaled
        // ascent+descent) by the ascent/descent ratio, so the baseline scales
        // with lineHeight (TEXT_BASELINE_PROPOSAL.md §3.2). All intermediate
        // math is i128 fixed-point with 1024 == 1 em; the unset/0 path below
        // keeps its byte-identical old geometry.
        n.text_baseline = Some(match n.text_line_height {
            Some(v) if v > 0 && m.ascent + m.descent > 0 => {
                let denom = (m.ascent + m.descent) as i128;
                let line_units = line_box as i128 * 1024;
                let natural_units = denom * px as i128;
                let leading_units = (line_units - natural_units) * m.ascent as i128 / denom;
                let scaled = m.ascent as i128 * px as i128 + leading_units;
                let rounded = if scaled >= 0 {
                    (scaled + 512) / 1024
                } else {
                    -((-scaled + 512) / 1024)
                };
                n.rect.y + rounded as i64
            }
            _ => n.rect.y + ascent + leading / 2,
        });
    }
}

/// Layout at a given animation clock position, updating `anim` with each
/// node's animated targets. The table is the renderer's cross-tick animation
/// state: a changed target restarts the interpolation from the previous target.
pub fn layout_animated_at(
    ir: &Ir,
    state: &StateView,
    width: u32,
    height: u32,
    elapsed_ms: i64,
    anim: &mut AnimTable,
) -> Vec<LaidOut> {
    let bp = Breakpoint::from_width(width);
    let rtl = ir.directionality == Directionality::Rtl;
    let pal = Palette::from_theme(&ir.theme);
    let mut tree = with_theme_font_scale(&ir.tree, ir.theme.font_scale);
    fill_font_metrics(&mut tree, ir.theme.font_metrics);
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
        rtl,
        Align::Start,
        elapsed_ms,
        None,
        crate::raster::ClipShapeSlot(None),
        &ir.icons,
        anim,
        &mut out,
        None,
    );
    apply_text_baselines(&mut out, &ir.theme);
    out
}

/// Opt-in incremental layout with a keyed memo cache (§6). Output is
/// byte-identical to [`layout_animated_at`] with a cold cache; a warm cache
/// reuses unchanged subtrees and records work in `cache.counters`.
pub fn layout_cached_at(
    ir: &Ir,
    state: &StateView,
    width: u32,
    height: u32,
    elapsed_ms: i64,
    cache: &mut crate::incremental::LayoutCache,
) -> Vec<LaidOut> {
    let bp = Breakpoint::from_width(width);
    let rtl = ir.directionality == Directionality::Rtl;
    let pal = Palette::from_theme(&ir.theme);
    let mut tree = with_theme_font_scale(&ir.tree, ir.theme.font_scale);
    fill_font_metrics(&mut tree, ir.theme.font_metrics);
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
    let mut anim = AnimTable::new();
    place(
        &tree,
        root,
        bp,
        &pal,
        state,
        rtl,
        Align::Start,
        elapsed_ms,
        None,
        crate::raster::ClipShapeSlot(None),
        &ir.icons,
        &mut anim,
        &mut out,
        Some(cache),
    );
    apply_text_baselines(&mut out, &ir.theme);
    out
}

/// Entry animation: the box grows from its centre over the given fixed-point
/// progress. The opacity fade is handled by the animation table, so this only
/// scales the rect.
fn entry_transform(rect: Rect, p: i64) -> Rect {
    let p = p.clamp(0, UNIT);
    let w = rect.w.saturating_mul(p) / UNIT;
    let h = rect.h.saturating_mul(p) / UNIT;
    Rect {
        x: rect.x + (rect.w - w) / 2,
        y: rect.y + (rect.h - h) / 2,
        w,
        h,
    }
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
fn bp_code(bp: Breakpoint) -> u8 {
    match bp {
        Breakpoint::Compact => 0,
        Breakpoint::Medium => 1,
        Breakpoint::Expanded => 2,
        Breakpoint::Wide => 3,
        Breakpoint::Ultra => 4,
    }
}

fn place(
    n: &Node,
    rect: Rect,
    bp: Breakpoint,
    pal: &Palette,
    state: &StateView,
    rtl: bool,
    inherited_align: Align,
    elapsed_ms: i64,
    clip: Option<Rect>,
    clip_shape: crate::raster::ClipShapeSlot,
    icons: &std::collections::BTreeMap<String, crate::icons::IconPaths>,
    anim: &mut AnimTable,
    out: &mut Vec<LaidOut>,
    mut cache: Option<&mut crate::incremental::LayoutCache>,
) {
    if !visible(n, state) {
        return; // hidden subtree: no layout, no raster
    }
    // Incremental layout (§6): reuse a cacheable subtree when identity, hash,
    // resolved deps and incoming constraint are unchanged. A hit skips all
    // recursion and glyph work below this node. Opt-in only: with no active
    // cache this block is inert and the full path is byte-identical.
    let mut cache_key = None;
    let mut cache_start = out.len();
    if let Some(c) = cache.as_deref_mut() {
        if c.is_active() {
            let mut id = crate::incremental::NodeIdentity::node(n.id);
            if let Some(k) = c.row_key.take() {
                id.row_key = Some(k);
            }
            let sh = crate::incremental::subtree_hash(n, state);
            let dv = crate::incremental::dep_values(n, state);
            // Only clock-dependent subtrees key on elapsed_ms. Static subtrees
            // reuse their memo across frames of a changing animation clock.
            let phase_key = if subtree_clock_sensitive(n) {
                elapsed_ms
            } else {
                0
            };
            let key = c.key(&id, sh, dv, rect, clip, rtl, bp_code(bp), phase_key);
            if let Some(seg) = c.get(&key, sh, rect.y) {
                out.extend(seg.iter().cloned());
                c.counters.layout_reused += seg.len();
                return;
            }
            c.counters.layout_recomputed += 1;
            cache_key = Some((key, sh));
            cache_start = out.len();
        }
    }
    let dir = direction(n.kind, bp);
    let scale = font_scale(&n.props, 16);
    // The target rect is the laid-out rect before the entry box grows; the
    // animated radius is measured against it so a settled frame is exact.
    let target_rect = rect;

    let target_radius = if n.props.variant == Some(Variant::Fab) {
        target_rect.w.min(target_rect.h).max(0) / 2
    } else {
        match n.kind {
            Kind::Icon | Kind::Avatar => target_rect.w.min(target_rect.h) / 2,
            Kind::Badge => target_rect.h / 2,
            _ => n.props.radius.unwrap_or(pal.radius),
        }
    }
    .max(0)
    .min(target_rect.w.min(target_rect.h).max(0) / 2);

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
        Kind::Text | Kind::Spacer | Kind::For => (None, None, 0),
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
    // Variant defaults override kind defaults but never an explicit colour or
    // border prop. A tone always wins on the border and adds its status dot in
    // the central raster path.
    if let Some(variant) = n.props.variant {
        if n.props.color.is_none() {
            fill = variant.fill();
        }
        if n.props.border.is_none() {
            border = variant.border();
        }
        border_w = variant.border_w();
    }
    if let Some(tone) = n.props.tone {
        border = Some(pal.tone_color(tone));
        border_w = border_w.max(1);
    }
    // Interpolate the animated props from the previous targets to the current
    // ones over the node's schedule. A node with `animate: none` keeps its
    // declared values; a settled animation is exactly the target, so the laid
    // out node at rest is byte-identical to a non-animated render.
    let base_fill = fill;
    let target_props: AnimProps = [
        AnimValue::scalar(target_radius),
        AnimValue::scalar(n.props.padding),
        AnimValue::scalar(n.props.shadow.unwrap_or(0).clamp(0, 8)),
        match base_fill {
            Some(c) => AnimValue::color(c),
            None => AnimValue::color(Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            }),
        },
        AnimValue::scalar(n.props.opacity.unwrap_or(UNIT)),
    ];
    let (interp, entry_p) = anim_interpolate(n, anim, elapsed_ms, target_props);
    // The entry box grows only on the first layout; a later prop change keeps
    // the full target rect and animates the prop values in place.
    let rect = match entry_p {
        Some(p) => entry_transform(target_rect, p),
        None => target_rect,
    };
    let radius = interp[ANIMATED_RADIUS]
        .as_scalar()
        .unwrap_or(target_radius)
        .max(0)
        .min(rect.w.min(rect.h).max(0) / 2);
    let padding = interp[ANIMATED_PADDING]
        .as_scalar()
        .unwrap_or(n.props.padding);
    let shadow = interp[ANIMATED_SHADOW].as_scalar().unwrap_or(0);
    // A node without a base fill never gains one: the colour slot animates
    // but is dropped unless the node had a fill to begin with.
    let mut fill = if base_fill.is_none() {
        None
    } else {
        match interp[ANIMATED_COLOR] {
            AnimValue::Color(c) => Some(c),
            AnimValue::Scalar(_) => None,
        }
    };
    let opacity = interp[ANIMATED_OPACITY].as_scalar().unwrap_or(UNIT);
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

    let (text, mut text_color, text_align) = match n.kind {
        Kind::Text => (
            Some(node_text(n, state)),
            Some(n.props.color.unwrap_or(pal.fg)),
            n.props.text_align.unwrap_or(inherited_align),
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
    // Logical text alignment mirrors under RTL; the glyph run itself does not.
    let text_align = mirror_align(text_align, rtl);
    if let Some(variant) = n.props.variant {
        text_color = Some(variant.text_color());
    }
    // Explicit `textColor` overrides the variant foreground but stays below the
    // high-contrast override below, matching the foreground/background split.
    if n.props.text_color.is_some() {
        text_color = n.props.text_color;
    }
    let text_color = if contrast {
        Some(contrast_text(fill.unwrap_or(pal.bg)))
    } else {
        text_color
    };

    let effects = Effects {
        gradient: n.props.gradient,
        gradient_horizontal: n.props.gradient_horizontal,
        glass: n.props.glass,
        shadow,
        opacity,
    };
    // Authored elevation is already a legal level (whitelist-validated at parse
    // time); the default path uses it unchanged. State deltas are projected by
    // style_node / StatePaint::apply.
    let authored_elevation = n.props.elevation.unwrap_or(0);

    // The icon glyph is drawn from the declared name; the fallback glyph is
    // only a safety net for a name the language layer failed to reject. A
    // package icon resolved from the IR `icons` table takes precedence over the
    // built-in set at raster time.
    let (icon_name, icon_shape) = if n.kind == Kind::Icon {
        let name = n.props.name.clone();
        let shape = name.as_deref().and_then(|k| icons.get(k)).cloned();
        (name, shape)
    } else {
        (None, None)
    };

    let mut visual_effects = n.props.effects.clone();
    glass_lamina(n, pal, text.is_some(), &mut visual_effects);
    let node_state = NodeState::resolve(None, n.props.state, n.props.disabled);

    // Option B state-paint channel: every animated node gets a track, but it
    // is inactive until a real state change. Inactive tracks contribute no
    // paint, so entry prop animations and the default path are untouched.
    let mut state_paint = None;
    if n.props.animate != crate::anim::Curve::None {
        let target_fill = match base_fill {
            Some(c) if n.props.glass => Some(lerp_color(c, pal.accent, UNIT / 4)),
            f => f,
        };
        let target_opacity = n.props.opacity.unwrap_or(UNIT);
        let target_visual_effects = {
            let mut v = n.props.effects.clone();
            glass_lamina(n, pal, text.is_some(), &mut v);
            v
        };
        let mut target_effects = effects;
        target_effects.shadow = n.props.shadow.unwrap_or(0).clamp(0, 8);
        target_effects.opacity = target_opacity;
        let target_node = LaidOut {
            id: n.id,
            kind: n.kind,
            rect,
            direction: dir,
            radius,
            radii: RoundingRadii::all(radius),
            fill: target_fill,
            border,
            border_w,
            text: text.clone(),
            icon_name: icon_name.clone(),
            icon_shape: icon_shape.clone(),
            text_color,
            text_scale: scale,
            text_align,
            font_metrics: node_metrics(n),
            max_lines: None,
            image_src: None,
            image_fit: n.props.fit,
            effects: target_effects,
            elevation: authored_elevation,
            visual_effects: target_visual_effects,
            particles: n.props.particles,
            control: None,
            clip: None,
            clip_shape: crate::raster::ClipShapeSlot(None),
            text_origin: None,
            text_weight: None,
            text_letter_spacing: None,
            text_font_px: None,
            text_baseline: None,
            text_line_height: None,
            phase: elapsed_ms,
            accent: pal.accent,
            on_accent: pal.on_accent,
            state: node_state,
            variant: n.props.variant,
            tone: n.props.tone,
            tone_color: n.props.tone.map(|t| pal.tone_color(t)),
            state_paint: crate::interaction::StatePaintSlot(None),
            incremental: crate::incremental::IncrementalMeta {
                identity: None,
                subtree_hash: 0,
                paint_hash: 0,
            },
        };
        let styled = if node_state == NodeState::Default {
            target_node
        } else {
            crate::interaction::style_node(&target_node)
        };
        let target = crate::interaction::StatePaint::capture(&styled);
        let contract = AnimSpec::new(
            n.props.animate,
            n.props
                .duration
                .unwrap_or_else(|| default_duration_ms(n.props.animate)),
            n.props.delay.unwrap_or(0),
        );
        anim.set_state_paint(n.id, target, node_state, contract, elapsed_ms);
        state_paint = anim.state_paint_at(n.id, elapsed_ms);
    }

    // IR object keys are CSS-ordered (TL, TR, BR, BL); RoundingRadii is
    // TL, TR, BL, BR. Map by name, never by key iteration order.
    let radii = match &n.props.corner_radii {
        Some(c) => {
            let scalar = CornerRadii {
                x: radius,
                y: radius,
            };
            let raw = RoundingRadii {
                top_left: c.top_left.map_or(scalar, |(x, y)| CornerRadii { x, y }),
                top_right: c.top_right.map_or(scalar, |(x, y)| CornerRadii { x, y }),
                bottom_left: c.bottom_left.map_or(scalar, |(x, y)| CornerRadii { x, y }),
                bottom_right: c.bottom_right.map_or(scalar, |(x, y)| CornerRadii { x, y }),
            };
            raw.scaled(rect.w, rect.h)
        }
        None => RoundingRadii::all(radius),
    };
    out.push(LaidOut {
        id: n.id,
        kind: n.kind,
        rect,
        direction: dir,
        radius,
        radii,
        fill,
        border,
        border_w,
        text,
        icon_name,
        icon_shape,
        text_color,
        text_scale: scale,
        text_align,
        font_metrics: node_metrics(n),
        max_lines: if n.kind == Kind::Text {
            n.props.max_lines
        } else {
            None
        },
        image_src: if n.kind == Kind::Image {
            n.props.src.clone()
        } else {
            None
        },
        image_fit: n.props.fit,
        effects,
        elevation: authored_elevation,
        visual_effects,
        particles: n.props.particles,
        control: control_of(n, state),
        clip,
        clip_shape: clip_shape.clone(),
        text_origin: None,
        text_weight: n.props.weight.clone(),
        text_letter_spacing: n.props.letter_spacing,
        text_font_px: n.props.font_px,
        text_baseline: None,
        text_line_height: n.props.line_height,
        phase: elapsed_ms,
        accent: pal.accent,
        on_accent: pal.on_accent,
        state: node_state,
        state_paint: crate::interaction::StatePaintSlot(state_paint),
        variant: n.props.variant,
        tone: n.props.tone,
        tone_color: n.props.tone.map(|t| pal.tone_color(t)),
        incremental: crate::incremental::IncrementalMeta {
            identity: Some(crate::incremental::NodeIdentity::node(n.id)),
            subtree_hash: crate::incremental::subtree_hash(n, state),
            paint_hash: crate::incremental::paint_hash(n),
        },
    });

    if !n.children.is_empty() && rect.w > 0 && rect.h > 0 {
        // Closure A (internal seam consistency, NOT Flutter parity): pass the
        // rounded surface boundary to clipped children so their content uses
        // the same coverage as the container edge instead of a binary cut.
        let child_clip_shape = if surface_node(n) {
            let max_r = radius
                .max(n.props.corner_radii.as_ref().map_or(0, |c| {
                    c.top_left
                        .map_or(0, |v| v.0.max(v.1))
                        .max(c.top_right.map_or(0, |v| v.0.max(v.1)))
                        .max(c.bottom_left.map_or(0, |v| v.0.max(v.1)))
                        .max(c.bottom_right.map_or(0, |v| v.0.max(v.1)))
                }))
                .min(rect.w.min(rect.h).max(0) / 2);
            if max_r > 0 {
                crate::raster::ClipShapeSlot(Some(crate::raster::ClipShape { rect, radii }))
            } else {
                clip_shape.clone()
            }
        } else {
            clip_shape.clone()
        };
        match n.kind {
            Kind::Grid => place_grid(
                n,
                rect,
                bp,
                pal,
                state,
                rtl,
                elapsed_ms,
                clip,
                child_clip_shape.clone(),
                padding,
                icons,
                anim,
                out,
                cache.as_deref_mut(),
            ),
            Kind::Scroll => place_scroll(
                n,
                rect,
                bp,
                pal,
                state,
                rtl,
                elapsed_ms,
                padding,
                child_clip_shape.clone(),
                icons,
                anim,
                out,
                cache.as_deref_mut(),
            ),
            Kind::For => place_for(
                n,
                rect,
                bp,
                pal,
                state,
                rtl,
                elapsed_ms,
                clip,
                child_clip_shape.clone(),
                padding,
                icons,
                anim,
                out,
                cache.as_deref_mut(),
            ),
            Kind::Dialog => place_dialog(
                n,
                rect,
                bp,
                pal,
                state,
                rtl,
                elapsed_ms,
                clip,
                child_clip_shape.clone(),
                padding,
                icons,
                anim,
                out,
                cache.as_deref_mut(),
            ),
            _ => place_children(
                n,
                rect,
                bp,
                pal,
                state,
                rtl,
                elapsed_ms,
                clip,
                child_clip_shape.clone(),
                padding,
                icons,
                anim,
                out,
                cache.as_deref_mut(),
            ),
        }
    }
    // Store the full laid-out subtree under its memo key.
    if let (Some((key, sh)), Some(c)) = (cache_key, cache.as_deref_mut()) {
        let seg = out[cache_start..].to_vec();
        if !seg.is_empty() {
            c.insert(key, sh, seg);
        }
    }
}

/// True when any node in this subtree produces clock-dependent output: an
/// entry/implicit animation, or a spinner whose phase is the animation clock.
/// Only such subtrees need `elapsed_ms` in their layout cache key; every other
/// subtree is clock-invariant and can be reused across frames of a changing
/// clock (e.g. a press ripple pump). Keeping the clock out of static subtrees
/// is what lets a tiny ripple avoid a full-tree layout every frame.
fn subtree_clock_sensitive(n: &Node) -> bool {
    if n.props.animate != crate::anim::Curve::None || n.kind == Kind::Spinner {
        return true;
    }
    n.children.iter().any(subtree_clock_sensitive)
}

/// Resolve one node's animated props at `elapsed_ms`.
///
/// The table stores the previous targets (`from`) and the current targets
/// (`to`) plus the tick the current animation started. A first layout installs
/// `from` as the zeroed defaults so the node appears with the entry animation;
/// a changed target restarts the animation at the current tick from the
/// previous target. An unchanged target keeps the running schedule, so an
/// animation that has already settled stays settled. `animate: none` returns
/// the targets unchanged.
///
/// The returned `Option` is `Some(progress)` only for the entry animation, so
/// the caller can grow the box from its centre on the first layout and leave
/// the rect alone for later prop changes.
fn anim_interpolate(
    n: &Node,
    anim: &mut AnimTable,
    elapsed_ms: i64,
    target: AnimProps,
) -> (Vec<AnimValue>, Option<i64>) {
    if n.props.animate == Curve::None {
        return (target.to_vec(), None);
    }
    let contract = AnimSpec::new(
        n.props.animate,
        n.props
            .duration
            .unwrap_or_else(|| default_duration_ms(n.props.animate)),
        n.props.delay.unwrap_or(0),
    );
    // Shared-clock slice is meaningful only with a clock id; a bare interval
    // is ignored. Without both, the schedule is byte-identical to the old path.
    let contract = match (n.props.anim_clock.as_deref(), n.props.anim_interval) {
        (Some(_), Some((b, e))) => contract.with_interval(b, e),
        _ => contract,
    };
    let target_vec = target.to_vec();
    let existing = anim.entries.get(&n.id);
    let is_entry = existing.is_none();
    let (from, to, contract, start_ms) = match existing {
        // An unchanged target keeps the running schedule, so an animation that
        // has already settled stays settled and a running one continues.
        Some(e) if e.to == target_vec => (e.from.clone(), e.to.clone(), e.contract, e.start_ms),
        // A first layout has no previous target: it is the entry animation,
        // driven by the absolute tick from zero. A changed target restarts at
        // the current tick from the previous target.
        _ => {
            let from = existing
                .map(|e| e.to.clone())
                .unwrap_or_else(|| anim_props_default().to_vec());
            let start_ms = if existing.is_some() { elapsed_ms } else { 0 };
            (from, target_vec, contract, start_ms)
        }
    };
    anim.set_targets(n.id, from.clone(), to.clone(), contract, start_ms);
    let p = anim
        .entries
        .get(&n.id)
        .map_or(UNIT, |e| e.progress_at(elapsed_ms));
    let values = from
        .iter()
        .zip(&to)
        .map(|(&f, &t)| AnimValue::lerp(f, t, p))
        .collect();
    (values, if is_entry { Some(p) } else { None })
}

/// True for leaf kinds whose own drawing paints a text band with the full
/// glyph height, so a squeezed rect would spill into the next sibling.
fn text_bearing(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Text | Kind::Button | Kind::Input | Kind::Badge | Kind::Avatar | Kind::Dialog
    )
}

/// True when a node paints an opaque bounded surface, so its children must be
/// clipped to its own rect and may not spill onto a neighbouring card.
///
/// A `Card`/`Container`/`List`/`Input`/`Dialog` always fills; a `Column`/`Row`
/// clips only when it declares a `color`, because a transparent layout wrapper
/// is pass-through and must not hide a sibling's legitimate overflow.
fn surface_node(n: &Node) -> bool {
    match n.kind {
        Kind::Card | Kind::Container | Kind::List | Kind::Input | Kind::Dialog => true,
        Kind::Column | Kind::Row => n.props.color.is_some(),
        _ => false,
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
    rtl: bool,
    elapsed_ms: i64,
    clip: Option<Rect>,
    clip_shape: crate::raster::ClipShapeSlot,
    padding: i64,
    icons: &std::collections::BTreeMap<String, crate::icons::IconPaths>,
    anim: &mut AnimTable,
    out: &mut Vec<LaidOut>,
    mut cache: Option<&mut crate::incremental::LayoutCache>,
) {
    let dir = direction(n.kind, bp);
    let content = inset(rect, padding);
    // An opaque surface clips its children to its own box: a squeezed card must
    // not paint its content over the next sibling. A transparent wrapper passes
    // the inherited clip through unchanged.
    let child_clip = if surface_node(n) {
        Some(match clip {
            Some(outer) => clip_intersect(outer, rect),
            None => rect,
        })
    } else {
        clip
    };
    let vertical = dir == Direction::Vertical;
    let content_main = if vertical { content.h } else { content.w };
    let cross_avail = if vertical { content.w } else { content.h };
    let mut kids: Vec<&Node> = n.children.iter().filter(|c| visible(c, state)).collect();
    // Horizontal rows reverse child order under RTL: visual slot i holds source
    // child n-1-i, with each child's computed size and the gap unchanged.
    // Vertical stacks (Column and compact Row) keep their order.
    if rtl && dir == Direction::Horizontal {
        kids.reverse();
    }
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
    //
    // A vertical stack stretches children to fill a taller box (unchanged), but
    // never shrinks one below its natural height: when the content is taller than
    // the box the column keeps natural heights and grows past it, and the
    // surface/window clip trims the overflow so children never overprint a
    // neighbour. A horizontal row still shares width purely proportionally.
    let total: i128 = sizes.iter().map(|&s| s as i128).sum();
    let natural_h_floor: Vec<i64> = sizes.clone();
    if vertical {
        if avail > 0 && count > 0 && total > 0 {
            let mut assigned = 0i64;
            for (s, floor) in sizes.iter_mut().zip(&natural_h_floor) {
                let share = ((*s as i128 * avail as i128) / total) as i64;
                *s = share.max(*floor);
                assigned = assigned.saturating_add(*s);
            }
            // Any leftover from the rounding/floor goes to the earliest children.
            let mut rem = (avail - assigned).max(0);
            for s in sizes.iter_mut() {
                if rem == 0 {
                    break;
                }
                *s += 1;
                rem -= 1;
            }
        }
        // No room, or no natural content: keep natural/zero heights so the clip
        // trims cleanly instead of overprinting.
    } else if avail > 0 && count > 0 {
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
    let main_offset = match mirror_align(n.props.align, rtl) {
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
        // Cross axis first: a wrapping text's height depends on its resolved
        // width, so the width must be known before the main-axis height floor.
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
        // Height a text child needs once wrapped at `cross`: every wrapped line
        // is a full glyph band, so a rect shorter than that overprints its own
        // lines. Other leaf kinds need one line.
        let text_floor = if vertical && c.kind == Kind::Text {
            let scale = font_scale(&c.props, 16);
            let metrics = node_metrics(c);
            let max_w = (cross - 4).max(1);
            let lines = if text_width(&node_text(c, state), scale, metrics) <= cross {
                1
            } else {
                match metrics {
                    font::FontMetrics::Proportional => font::wrap_width_limited(
                        &node_text(c, state),
                        max_w,
                        scale,
                        c.props.max_lines,
                    )
                    .len(),
                    font::FontMetrics::Fixed => {
                        let adv = font::advance(scale).max(1);
                        let max_chars = (max_w.max(adv) / adv) as usize;
                        wrap_text_count(&node_text(c, state), max_chars, c.props.max_lines)
                    }
                }
            }
            .max(1) as i64;
            (font::GLYPH_H * scale)
                .saturating_add(lines.saturating_sub(1) * (font::GLYPH_H * scale + 2))
        } else {
            0
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
        } else if vertical && main_override.is_none() && leaf_like(c) && text_bearing(c.kind) {
            // A vertical stack must not clip a text child below one glyph line:
            // the raster paints the full glyph band regardless of the rect
            // height, so a squeezed rect overprints the next sibling. Exceeding
            // the parent's main extent is preferable to overprinting. Horizontal
            // rows use height as the cross axis and never take this path.
            natural_h(c, bp, state).max(sizes[i]).max(text_floor).max(0)
        } else {
            main_override.unwrap_or(sizes[i]).min(content_main).max(0)
        };
        let cross_offset = match mirror_align(n.props.align, rtl) {
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
            rtl,
            n.props.align,
            elapsed_ms,
            child_clip,
            clip_shape.clone(),
            icons,
            anim,
            out,
            cache.as_deref_mut(),
        );
        cursor = cursor.saturating_add(main.saturating_add(gap));
    }
}

/// Fluid grid: `columns` columns, row-major, each row as tall as its tallest cell.
///
/// The compact tier stacks every Grid into one column, matching the DOM
/// emitter's compact rule; an authored desktop column count would otherwise
/// clip each cell's text on a phone. From the medium tier up an explicit
/// `columns` prop keeps the fixed grid, and without it the expanded tiers and
/// above derive the column count, gutter and margin from the container width.
#[allow(clippy::too_many_arguments)]
fn place_grid(
    n: &Node,
    rect: Rect,
    bp: Breakpoint,
    pal: &Palette,
    state: &StateView,
    rtl: bool,
    elapsed_ms: i64,
    clip: Option<Rect>,
    clip_shape: crate::raster::ClipShapeSlot,
    padding: i64,
    icons: &std::collections::BTreeMap<String, crate::icons::IconPaths>,
    anim: &mut AnimTable,
    out: &mut Vec<LaidOut>,
    mut cache: Option<&mut crate::incremental::LayoutCache>,
) {
    let kids: Vec<&Node> = n.children.iter().filter(|c| visible(c, state)).collect();
    if kids.is_empty() {
        return;
    }
    let base = inset(rect, padding);
    // The compact and medium tiers keep the fixed grid; the expanded tiers and
    // above derive the column count, gutter and margin from the width.
    let fluid = if bp >= Breakpoint::Expanded && n.props.columns.is_none() {
        Some(fluid_grid(base.w))
    } else {
        None
    };
    let authored = fluid.map_or_else(|| n.props.columns.unwrap_or(2).max(1), |g| g.columns);
    // The compact tier stacks every Grid into one column, mirroring the DOM
    // emitter's `@media (max-width: 599px) .grid { 1fr }`. A Grid that is
    // really one horizontal row (avatar + label + badge + meter) declares
    // `compactColumns` to keep its authored count on a phone.
    let cols = if bp == Breakpoint::Compact {
        n.props
            .compact_columns
            .unwrap_or(1)
            .clamp(1, MAX_GRID_COLUMNS as i64)
    } else {
        authored
    };
    let gutter = fluid.map_or(0, |g| g.gutter);
    let content = match fluid {
        Some(g) => inset(base, g.margin),
        None => base,
    };
    // Integer division: the remainder pixels go to the leftmost columns.
    let col_total = (content.w - gutter.saturating_mul(cols.saturating_sub(1))).max(0);
    let col_w = col_total / cols;
    let col_rem = col_total % cols;
    // Opt-in: a direct child declares columnSpan. Pack spanning cells into rows
    // while the plain path below stays byte-identical when none declares it.
    if kids.iter().any(|c| c.props.column_span.is_some()) {
        let width_of = |p: i64| col_w + if p < col_rem { 1 } else { 0 };
        // (row, child index, logical start column, spanned columns)
        let mut cells: Vec<(usize, usize, i64, i64)> = Vec::new();
        let mut row = 0usize;
        let mut cursor = 0i64;
        for (i, c) in kids.iter().enumerate() {
            let span = c.props.column_span.unwrap_or(1).clamp(1, cols);
            if cursor + span > cols {
                row += 1;
                cursor = 0;
            }
            cells.push((row, i, cursor, span));
            cursor += span;
        }
        let rcount = row + 1;
        let mut heights = vec![0i64; rcount];
        for &(r, i, _, _) in &cells {
            heights[r] = heights[r].max(natural_h(kids[i], bp, state));
        }
        let mut y = content.y;
        for r in 0..rcount {
            let h = heights[r];
            for &(rr, i, start, span) in &cells {
                if rr != r {
                    continue;
                }
                let p_start = if rtl { cols - start - span } else { start };
                let mut x = content.x;
                for p in 0..p_start {
                    x += width_of(p) + gutter;
                }
                let mut w = gutter.saturating_mul(span - 1);
                for p in p_start..p_start + span {
                    w += width_of(p);
                }
                let c = kids[i];
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
                    rtl,
                    n.props.align,
                    elapsed_ms,
                    clip,
                    clip_shape.clone(),
                    icons,
                    anim,
                    out,
                    cache.as_deref_mut(),
                );
            }
            y += h;
        }
        return;
    }
    let rows = (kids.len() as i64 + cols - 1) / cols;

    // Row heights: the tallest natural height in each row.
    let mut row_h = vec![0i64; rows as usize];
    for (i, c) in kids.iter().enumerate() {
        let row = (i as i64 / cols) as usize;
        row_h[row] = row_h[row].max(natural_h(c, bp, state));
    }
    // Opt-in fill: when the box is taller than the natural rows, spread the
    // surplus evenly in i128 so rounding matches on every host; the remainder
    // goes to the earliest rows. Unset keeps natural rows byte-identical.
    if n.props.fill_height && rows > 0 {
        let natural_total: i128 = row_h.iter().map(|&h| h as i128).sum();
        let surplus = content.h as i128 - natural_total;
        if surplus > 0 {
            let rows_i = rows as i128;
            let each = surplus / rows_i;
            let mut rem = surplus % rows_i;
            for h in row_h.iter_mut() {
                *h = (*h as i128 + each + if rem > 0 { 1 } else { 0 }) as i64;
                if rem > 0 {
                    rem -= 1;
                }
            }
        }
    }

    let mut y = content.y;
    for row in 0..rows as usize {
        // Content-driven: every row keeps its natural height even past the
        // grid box; the enclosing surface clip trims the overflow. Clamping a
        // row to the leftover height squeezes its fixed-height cells (badge,
        // meter) into overlapping slivers.
        let h = row_h[row];
        let mut x = content.x;
        for col in 0..cols {
            // RTL reverses column order within each row; row order and column
            // widths are unchanged. With one column this is a no-op.
            let src_col = if rtl { cols - 1 - col } else { col };
            let idx = row * cols as usize + src_col as usize;
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
                rtl,
                n.props.align,
                elapsed_ms,
                clip,
                clip_shape.clone(),
                icons,
                anim,
                out,
                cache.as_deref_mut(),
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
    rtl: bool,
    elapsed_ms: i64,
    padding: i64,
    clip_shape: crate::raster::ClipShapeSlot,
    icons: &std::collections::BTreeMap<String, crate::icons::IconPaths>,
    anim: &mut AnimTable,
    out: &mut Vec<LaidOut>,
    mut cache: Option<&mut crate::incremental::LayoutCache>,
) {
    let content = inset(rect, padding);
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
            rtl,
            n.props.align,
            elapsed_ms,
            clip,
            clip_shape.clone(),
            icons,
            anim,
            out,
            cache.as_deref_mut(),
        );
        y = y.saturating_add(h.saturating_add(gap));
    }
}

/// A `For` node lays its template out once per element of the `source` list
/// state, with the `item` name bound to that element in a scoped state view.
///
/// Rows follow array order (deterministic, no sorting or filtering); an empty
/// or missing list lays out nothing. Every instance shares the template's node
/// ids, so identical rows animate and diff identically.
#[allow(clippy::too_many_arguments)]
fn place_for(
    n: &Node,
    rect: Rect,
    bp: Breakpoint,
    pal: &Palette,
    state: &StateView,
    rtl: bool,
    elapsed_ms: i64,
    clip: Option<Rect>,
    clip_shape: crate::raster::ClipShapeSlot,
    padding: i64,
    icons: &std::collections::BTreeMap<String, crate::icons::IconPaths>,
    anim: &mut AnimTable,
    out: &mut Vec<LaidOut>,
    mut cache: Option<&mut crate::incremental::LayoutCache>,
) {
    let content = inset(rect, padding);
    let gap = n.props.gap.max(0);
    let elements = list_elements(n, state);

    // Lazy sliver window (DYNAMIC_LISTS.md): with a clip, only rows intersecting
    // the viewport plus a fixed overscan are materialized. The pre-pass computes
    // each row's y/h with integer arithmetic and a scoped view -- no place(), no
    // LaidOut, no glyph work -- while advancing y for every row, so visible rows
    // keep their exact scroll offset. `clip == None` places every row, keeping
    // the no-Scroll path and all dynamic-list goldens byte-identical.
    let mut spans: Vec<(i64, i64)> = Vec::with_capacity(elements.len());
    let mut y = content.y;
    for element in &elements {
        let mut scoped = state.clone();
        if let Some(name) = n.props.item.as_deref() {
            scoped.set(name, StateValue::Str(element.clone()));
        }
        for c in n.children.iter().filter(|c| visible(c, &scoped)) {
            let h = c.props.height.unwrap_or_else(|| natural_h(c, bp, &scoped));
            spans.push((y, h));
            y = y.saturating_add(h.saturating_add(gap));
        }
    }

    // Select the row-index range: first..last intersecting the clip, grown by
    // SLIVER_OVERSCAN rows on each side and clamped to the materialized rows.
    let (lo, hi) = if let Some(c) = clip {
        let win_top = c.y;
        let win_bottom = c.y.saturating_add(c.h);
        let mut first = None;
        let mut last = None;
        for (i, &(ry, rh)) in spans.iter().enumerate() {
            if ry < win_bottom && ry.saturating_add(rh) > win_top {
                if first.is_none() {
                    first = Some(i);
                }
                last = Some(i);
            }
        }
        match (first, last) {
            (Some(f), Some(l)) => (
                f.saturating_sub(SLIVER_OVERSCAN),
                (l + SLIVER_OVERSCAN).min(spans.len().saturating_sub(1)),
            ),
            _ => (spans.len(), spans.len()), // no row intersects: place nothing
        }
    } else {
        (0, spans.len().saturating_sub(1))
    };

    // Rewalk rows, placing only the selected range. y/h come straight from the
    // pre-pass, so a visible row's rect is byte-equal to the full-layout path.
    let mut row = 0usize;
    for element in &elements {
        let mut scoped = state.clone();
        if let Some(name) = n.props.item.as_deref() {
            scoped.set(name, StateValue::Str(element.clone()));
        }
        for c in n.children.iter().filter(|c| visible(c, &scoped)) {
            if row >= lo && row <= hi && row < spans.len() {
                let (sy, h) = spans[row];
                let w = c
                    .props
                    .width
                    .unwrap_or_else(|| natural_w(c, bp, &scoped))
                    .min(content.w)
                    .max(0);
                let child_rect = inset(
                    Rect {
                        x: content.x,
                        y: sy,
                        w,
                        h,
                    },
                    c.props.margin,
                );
                let before = out.len();
                place(
                    c,
                    child_rect,
                    bp,
                    pal,
                    &scoped,
                    rtl,
                    n.props.align,
                    elapsed_ms,
                    clip,
                    clip_shape.clone(),
                    icons,
                    anim,
                    out,
                    cache.as_deref_mut(),
                );
                // Runtime row identity pairs the template node id with this
                // element's value, index-independent (inserting a row reuses
                // surviving rows). Apply it to the row root and its subtree.
                for laid in &mut out[before..] {
                    if let Some(id) = laid.incremental.identity.as_mut() {
                        id.row_key = Some(element.clone());
                    }
                }
            }
            row += 1;
        }
    }
}
/// Current elements of the `For` node's `source` list state; a missing source
/// or a non-list value yields no elements (zero rows, never an error).
fn list_elements(n: &Node, state: &StateView) -> Vec<String> {
    let source = n.props.source.as_deref().unwrap_or("");
    match state.get(source) {
        Some(StateValue::List(list)) => list.clone(),
        _ => Vec::new(),
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
    rtl: bool,
    elapsed_ms: i64,
    clip: Option<Rect>,
    clip_shape: crate::raster::ClipShapeSlot,
    padding: i64,
    icons: &std::collections::BTreeMap<String, crate::icons::IconPaths>,
    anim: &mut AnimTable,
    out: &mut Vec<LaidOut>,
    mut cache: Option<&mut crate::incremental::LayoutCache>,
) {
    let content = inset(rect, padding);
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
            rtl,
            n.props.align,
            elapsed_ms,
            clip,
            clip_shape.clone(),
            icons,
            anim,
            out,
            cache.as_deref_mut(),
        );
        y = y.saturating_add(h.saturating_add(gap));
    }
}
