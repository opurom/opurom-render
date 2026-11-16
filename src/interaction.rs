//! Five-state interaction matrix and design-system variants/tones.
//!
//! Every transform is integer-only. The caller may override a node's authored
//! state (the CLI does this for state-matrix screenshots), but an authored
//! `disabled` prop always resolves to `NodeState::Disabled`.

use crate::{Kind, LaidOut, Rgba};

/// Thickness of the focus ring painted outside a focused node's rect.
pub const FOCUS_RING: i64 = 2;
/// Fixed-point unit used by opacity and colour blends.
const UNIT: i64 = 1024;
const WHITE: Rgba = Rgba::rgb(0xff, 0xff, 0xff);

// Measured from the default UI reference PNGs. Existing theme accents remain
// untouched; these values are defaults for the new variant/tone props only.
pub const PRIMARY: Rgba = Rgba::rgb(0x0a, 0x71, 0xfc);
pub const CYAN: Rgba = Rgba::rgb(0x0d, 0xb2, 0xff);
pub const SUCCESS: Rgba = Rgba::rgb(0x1f, 0xb1, 0x4c);
pub const WARNING: Rgba = Rgba::rgb(0xfd, 0x96, 0x0e);
pub const ERROR: Rgba = Rgba::rgb(0xf6, 0x2b, 0x35);
pub const INFO: Rgba = PRIMARY;
pub const DEEP_INK: Rgba = Rgba::rgb(0x13, 0x2c, 0x5a);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NodeState {
    #[default]
    Default,
    Hover,
    Active,
    Focus,
    Disabled,
}

impl NodeState {
    pub fn parse(s: &str) -> Option<NodeState> {
        match s {
            "default" => Some(NodeState::Default),
            "hover" => Some(NodeState::Hover),
            "active" => Some(NodeState::Active),
            "focus" => Some(NodeState::Focus),
            "disabled" => Some(NodeState::Disabled),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            NodeState::Default => "default",
            NodeState::Hover => "hover",
            NodeState::Active => "active",
            NodeState::Focus => "focus",
            NodeState::Disabled => "disabled",
        }
    }

    /// A caller-supplied state overrides the authored state; `disabled` wins
    /// over both. The resolution has no runtime or host-dependent inputs.
    pub fn resolve(caller: Option<NodeState>, authored: NodeState, disabled: bool) -> NodeState {
        if disabled {
            NodeState::Disabled
        } else {
            caller.unwrap_or(authored)
        }
    }

    /// State paint transform. Focus deliberately leaves the colours alone: its
    /// visible change is the exact outer ring drawn in the central box path.
    pub fn color(self, c: Rgba) -> Rgba {
        match self {
            NodeState::Default | NodeState::Focus => c,
            NodeState::Hover => Rgba {
                r: lighten(c.r),
                g: lighten(c.g),
                b: lighten(c.b),
                a: c.a,
            },
            NodeState::Active => Rgba {
                r: darken(c.r),
                g: darken(c.g),
                b: darken(c.b),
                a: c.a,
            },
            NodeState::Disabled => desaturate(c),
        }
    }

    /// Fixed-point alpha reduction for disabled nodes.
    pub fn opacity(self, alpha: i64) -> i64 {
        match self {
            NodeState::Disabled => alpha.max(0).saturating_mul(3) / 5,
            _ => alpha,
        }
    }
}

fn lighten(v: u8) -> u8 {
    (v as i64 + (255 - v as i64) * 3 / 16).clamp(0, 255) as u8
}

fn darken(v: u8) -> u8 {
    (v as i64 * 13 / 16).clamp(0, 255) as u8
}

fn desaturate(c: Rgba) -> Rgba {
    let l = (c.r as i64 * 299 + c.g as i64 * 587 + c.b as i64 * 114) / 1000;
    let gray = |v: u8| ((v as i64 + 2 * l) / 3).clamp(0, 255) as u8;
    Rgba {
        r: gray(c.r),
        g: gray(c.g),
        b: gray(c.b),
        a: (c.a as i64 * 3 / 5).clamp(0, 255) as u8,
    }
}

fn mix(a: Rgba, b: Rgba, t: i64) -> Rgba {
    let t = t.clamp(0, UNIT);
    let channel = |x: u8, y: u8| {
        (((y as i64 - x as i64) * t + UNIT / 2) / UNIT + x as i64).clamp(0, 255) as u8
    };
    Rgba {
        r: channel(a.r, b.r),
        g: channel(a.g, b.g),
        b: channel(a.b, b.b),
        a: channel(a.a, b.a),
    }
}

