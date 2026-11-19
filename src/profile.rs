//! Deterministic render profiles used by the CLI.
//!
//! The product profile is the default and selects the highest supported
//! fidelity: geometric edge coverage, proportional font metrics on the
//! high-resolution glyph atlas, motion, and DPR-aware rasterization. The compat
//! profile pins the historical byte output used by the golden fixtures; it is
//! entered only by an explicit `--quality compat` or `--legacy-golden` flag.
//!
//! All choices are plain data. No ambient state, host query or wall clock is
//! consulted, so the same profile and IR always produce the same frame.

use crate::anim::Curve;
use crate::font::FontMetrics;
use crate::ir::Node;

/// Logical pixel ratio unit: `DPI_UNIT = 1024` means 1.0, matching the DPR
/// fixed-point convention in `opurom-spec/DESKTOP_PARITY.md`.
pub const DPI_UNIT: i64 = 1024;
/// Default device pixel ratio (1x). Preserves the pinned framebuffer size.
pub const DEFAULT_DPR: i64 = DPI_UNIT;
/// Largest integer DPR the deterministic raster accepts.
pub const MAX_DPR: i64 = 4 * DPI_UNIT;

/// Which end of the fidelity spectrum a render uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderProfile {
    /// Highest product specification. Default for the CLI.
    Product,
    /// Historical byte-exact output. Explicit opt-in only.
    Compat,
}

impl RenderProfile {
    pub fn parse(s: &str) -> Option<RenderProfile> {
        match s {
            "product" | "full" => Some(RenderProfile::Product),
            "compat" | "legacy-golden" => Some(RenderProfile::Compat),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            RenderProfile::Product => "product",
            RenderProfile::Compat => "compat",
        }
    }
}

impl Default for RenderProfile {
    fn default() -> Self {
        RenderProfile::Product
    }
}

/// Motion tokens selected by the product profile. Durations come from Flutter
/// 3.50.0 `packages/flutter/lib/src/material/motion.dart:28/60/192`
/// (`Durations.short1`, `Durations.medium1`, standard `Cubic(0.2, 0, 0, 1)`).
/// The `M3Short1` / `M3Medium1` curves carry the same easing in the renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MotionSpec {
    pub enabled: bool,
    /// Control/state transition duration in milliseconds.
    pub state_ms: i64,
    /// Panel/page transition duration in milliseconds.
    pub panel_ms: i64,
    /// Curve used when a node declares `animate` without an explicit one.
    pub curve: Curve,
}

impl MotionSpec {
    pub fn product() -> MotionSpec {
        MotionSpec {
            enabled: true,
            state_ms: crate::anim::M3_SHORT1_MS,
            panel_ms: crate::anim::M3_MEDIUM1_MS,
            curve: Curve::M3Medium1,
        }
    }

    pub fn disabled() -> MotionSpec {
        MotionSpec {
            enabled: false,
            ..MotionSpec::product()
        }
    }
}

impl Default for MotionSpec {
    fn default() -> Self {
        MotionSpec::product()
    }
}

/// One resolved set of rendering defaults. An explicit CLI flag replaces the
/// corresponding field after the profile baseline is selected; it never
/// changes the profile choice itself unless it is one of the profile flags.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderPolicy {
    pub profile: RenderProfile,
    /// 4x geometric edge coverage of rounded/path edges.
    pub edge_aa: bool,
    /// Theme-level font metrics applied to nodes without their own prop.
    pub font_metrics: FontMetrics,
    /// Motion tokens; `enabled == false` is `--no-motion`.
    pub motion: MotionSpec,
    /// Device pixel ratio in fixed point (`DPI_UNIT == 1x`).
    pub dpr: i64,
    /// `--low-power`: cap particle fields and drop expensive effect passes.
    pub low_power: bool,
    /// `--system-font`: skip bitmap text for a host-rendered system-font layer.
    pub system_font: bool,
}

impl RenderPolicy {
    /// Highest product fidelity. This is the CLI default.
    pub fn product() -> RenderPolicy {
        RenderPolicy {
            profile: RenderProfile::Product,
            edge_aa: true,
            font_metrics: FontMetrics::Proportional,
            motion: MotionSpec::product(),
            dpr: DEFAULT_DPR,
            low_power: false,
            system_font: false,
        }
    }

