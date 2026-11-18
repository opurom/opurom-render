//! Product showcase scene contract.
//!
//! A scene is the unit the visual CI renders on every target. It is a pure
//! lowered description: viewport, theme, component state, recipe and the
//! frame artifacts each target must emit. The renderer neither interprets IR
//! at runtime nor imports a sibling repository.

use crate::layout::Rect;
use crate::recipes::{ComponentState, RecipeKind};
use crate::Rgba;

/// Visual matrix viewports from the product quality gate.
pub const SHOWCASE_VIEWPORTS: [(u32, u32); 4] =
    [(390, 844), (768, 1024), (1280, 800), (1920, 1080)];

/// Showcase scenes rendered by the visual gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShowcaseScene {
    Showcase,
    Gallery,
    Dashboard,
    DesignSystem,
    Forms,
    Navigation,
}

impl ShowcaseScene {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Showcase => "showcase",
            Self::Gallery => "gallery",
            Self::Dashboard => "dashboard",
            Self::DesignSystem => "design-system",
            Self::Forms => "forms",
            Self::Navigation => "navigation",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "showcase" => Some(Self::Showcase),
            "gallery" => Some(Self::Gallery),
            "dashboard" => Some(Self::Dashboard),
            "design-system" => Some(Self::DesignSystem),
            "forms" => Some(Self::Forms),
            "navigation" => Some(Self::Navigation),
            _ => None,
        }
    }
}

/// Theme axis of the visual matrix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShowcaseTheme {
    Light,
    Dark,
}

impl ShowcaseTheme {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

/// Frame artifacts each target must emit for the visual matrix.
pub const FRAME_ARTIFACTS: [&str; 7] = [
    "frame.png",
    "frame.svg",
    "frame.ops.json",
    "layout.json",
    "a11y.json",
    "motion.json",
    "quality.json",
];

/// One cell of the visual matrix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneCell {
    pub scene: ShowcaseScene,
    pub viewport: (u32, u32),
    pub theme: ShowcaseTheme,
    pub state: ComponentState,
    pub recipe: RecipeKind,
}

impl SceneCell {
    /// Stable artifact prefix, e.g. `showcase-390x844-light-hover-Button`.
    pub fn artifact_prefix(&self) -> String {
        format!(
            "{}-{}x{}-{}-{}-{}",
            self.scene.name(),
            self.viewport.0,
            self.viewport.1,
            self.theme.name(),
            self.state.name(),
            self.recipe.name()
        )
    }

    /// Deterministic background colour for the cell's scene/theme pair.
    pub const fn background(self) -> Rgba {
        match (self.theme, self.scene) {
            (ShowcaseTheme::Light, _) => Rgba::rgb(244, 246, 249),
            (ShowcaseTheme::Dark, ShowcaseScene::Dashboard) => Rgba::rgb(14, 23, 41),
            (ShowcaseTheme::Dark, _) => Rgba::rgb(22, 34, 59),
        }
    }

    /// The logical frame the target must rasterize into.
    pub const fn frame(self) -> Rect {
        Rect {
            x: 0,
            y: 0,
            w: self.viewport.0 as i64,
            h: self.viewport.1 as i64,
        }
    }
}

/// Full product matrix: every scene, viewport, theme, state and recipe.
pub fn visual_matrix() -> Vec<SceneCell> {
    let scenes = [
        ShowcaseScene::Showcase,
        ShowcaseScene::Gallery,
        ShowcaseScene::Dashboard,
        ShowcaseScene::DesignSystem,
        ShowcaseScene::Forms,
        ShowcaseScene::Navigation,
    ];
    let recipes = [
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
    ];
    let states = [
        ComponentState::Default,
        ComponentState::Hover,
        ComponentState::Active,
        ComponentState::Focus,
        ComponentState::Disabled,
    ];
    let themes = [ShowcaseTheme::Light, ShowcaseTheme::Dark];
    let mut cells = Vec::new();
    for scene in scenes {
        for &viewport in SHOWCASE_VIEWPORTS.iter() {
            for theme in themes {
                for state in states {
                    for recipe in recipes {
                        cells.push(SceneCell {
                            scene,
                            viewport,
                            theme,
                            state,
                            recipe,
                        });
                    }
                }
            }
        }
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_is_complete_and_cells_are_stable() {
        let cells = visual_matrix();
        assert_eq!(cells.len(), 6 * 4 * 2 * 5 * 12);
        let first = &cells[0];
        assert_eq!(first.scene, ShowcaseScene::Showcase);
        assert_eq!(first.viewport, (390, 844));
        assert_eq!(first.theme, ShowcaseTheme::Light);
        assert_eq!(first.state, ComponentState::Default);
        assert_eq!(first.recipe, RecipeKind::AppBar);
        assert_eq!(
            cells[1].artifact_prefix(),
            "showcase-390x844-light-default-Card"
        );
    }

    #[test]
    fn scenes_parse_and_frame_artifacts_are_frozen() {
        for scene in [
            ShowcaseScene::Showcase,
            ShowcaseScene::Gallery,
            ShowcaseScene::Dashboard,
            ShowcaseScene::DesignSystem,
            ShowcaseScene::Forms,
            ShowcaseScene::Navigation,
        ] {
            assert_eq!(ShowcaseScene::parse(scene.name()), Some(scene));
        }
        assert_eq!(FRAME_ARTIFACTS.len(), 7);
        assert_eq!(SHOWCASE_VIEWPORTS.len(), 4);
    }

    #[test]
    fn scene_backgrounds_follow_the_theme() {
        let light = SceneCell {
            scene: ShowcaseScene::Showcase,
            viewport: (390, 844),
            theme: ShowcaseTheme::Light,
            state: ComponentState::Default,
            recipe: RecipeKind::Card,
        };
        let dark = SceneCell {
            theme: ShowcaseTheme::Dark,
            ..light
        };
        assert_ne!(light.background(), dark.background());
        assert_eq!(light.frame().w, 390);
        assert_eq!(light.frame().h, 844);
    }
}