/// Extra-dark ink used for the 1 px pressed inset stroke.
pub fn press_color(c: Rgba) -> Rgba {
    Rgba {
        r: (c.r as i64 * 3 / 4).clamp(0, 255) as u8,
        g: (c.g as i64 * 3 / 4).clamp(0, 255) as u8,
        b: (c.b as i64 * 3 / 4).clamp(0, 255) as u8,
        a: c.a,
    }
}

/// Caller-owned press phase (ANIMATION_AND_PHYSICS.md §8). A runtime value that
/// never enters the IR or state JSON. `touch` is in logical pixels relative to
/// the pressed node; `None` makes the ripple centre the node rect centre.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PressPhase {
    pub node_id: u32,
    pub press_start_tick: i64,
    pub touch: Option<(i64, i64)>,
}

/// Ripple timing in milliseconds. Structural constants held here until the
/// material token table lands; callers may override duration per primitive.
/// The value is the framework default from the medium motion token (§7).
pub const RIPPLE_DURATION_MS: i64 = 220;
/// Peak alpha of a fresh ripple, in fixed-point `UNIT` units.
pub const RIPPLE_PEAK_ALPHA: i64 = 256; // 25% opacity

/// Normalized ripple progress at `elapsed_ms` since the press, in `0..=UNIT`.
/// Before the press zero, after the duration exactly `UNIT`; integer only.
pub fn ripple_progress(elapsed_ms: i64, duration_ms: i64) -> i64 {
    let d = duration_ms.max(1);
    (elapsed_ms.clamp(0, d) as i128 * UNIT as i128 / d as i128) as i64
}

/// Ripple radius at `progress`: grows linearly to `max_radius`, integer fixed
/// point with half-up rounding so small radii do not stall at zero.
pub fn ripple_radius(progress: i64, max_radius: i64) -> i64 {
    ((progress as i128 * max_radius as i128 + UNIT as i128 / 2) / UNIT as i128) as i64
}

/// Ripple alpha at `progress`: fades from `peak` to zero as the circle grows.
pub fn ripple_alpha(progress: i64, peak: i64) -> i64 {
    (peak as i128 * (UNIT - progress) as i128 / UNIT as i128).clamp(0, UNIT as i128) as i64
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    Primary,
    Secondary,
    Text,
    Danger,
    Ghost,
    Fab,
}

impl Variant {
    pub fn parse(s: &str) -> Option<Variant> {
        match s {
            "primary" => Some(Variant::Primary),
            "secondary" => Some(Variant::Secondary),
            "text" => Some(Variant::Text),
            "danger" => Some(Variant::Danger),
            "ghost" => Some(Variant::Ghost),
            "fab" => Some(Variant::Fab),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Variant::Primary => "primary",
            Variant::Secondary => "secondary",
            Variant::Text => "text",
            Variant::Danger => "danger",
            Variant::Ghost => "ghost",
            Variant::Fab => "fab",
        }
    }

    pub fn fill(self) -> Option<Rgba> {
        match self {
            Variant::Primary => Some(PRIMARY),
            Variant::Secondary => Some(Rgba { a: 0, ..WHITE }),
            Variant::Text => None,
            Variant::Danger => Some(ERROR),
            Variant::Ghost => Some(mix(WHITE, CYAN, UNIT / 8)),
            Variant::Fab => Some(CYAN),
        }
    }

    pub fn border(self) -> Option<Rgba> {
        match self {
            Variant::Secondary => Some(PRIMARY),
            _ => None,
        }
    }

    pub fn border_w(self) -> i64 {
        match self {
            Variant::Secondary => 1,
            _ => 0,
        }
    }

    pub fn text_color(self) -> Rgba {
        match self {
            Variant::Primary | Variant::Danger | Variant::Fab => WHITE,
            Variant::Secondary | Variant::Ghost => DEEP_INK,
            Variant::Text => PRIMARY,
        }
    }

