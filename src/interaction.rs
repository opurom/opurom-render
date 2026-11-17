//! Eight-state component matrix and design-system variants/tones.
//!
//! Every transform is integer-only. The caller may override a node's authored
//! state (the CLI does this for state-matrix screenshots), but `disabled`
//! always wins. Business states (`loading`/`success`/`error`) mask interaction
//! feedback; their paint comes from the token constants below.

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
    /// Business state; Flutter has no equivalent (NO_FLUTTER_EQUIVALENT).
    Loading,
    /// Business state; Flutter has no equivalent (NO_FLUTTER_EQUIVALENT).
    Success,
    /// State-semantic value, distinct from the `tone=error` palette value.
    Error,
}

impl NodeState {
    pub fn parse(s: &str) -> Option<NodeState> {
        match s {
            "default" => Some(NodeState::Default),
            "hover" => Some(NodeState::Hover),
            "active" => Some(NodeState::Active),
            "focus" => Some(NodeState::Focus),
            "disabled" => Some(NodeState::Disabled),
            "loading" => Some(NodeState::Loading),
            "success" => Some(NodeState::Success),
            "error" => Some(NodeState::Error),
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
            NodeState::Loading => "loading",
            NodeState::Success => "success",
            NodeState::Error => "error",
        }
    }

    /// Chief priority: disabled > one business state > pressed > focused >
    /// hovered > default. Business states mask interaction feedback, so a
    /// business caller suppresses a hover/active/focus authored state and vice
    /// versa. Among two business states the caller wins (mutual exclusion).
    pub fn resolve(caller: Option<NodeState>, authored: NodeState, disabled: bool) -> NodeState {
        let priority = |s: NodeState| match s {
            NodeState::Default => 0,
            NodeState::Hover => 1,
            NodeState::Focus => 2,
            NodeState::Active => 3,
            NodeState::Loading | NodeState::Success | NodeState::Error => 4,
            NodeState::Disabled => 5,
        };
        if disabled {
            return NodeState::Disabled;
        }
        match caller {
            Some(c) => {
                if priority(c) >= priority(authored) {
                    c
                } else {
                    authored
                }
            }
            None => authored,
        }
    }

