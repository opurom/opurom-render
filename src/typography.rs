//! Product typography roles and metrics contract.
//!
//! This module defines a lowered, deterministic typography contract. It does
//! not interpret IR at runtime and does not import sibling repositories.
//! Compat keeps fixed metrics and the bitmap fallback; Product selects
//! proportional metrics and system-font overlay as the default.

use crate::quality::QualityProfile;

/// Product text roles. Roles carry no runtime behavior: they are lowered by
/// the compiler into concrete IR properties for every target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextRole {
    Display,
    Headline,
    Title,
    Body,
    Label,
    Caption,
}

impl TextRole {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Display => "display",
            Self::Headline => "headline",
            Self::Title => "title",
            Self::Body => "body",
            Self::Label => "label",
            Self::Caption => "caption",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "display" => Some(Self::Display),
            "headline" => Some(Self::Headline),
            "title" => Some(Self::Title),
            "body" => Some(Self::Body),
            "label" => Some(Self::Label),
            "caption" => Some(Self::Caption),
            _ => None,
        }
    }

    /// Default size in logical pixels before scale and DPR are applied.
    pub const fn font_size(self) -> i64 {
        match self {
            Self::Display => 48,
            Self::Headline => 32,
            Self::Title => 24,
            Self::Body => 16,
            Self::Label => 14,
            Self::Caption => 12,
        }
    }

    /// Line height in 1024 fixed-point units.
    pub const fn line_height(self) -> i64 {
        match self {
            Self::Display => 1152,
            Self::Headline => 1229,
            Self::Title => 1280,
            Self::Body => 1408,
            Self::Label => 1408,
            Self::Caption => 1331,
        }
    }

    /// Positive values widen tracking, negative values tighten it.
    pub const fn tracking(self) -> i64 {
        match self {
            Self::Display => -12,
            Self::Headline => -6,
            Self::Title => 0,
            Self::Body => 0,
            Self::Label => 32,
            Self::Caption => 48,
        }
    }

    /// Minimum touch target in logical pixels. Purely informational for
    /// non-interactive roles; visual gates use it for interactive nodes.
    pub const fn min_touch_target(self) -> i64 {
        match self {
            Self::Caption => 32,
            _ => 44,
        }
    }

    /// Color slot name consumed by the design-system token resolver.
    pub const fn color_slot(self) -> &'static str {
        match self {
            Self::Body | Self::Caption => "muted",
            _ => "ink",
        }
    }
}

/// Fully lowered typography setting for one text span.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypographySpec {
    pub role: TextRole,
    pub font_size: i64,
    pub line_height: i64,
    pub tracking: i64,
    pub weight: i64,
    pub baseline_fixed: i64,
    pub color_slot: &'static str,
    pub min_touch_target: i64,
}

impl TypographySpec {
    pub fn from_role(role: TextRole) -> Self {
        Self {
            role,
            font_size: role.font_size(),
            line_height: role.line_height(),
            tracking: role.tracking(),
            weight: 400,
            baseline_fixed: 1024,
            color_slot: role.color_slot(),
            min_touch_target: role.min_touch_target(),
        }
    }

    /// Lower a role under a profile. Compat forces the historical 16px fixed
    /// metric so golden digests cannot drift. Product uses the role values.
    pub fn lowered(role: TextRole, profile: QualityProfile) -> Self {
        match profile {
            QualityProfile::Compat => Self {
                font_size: 16,
                line_height: 1024,
                tracking: 0,
                weight: 400,
                baseline_fixed: 1024,
                ..Self::from_role(role)
            },
            QualityProfile::Product => Self::from_role(role),
        }
    }

    /// Apply an explicit authored size without changing the lowered role
    /// metrics. The caller owns validation; this is a pure integer transform.
    pub fn with_font_size(mut self, size: i64) -> Self {
        if size > 0 {
            self.font_size = size;
        }
        self
    }
}

/// Quality-driven font selection. Compat may use the bitmap font; Product
/// prefers an explicit family, then the system font overlay, then the
/// high-quality coverage atlas, and never the bitmap fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontSource {
    BitmapFallback,
    CoverageAtlas,
    SystemOverlay,
    ExplicitFamily,
}

pub fn resolve_font_source(profile: QualityProfile, explicit_family: Option<&str>) -> FontSource {
    match profile {
        QualityProfile::Compat => FontSource::BitmapFallback,
        QualityProfile::Product => match explicit_family {
            Some(family) if !family.is_empty() => FontSource::ExplicitFamily,
            Some(_) | None => FontSource::SystemOverlay,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_font_source, FontSource, TextRole, TypographySpec};
    use crate::QualityProfile;

    #[test]
    fn all_roles_parse_and_have_monotone_sizes() {
        let roles = [
            TextRole::Caption,
            TextRole::Label,
            TextRole::Body,
            TextRole::Title,
            TextRole::Headline,
            TextRole::Display,
        ];
        for role in roles {
            assert_eq!(TextRole::parse(role.name()), Some(role));
            assert!(role.font_size() > 0);
            assert!(role.line_height() > 0);
        }
        for pair in roles.windows(2) {
            assert!(pair[0].font_size() < pair[1].font_size());
        }
    }

    #[test]
    fn compat_lowering_preserves_historical_metrics() {
        let spec = TypographySpec::lowered(TextRole::Display, QualityProfile::Compat);
        assert_eq!(spec.font_size, 16);
        assert_eq!(spec.line_height, 1024);
        assert_eq!(spec.tracking, 0);
        assert_eq!(
            resolve_font_source(QualityProfile::Compat, Some("Any")),
            FontSource::BitmapFallback
        );
    }

    #[test]
    fn product_lowering_preserves_role_hierarchy() {
        let body = TypographySpec::lowered(TextRole::Body, QualityProfile::Product);
        let display = TypographySpec::lowered(TextRole::Display, QualityProfile::Product);
        assert_eq!(body.font_size, 16);
        assert_eq!(display.font_size, 48);
        assert!(display.line_height > 0 && display.tracking < 0);
        assert_eq!(
            resolve_font_source(QualityProfile::Product, None),
            FontSource::SystemOverlay
        );
        assert_eq!(
            resolve_font_source(QualityProfile::Product, Some("Custom")),
            FontSource::ExplicitFamily
        );
    }

    #[test]
    fn authored_size_is_positive_only() {
        let spec = TypographySpec::from_role(TextRole::Body).with_font_size(22);
        assert_eq!(spec.font_size, 22);
        assert_eq!(spec.with_font_size(-1).font_size, 22);
    }
}