    pub fn is_circular(self) -> bool {
        self == Variant::Fab
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Success,
    Warning,
    Error,
    Info,
    Neutral,
}

impl Tone {
    pub fn parse(s: &str) -> Option<Tone> {
        match s {
            "success" => Some(Tone::Success),
            "warning" => Some(Tone::Warning),
            "error" => Some(Tone::Error),
            "info" => Some(Tone::Info),
            "neutral" => Some(Tone::Neutral),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Tone::Success => "success",
            Tone::Warning => "warning",
            Tone::Error => "error",
            Tone::Info => "info",
            Tone::Neutral => "neutral",
        }
    }

    pub fn color(self) -> Rgba {
        match self {
            Tone::Success => SUCCESS,
            Tone::Warning => WARNING,
            Tone::Error => ERROR,
            Tone::Info => INFO,
            Tone::Neutral => DEEP_INK,
        }
    }
}

/// Kinds the CLI state-matrix flag addresses. Pure layout containers and
/// static text/image/spacer nodes are excluded.
pub fn is_interactive(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Button
            | Kind::Input
            | Kind::Icon
            | Kind::Badge
            | Kind::Progress
            | Kind::Switch
            | Kind::Slider
            | Kind::Checkbox
            | Kind::Radio
            | Kind::Tabs
            | Kind::Avatar
            | Kind::Spinner
            | Kind::Table
            | Kind::Chart
            | Kind::Calendar
            | Kind::RichText
            | Kind::Player
            | Kind::Map
            | Kind::Menu
            | Kind::Tooltip
            | Kind::Extended(_)
    )
}

/// Apply one resolved state to every colour and opacity the raster can read.
/// A default-state node is never cloned by `raster::draw`, so old frames stay
/// byte-identical.
pub fn style_node(n: &LaidOut) -> LaidOut {
    let mut out = n.clone();
    let paint = |c: Rgba| n.state.color(c);
    out.fill = out.fill.map(paint);
    out.border = out.border.map(paint);
    out.text_color = out.text_color.map(paint);
    out.accent = paint(out.accent);
    out.on_accent = paint(out.on_accent);
    out.tone_color = out.tone_color.map(paint);
    out.effects.gradient = out.effects.gradient.map(|(a, b)| (paint(a), paint(b)));
    out.effects.opacity = n.state.opacity(out.effects.opacity);
    for effect in &mut out.visual_effects {
        effect.color = effect.color.map(paint);
        effect.alpha = n.state.opacity(effect.alpha);
    }
    if let Some(particles) = out.particles.as_mut() {
        particles.color = particles.color.map(paint);
        particles.alpha = n.state.opacity(particles.alpha);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_and_resolution_is_disabled_first() {
        for (text, state) in [
            ("default", NodeState::Default),
            ("hover", NodeState::Hover),
            ("active", NodeState::Active),
            ("focus", NodeState::Focus),
            ("disabled", NodeState::Disabled),
        ] {
            assert_eq!(NodeState::parse(text), Some(state));
            assert_eq!(state.name(), text);
        }
        assert_eq!(NodeState::parse("pressed"), None);
        assert_eq!(
            NodeState::resolve(Some(NodeState::Hover), NodeState::Focus, false),
            NodeState::Hover
        );
        assert_eq!(
            NodeState::resolve(Some(NodeState::Hover), NodeState::Focus, true),
            NodeState::Disabled
        );
        assert_eq!(
            NodeState::resolve(None, NodeState::Active, false),
            NodeState::Active
        );
    }

    #[test]
    fn variant_and_tone_names_round_trip() {
        for text in ["primary", "secondary", "text", "danger", "ghost", "fab"] {
            assert_eq!(Variant::parse(text).unwrap().name(), text);
        }
        for text in ["success", "warning", "error", "info", "neutral"] {
            assert_eq!(Tone::parse(text).unwrap().name(), text);
        }
        assert_eq!(Variant::parse("link"), None);
        assert_eq!(Tone::parse("caution"), None);
    }

    #[test]
    fn state_colour_rules_are_exact_integers() {
        let c = Rgba::rgb(0x10, 0x20, 0x30);
        assert_eq!(NodeState::Hover.color(c), Rgba::rgb(0x3c, 0x49, 0x56));
        assert_eq!(NodeState::Active.color(c), Rgba::rgb(0x0d, 0x1a, 0x27));
        assert_eq!(
            NodeState::Disabled.color(c),
            Rgba {
                r: 0x18,
                g: 0x1e,
                b: 0x23,
                a: 153
            }
        );
        assert_eq!(NodeState::Disabled.opacity(UNIT), 614);
    }
}