    /// State paint transform. Focus deliberately leaves the colours alone: its
    /// visible change is the exact outer ring drawn in the central box path.
    /// Business states use the token constants, never component literals.
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
            NodeState::Loading => CYAN,
            NodeState::Success => SUCCESS,
            NodeState::Error => ERROR,
            NodeState::Disabled => desaturate(c),
        }
    }

    /// Fixed-point alpha reduction for disabled nodes only.
    pub fn opacity(self, alpha: i64) -> i64 {
        match self {
            NodeState::Disabled => alpha.max(0).saturating_mul(3) / 5,
            NodeState::Default
            | NodeState::Hover
            | NodeState::Active
            | NodeState::Focus
            | NodeState::Loading
            | NodeState::Success
            | NodeState::Error => alpha,
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
    /// `None` means the pointer is still down (unconfirmed ripple); `Some(t)`
    /// is the caller-clock tick of release. The down tick stays in
    /// `press_start_tick` so the radius can continue from its held progress.
    pub confirm_tick: Option<i64>,
    pub touch: Option<(i64, i64)>,
}

/// Ripple timing, mirroring Flutter's `InkRipple` three controllers. All
/// durations are caller-clock milliseconds; no wall clock is consulted.
/// The unconfirmed radius grows slowly over one second while the pointer is
/// down; release accelerates it to the end over `RIPPLE_RADIUS_MS`, then the
/// alpha fades over the last 150 ms of `RIPPLE_FADE_OUT_MS`.
pub const RIPPLE_UNCONFIRMED_MS: i64 = 1000;
pub const RIPPLE_FADE_IN_MS: i64 = 75;
pub const RIPPLE_RADIUS_MS: i64 = 225;
pub const RIPPLE_FADE_OUT_MS: i64 = 375;
pub const RIPPLE_FADE_OUT_START_MS: i64 = RIPPLE_RADIUS_MS;
pub const RIPPLE_FADE_WINDOW_MS: i64 = RIPPLE_FADE_OUT_MS - RIPPLE_FADE_OUT_START_MS;
pub const RIPPLE_TOTAL_MS: i64 = RIPPLE_UNCONFIRMED_MS;
/// Legacy aggregate duration kept for the original primitive's callers/tests.
pub const RIPPLE_DURATION_MS: i64 = RIPPLE_FADE_OUT_MS;
/// Splash diameter starts at 60% of the target, so the radius starts at 30%.
pub const RIPPLE_START_PERCENT: i64 = 30;
/// Flutter ends the radius tween 5 logical pixels past the target radius.
pub const RIPPLE_END_EXTRA: i64 = 5;
/// Peak alpha of a fresh ripple, in fixed-point `UNIT` units.
pub const RIPPLE_PEAK_ALPHA: i64 = 256; // 25% opacity

/// Parameter units of the integer cubic Bezier used by `Curves.ease`.
const EASE_UNIT: i64 = 1 << 16;

/// Integer approximation of Flutter `Curves.ease` = Cubic(0.25, 0.1, 0.25, 1).
/// Inverts the curve's x coordinate by binary search, then reads y. Every
/// operation is integer fixed point; the result is in `0..=UNIT`.
fn ease(x: i64) -> i64 {
    let xt = (x.clamp(0, UNIT) as i128 * EASE_UNIT as i128 / UNIT as i128) as i64;
    let (p1x, p1y, p2x, p2y) = (EASE_UNIT / 4, EASE_UNIT / 10, EASE_UNIT / 4, EASE_UNIT);
    let (mut lo, mut hi) = (0i64, EASE_UNIT);
    for _ in 0..17 {
        let mid = (lo + hi) / 2;
        let (bx, _) =
            crate::curve::cubic_bezier((0, 0), (p1x, p1y), (p2x, p2y), (EASE_UNIT, EASE_UNIT), mid);
        if bx <= xt {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let (_, y) =
        crate::curve::cubic_bezier((0, 0), (p1x, p1y), (p2x, p2y), (EASE_UNIT, EASE_UNIT), lo);
    (y as i128 * UNIT as i128 / EASE_UNIT as i128) as i64
}

/// Floor integer square root, no float. `mid <= v / mid` avoids overflow.
fn isqrt(v: i128) -> i128 {
    if v <= 0 {
        return 0;
    }
    let (mut lo, mut hi) = (1i128, v);
    while lo < hi {
        let mid = lo + (hi - lo + 1) / 2;
        if mid <= v / mid {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo
}

/// Flutter `_getTargetRadius` (ink_ripple.dart:43): half the box diagonal, so
/// the splash reaches all four corners. Integer floor, no float.
pub fn ripple_target_radius(w: i64, h: i64) -> i64 {
    let w = w.max(0) as i128;
    let h = h.max(0) as i128;
    (isqrt(w * w + h * h) / 2) as i64
}

/// Radius endpoint for a target: Flutter draws up to `target + 5`.
fn ripple_end(max_radius: i64) -> i64 {
    max_radius.max(0) + RIPPLE_END_EXTRA
}

/// Radius while the pointer is still down: 30% to target+5 over one second.
pub fn ripple_radius_unconfirmed(elapsed_ms: i64, max_radius: i64) -> i64 {
    if elapsed_ms >= RIPPLE_UNCONFIRMED_MS {
        return ripple_end(max_radius);
    }
    let start = max_radius.max(0) * RIPPLE_START_PERCENT / 100;
    let x = elapsed_ms.max(0) as i128 * UNIT as i128 / RIPPLE_UNCONFIRMED_MS as i128;
    start + (ripple_end(max_radius) - start) * ease(x as i64) / UNIT
}

/// Alpha while the pointer is down: fade in over 75 ms, then hold at peak.
pub fn ripple_alpha_held(elapsed_ms: i64, peak: i64) -> i64 {
    if elapsed_ms <= 0 {
        return 0;
    }
    let a = elapsed_ms.min(RIPPLE_FADE_IN_MS) as i128 * peak as i128 / RIPPLE_FADE_IN_MS as i128;
    a.clamp(0, UNIT as i128) as i64
}

/// Radius after release: continue from the held controller value, accelerate
/// to target+5 in 225 ms. `release_ms` is the held duration at release.
pub fn ripple_radius_confirmed(release_ms: i64, since_ms: i64, max_radius: i64) -> i64 {
    if since_ms >= RIPPLE_RADIUS_MS {
        return ripple_end(max_radius);
    }
    let v0 = release_ms.clamp(0, RIPPLE_UNCONFIRMED_MS) as i128 * UNIT as i128
        / RIPPLE_UNCONFIRMED_MS as i128;
    let v = v0 + (UNIT as i128 - v0) * since_ms.max(0) as i128 / RIPPLE_RADIUS_MS as i128;
    let start = max_radius.max(0) * RIPPLE_START_PERCENT / 100;
    start + (ripple_end(max_radius) - start) * ease(v as i64) / UNIT
}

/// Alpha after release: hold peak for 225 ms, then fade to zero by 375 ms.
pub fn ripple_alpha_confirmed(since_ms: i64, peak: i64) -> i64 {
    if since_ms < RIPPLE_FADE_OUT_START_MS {
        return peak.clamp(0, UNIT);
    }
    if since_ms >= RIPPLE_FADE_OUT_MS {
        return 0;
    }
    let left = RIPPLE_FADE_OUT_MS - since_ms;
    (peak as i128 * left as i128 / RIPPLE_FADE_WINDOW_MS as i128).clamp(0, UNIT as i128) as i64
}

/// Resolve one frame of the ripple state machine: `None` means draw nothing.
pub fn ripple_frame(
    press: &PressPhase,
    now_ms: i64,
    max_radius: i64,
    peak: i64,
) -> Option<(i64, i64)> {
    let down = now_ms - press.press_start_tick;
    if down < 0 {
        return None;
    }
    let (radius, alpha) = match press.confirm_tick {
        None => (
            ripple_radius_unconfirmed(down, max_radius),
            ripple_alpha_held(down, peak),
        ),
        Some(confirm) => {
            let since = now_ms - confirm;
            if since < 0 {
                return None;
            }
            if since >= RIPPLE_FADE_OUT_MS {
                return None;
            }
            let release = confirm - press.press_start_tick;
            (
                ripple_radius_confirmed(release, since, max_radius),
                ripple_alpha_confirmed(since, peak),
            )
        }
    };
    if radius <= 0 || alpha <= 0 {
        None
    } else {
        Some((radius, alpha))
    }
}

/// Legacy linear progress retained for the original primitive callers.
pub fn ripple_progress(elapsed_ms: i64, duration_ms: i64) -> i64 {
    let d = duration_ms.max(1);
    (elapsed_ms.clamp(0, d) as i128 * UNIT as i128 / d as i128) as i64
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
            ("loading", NodeState::Loading),
            ("success", NodeState::Success),
            ("error", NodeState::Error),
        ] {
            assert_eq!(NodeState::parse(text), Some(state));
            assert_eq!(state.name(), text);
        }
        assert_eq!(NodeState::parse("pressed"), None);
        // Chief priority: focused outranks hovered, active outranks focus.
        assert_eq!(
            NodeState::resolve(Some(NodeState::Hover), NodeState::Focus, false),
            NodeState::Focus
        );
        assert_eq!(
            NodeState::resolve(Some(NodeState::Hover), NodeState::Focus, true),
            NodeState::Disabled
        );
        assert_eq!(
            NodeState::resolve(None, NodeState::Active, false),
            NodeState::Active
        );
        // A business state masks interaction feedback in either direction.
        assert_eq!(
            NodeState::resolve(Some(NodeState::Hover), NodeState::Loading, false),
            NodeState::Loading
        );
        assert_eq!(
            NodeState::resolve(Some(NodeState::Active), NodeState::Error, false),
            NodeState::Error
        );
        assert_eq!(
            NodeState::resolve(Some(NodeState::Success), NodeState::Hover, false),
            NodeState::Success
        );
        // Disabled outranks every business state.
        assert_eq!(
            NodeState::resolve(Some(NodeState::Error), NodeState::Success, true),
            NodeState::Disabled
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
        // Business paints come from tokens, not component literals.
        assert_eq!(NodeState::Loading.color(c), CYAN);
        assert_eq!(NodeState::Success.color(c), SUCCESS);
        assert_eq!(NodeState::Error.color(c), ERROR);
        for s in [
            NodeState::Default,
            NodeState::Hover,
            NodeState::Active,
            NodeState::Focus,
            NodeState::Loading,
            NodeState::Success,
            NodeState::Error,
        ] {
            assert_eq!(s.opacity(UNIT), UNIT);
        }
    }
}
