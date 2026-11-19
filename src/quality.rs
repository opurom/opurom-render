//! Frozen visual quality contract shared by renderer targets through IR/ABI only.
//!
//! Compat preserves the historical golden byte stream. Product enables the
//! visual features that make an application look finished: analytic edge
//! coverage, proportional typography hints, system-font overlay, effects,
//! app-level motion and DPR-aware raster metrics. This module is a pure
//! contract; it neither interprets IR nor imports sibling repositories.

/// Stable display-list quality field. Values are intentionally small integers
/// so every target encoder can lower them without a runtime parser.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QualityProfile {
    Compat,
    Product,
}

impl QualityProfile {
    /// Product is the constructor default. The normal application path is
    /// always the highest visual specification; Compat is an explicit
    /// downgrade used by digest regression tests and low-dependency fallback.
    pub const fn default() -> Self {
        Self::Product
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Compat => "compat",
            Self::Product => "product",
        }
    }

    /// Stable wire value written to display-list `quality` and consumed by
    /// GPU/web/native encoders through the frozen contract.
    pub const fn wire(self) -> u8 {
        match self {
            Self::Compat => 0,
            Self::Product => 1,
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "compat" => Some(Self::Compat),
            "product" => Some(Self::Product),
            _ => None,
        }
    }

    /// Resolve an explicit profile with legacy boolean overrides. A legacy
    /// override can raise Compat to Product for one visual capability, but it
    /// cannot downgrade an explicit Product profile.
    pub fn resolve(explicit: Option<Self>, edge_aa: bool, proportional_metrics: bool) -> Self {
        match explicit {
            Some(Self::Product) => Self::Product,
            Some(Self::Compat) if edge_aa || proportional_metrics => Self::Product,
            Some(Self::Compat) => Self::Compat,
            None => Self::Product,
        }
    }

    pub const fn edge_coverage(self) -> bool {
        matches!(self, Self::Product)
    }

    pub const fn proportional_metrics(self) -> bool {
        matches!(self, Self::Product)
    }

    pub const fn system_font_overlay(self) -> bool {
        matches!(self, Self::Product)
    }

    pub const fn effects(self) -> bool {
        matches!(self, Self::Product)
    }

    pub const fn app_motion(self) -> bool {
        matches!(self, Self::Product)
    }

    /// Product raster is DPR-aware. The scale is a caller-owned integer fixed
    /// point value (1024 == 1.0) so it stays deterministic and target-neutral.
    pub const fn dpr_fixed(self) -> i64 {
        match self {
            Self::Compat => 1024,
            Self::Product => 2048,
        }
    }

    /// Product targets may use perceptual comparison; compat remains byte
    /// comparable. This returns a stable name for visual gate manifests.
    pub const fn comparison(self) -> &'static str {
        match self {
            Self::Compat => "bit-exact",
            Self::Product => "perceptual",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::QualityProfile;

    #[test]
    fn product_is_the_normal_default() {
        assert_eq!(QualityProfile::default(), QualityProfile::Product);
        assert!(!QualityProfile::Compat.edge_coverage());
        assert!(!QualityProfile::Compat.proportional_metrics());
        assert_eq!(QualityProfile::Compat.wire(), 0);
    }

    #[test]
    fn product_enables_the_full_visual_contract() {
        let p = QualityProfile::Product;
        assert!(p.edge_coverage());
        assert!(p.proportional_metrics());
        assert!(p.system_font_overlay());
        assert!(p.effects());
        assert!(p.app_motion());
        assert_eq!(p.dpr_fixed(), 2048);
        assert_eq!(p.comparison(), "perceptual");
    }

    #[test]
    fn parsing_and_legacy_overrides_are_stable() {
        assert_eq!(
            QualityProfile::parse("compat"),
            Some(QualityProfile::Compat)
        );
        assert_eq!(
            QualityProfile::parse("product"),
            Some(QualityProfile::Product)
        );
        assert_eq!(QualityProfile::parse("fast"), None);
        assert_eq!(
            QualityProfile::resolve(None, false, false),
            QualityProfile::Product
        );
        assert_eq!(
            QualityProfile::resolve(Some(QualityProfile::Compat), true, false),
            QualityProfile::Product
        );
        assert_eq!(
            QualityProfile::resolve(Some(QualityProfile::Product), false, false),
            QualityProfile::Product
        );
    }
}

/// Product DPR-aware raster gate. Compat keeps the logical 1:1 bitmap so
/// existing goldens stay byte-identical; Product renders at the supplied
/// integer device pixel ratio and lets callers map logical coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DprScale {
    /// Device pixels per logical pixel, 1024 fixed point. 1024 = 1x, 2048 = 2x.
    pub fixed: i64,
}

impl DprScale {
    pub const fn from_profile(profile: QualityProfile) -> Self {
        Self {
            fixed: profile.dpr_fixed(),
        }
    }

    pub const fn from_ratio(ratio_x1024: i64) -> Self {
        Self {
            fixed: if ratio_x1024 < 1024 {
                1024
            } else {
                ratio_x1024
            },
        }
    }

    /// Map a logical length to device pixels, rounding half up.
    pub const fn scale(self, logical: i64) -> i64 {
        (logical * self.fixed + 512) / 1024
    }

    /// Map a device-pixel length back to logical pixels, rounding half up.
    pub const fn unscale(self, device: i64) -> i64 {
        (device * 1024 + self.fixed / 2) / self.fixed
    }

    /// Product gate: at least 2x, raster runs with analytic edge coverage.
    pub const fn requires_edge_coverage(self) -> bool {
        self.fixed >= 2048
    }
}

#[cfg(test)]
mod dpr_tests {
    use super::{DprScale, QualityProfile};

    #[test]
    fn compat_stays_at_logical_one_to_one() {
        let dpr = DprScale::from_profile(QualityProfile::Compat);
        assert_eq!(dpr.fixed, 1024);
        assert_eq!(dpr.scale(16), 16);
        assert_eq!(dpr.unscale(16), 16);
        assert!(!dpr.requires_edge_coverage());
    }

    #[test]
    fn product_defaults_to_two_x_and_rounds_half_up() {
        let dpr = DprScale::from_profile(QualityProfile::Product);
        assert_eq!(dpr.fixed, 2048);
        assert_eq!(dpr.scale(16), 32);
        assert_eq!(dpr.scale(5), 10);
        assert_eq!(dpr.unscale(32), 16);
        assert!(dpr.requires_edge_coverage());
    }

    #[test]
    fn explicit_ratios_clamp_to_the_logical_minimum() {
        assert_eq!(DprScale::from_ratio(0).fixed, 1024);
        assert_eq!(DprScale::from_ratio(3072).scale(10), 30);
        assert_eq!(DprScale::from_ratio(3072).unscale(30), 10);
    }
}
