//! Target painter capability contract.
//!
//! Every product target lowers the same painter vocabulary from the frozen
//! quality, typography, recipe and motion contracts. The contract is a pure
//! description: no runtime IR parsing and no cross-repository source imports.
//! It exists so a web DOM/Canvas emitter, a native Android/iOS painter, and
//! the Metal/software backends all answer the same questions about what the
//! product path must support and what the compat fallback may omit.

use crate::motion::MotionSpec;
use crate::quality::QualityProfile;
use crate::recipes::RecipeKind;
use crate::typography::{FontSource, TextRole};

/// Backends the visual layer can lower to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetPainter {
    /// CPU raster with deterministic RGBA output; used for goldens.
    SoftwareRaster,
    /// Metal compute raster with byte-identical software parity.
    Metal,
    /// DOM/CSS product web renderer with native radius/shadow/transform.
    WebDom,
    /// Canvas 2D product web renderer for scenes DOM cannot express.
    WebCanvas,
    /// Verify-only web path: RGBA readback digests, never product UI.
    WebVerifyCanvas,
    /// Native Android/iOS/desktop painter with its own AA and text stack.
    Native,
    /// Mini-program targets; degrade motion by host capability.
    MiniProgram,
}

impl TargetPainter {
    pub const fn name(self) -> &'static str {
        match self {
            Self::SoftwareRaster => "software-raster",
            Self::Metal => "metal",
            Self::WebDom => "web-dom",
            Self::WebCanvas => "web-canvas",
            Self::WebVerifyCanvas => "web-verify-canvas",
            Self::Native => "native",
            Self::MiniProgram => "mini-program",
        }
    }

    /// Product UI may ship on this target. The verify canvas is test-only.
    pub const fn product_capable(self) -> bool {
        !matches!(self, Self::WebVerifyCanvas)
    }

    /// Whether the target resolves text with its own system font stack.
    pub const fn native_text(self) -> bool {
        matches!(self, Self::WebDom | Self::Native | Self::MiniProgram)
    }

    /// Whether the target can use host motion frames (rAF, vsync).
    pub const fn host_frame_loop(self) -> bool {
        matches!(
            self,
            Self::SoftwareRaster
                | Self::Metal
                | Self::WebDom
                | Self::WebCanvas
                | Self::Native
                | Self::MiniProgram
        )
    }
}

/// Resolved product requirements for one target frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PainterRequirements {
    pub profile: QualityProfile,
    pub font_source: FontSource,
    pub edge_coverage: bool,
    pub dpr_enabled: bool,
    pub motion: MotionSpec,
    pub recipe_kinds: [bool; 12],
}

impl PainterRequirements {
    pub fn resolve(profile: QualityProfile, painter: TargetPainter) -> Self {
        let mut recipe_kinds = [false; 12];
        for (index, _kind) in [
            RecipeKind::AppBar,
            RecipeKind::Card,
            RecipeKind::Button,
            RecipeKind::IconButton,
            RecipeKind::Input,
            RecipeKind::Switch,
            RecipeKind::Slider,
            RecipeKind::ListItem,
            RecipeKind::Tabs,
            RecipeKind::Dialog,
            RecipeKind::Badge,
            RecipeKind::Progress,
        ]
        .iter()
        .enumerate()
        {
            // Product targets lower every recipe; the verify canvas lowers
            // none because it never renders product UI.
            recipe_kinds[index] = profile == QualityProfile::Product && painter.product_capable();
        }
        Self {
            profile,
            font_source: if profile == QualityProfile::Product {
                if painter.native_text() {
                    FontSource::SystemOverlay
                } else {
                    FontSource::CoverageAtlas
                }
            } else {
                FontSource::BitmapFallback
            },
            edge_coverage: profile.edge_coverage() && painter.product_capable(),
            dpr_enabled: profile.dpr_fixed() > 1024 && painter.product_capable(),
            motion: MotionSpec::from_profile(profile, false),
            recipe_kinds,
        }
    }

    /// True when every one of the twelve recipes is required.
    pub fn all_recipes_required(&self) -> bool {
        self.recipe_kinds.iter().all(|required| *required)
    }

    /// TextRole must resolve to a named font role on every product target.
    pub const fn role_supported(&self, role: TextRole) -> bool {
        matches!(role, TextRole::Display)
            || matches!(role, TextRole::Headline)
            || matches!(role, TextRole::Title)
            || matches!(role, TextRole::Body)
            || matches!(role, TextRole::Label)
            || matches!(role, TextRole::Caption)
    }
}

#[cfg(test)]
mod tests {
    use super::{PainterRequirements, TargetPainter};
    use crate::quality::QualityProfile;
    use crate::typography::{FontSource, TextRole};

    #[test]
    fn verify_canvas_is_never_a_product_target() {
        assert!(!TargetPainter::WebVerifyCanvas.product_capable());
        assert!(TargetPainter::WebDom.product_capable());
        assert!(TargetPainter::Metal.product_capable());
    }

    #[test]
    fn product_dom_requirements_lower_every_recipe() {
        let req = PainterRequirements::resolve(QualityProfile::Product, TargetPainter::WebDom);
        assert!(req.all_recipes_required());
        assert!(req.edge_coverage);
        assert!(req.dpr_enabled);
        assert!(req.motion.enabled);
        assert_eq!(req.font_source, FontSource::SystemOverlay);
        assert!(req.role_supported(TextRole::Body));
    }

    #[test]
    fn compat_requirements_keep_the_historical_fallback() {
        let req =
            PainterRequirements::resolve(QualityProfile::Compat, TargetPainter::SoftwareRaster);
        assert!(!req.all_recipes_required());
        assert!(!req.edge_coverage);
        assert!(!req.dpr_enabled);
        assert!(!req.motion.enabled);
        assert_eq!(req.font_source, FontSource::BitmapFallback);
    }

    #[test]
    fn product_native_targets_use_their_own_text_stack() {
        assert!(TargetPainter::Native.native_text());
        assert!(TargetPainter::WebDom.native_text());
        assert!(!TargetPainter::Metal.native_text());
        let req = PainterRequirements::resolve(QualityProfile::Product, TargetPainter::Metal);
        assert_eq!(req.font_source, FontSource::CoverageAtlas);
    }
}