    /// Historical pinned defaults. Only reached through an explicit flag.
    pub fn compat() -> RenderPolicy {
        RenderPolicy {
            profile: RenderProfile::Compat,
            edge_aa: false,
            font_metrics: FontMetrics::Fixed,
            motion: MotionSpec::product(),
            dpr: DEFAULT_DPR,
            low_power: false,
            system_font: false,
        }
    }

    pub fn for_profile(profile: RenderProfile) -> RenderPolicy {
        match profile {
            RenderProfile::Product => RenderPolicy::product(),
            RenderProfile::Compat => RenderPolicy::compat(),
        }
    }

    /// True when any low-fidelity choice was made, whether by profile or flag.
    pub fn is_degraded(&self) -> bool {
        self.profile == RenderProfile::Compat
            || !self.edge_aa
            || !self.motion.enabled
            || self.low_power
            || self.dpr < DEFAULT_DPR
    }

    /// One deterministic report line for `--low-power` / `--no-motion`.
    /// Auto-degrade reports its own `tier=` line; this line is never printed on
    /// the default product path, so the standard stdout stays `digest`, `dirty`.
    pub fn report_line(&self) -> String {
        format!(
            "profile={} edge_aa={} font_metrics={} motion={} low_power={} dpr={}",
            self.profile.name(),
            if self.edge_aa { "on" } else { "off" },
            match self.font_metrics {
                FontMetrics::Fixed => "fixed",
                FontMetrics::Proportional => "proportional",
            },
            if self.motion.enabled { "on" } else { "off" },
            if self.low_power { "on" } else { "off" },
            self.dpr,
        )
    }
}

/// Strip authored animation from a tree. `--no-motion` is explicit and only
/// affects the profile that selects it; settled frames are unchanged because a
/// settled animation already equals its target.
pub fn disable_motion(node: &mut Node) {
    node.props.animate = Curve::None;
    for child in &mut node.children {
        disable_motion(child);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_defaults_are_the_highest_fidelity() {
        let p = RenderPolicy::product();
        assert_eq!(p.profile, RenderProfile::Product);
        assert!(p.edge_aa);
        assert_eq!(p.font_metrics, FontMetrics::Proportional);
        assert!(p.motion.enabled);
        assert_eq!(p.dpr, DPI_UNIT);
        assert!(!p.low_power);
        assert!(!p.is_degraded());
    }

    #[test]
    fn compat_defaults_are_pinned_and_degraded() {
        let c = RenderPolicy::compat();
        assert_eq!(c.profile, RenderProfile::Compat);
        assert!(!c.edge_aa);
        assert_eq!(c.font_metrics, FontMetrics::Fixed);
        assert!(c.is_degraded());
    }

    #[test]
    fn profile_names_round_trip() {
        for name in ["product", "compat", "legacy-golden", "full"] {
            let p = RenderProfile::parse(name).unwrap();
            assert_eq!(RenderProfile::parse(p.name()), Some(p));
        }
        assert_eq!(RenderProfile::parse("slow"), None);
    }

    #[test]
    fn report_line_is_stable_and_names_every_choice() {
        assert_eq!(
            RenderPolicy::product().report_line(),
            "profile=product edge_aa=on font_metrics=proportional motion=on low_power=off dpr=1024"
        );
        let muted = RenderPolicy {
            low_power: true,
            motion: MotionSpec::disabled(),
            edge_aa: false,
            ..RenderPolicy::product()
        };
        assert_eq!(
            muted.report_line(),
            "profile=product edge_aa=off font_metrics=proportional motion=off low_power=on dpr=1024"
        );
    }

    #[test]
    fn motion_tokens_match_flutter_sources() {
        let m = MotionSpec::product();
        assert_eq!(m.state_ms, crate::anim::M3_SHORT1_MS);
        assert_eq!(m.panel_ms, crate::anim::M3_MEDIUM1_MS);
        assert_eq!(m.curve, Curve::M3Medium1);
        assert!(!MotionSpec::disabled().enabled);
    }
}
