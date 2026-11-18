//! Semantic component recipe contract.
//!
//! A recipe is a pure description lowered by the compiler into ordinary IR
//! primitives. It never interprets IR at runtime and never imports a sibling
//! repository. The twelve recipes below define the defaults required by the
//! product visual layer; target emitters lower the same contract.

use crate::typography::TextRole;

/// The first twelve semantic recipes of the product design system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecipeKind {
    AppBar,
    Card,
    Button,
    IconButton,
    Input,
    Switch,
    Slider,
    ListItem,
    Tabs,
    Dialog,
    Badge,
    Progress,
}

impl RecipeKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::AppBar => "AppBar",
            Self::Card => "Card",
            Self::Button => "Button",
            Self::IconButton => "IconButton",
            Self::Input => "Input",
            Self::Switch => "Switch",
            Self::Slider => "Slider",
            Self::ListItem => "ListItem",
            Self::Tabs => "Tabs",
            Self::Dialog => "Dialog",
            Self::Badge => "Badge",
            Self::Progress => "Progress",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "AppBar" => Some(Self::AppBar),
            "Card" => Some(Self::Card),
            "Button" => Some(Self::Button),
            "IconButton" => Some(Self::IconButton),
            "Input" => Some(Self::Input),
            "Switch" => Some(Self::Switch),
            "Slider" => Some(Self::Slider),
            "ListItem" => Some(Self::ListItem),
            "Tabs" => Some(Self::Tabs),
            "Dialog" => Some(Self::Dialog),
            "Badge" => Some(Self::Badge),
            "Progress" => Some(Self::Progress),
            _ => None,
        }
    }

    pub const fn typography_role(self) -> TextRole {
        match self {
            Self::AppBar | Self::Dialog => TextRole::Title,
            Self::Card | Self::ListItem => TextRole::Body,
            Self::Button | Self::Input | Self::Tabs => TextRole::Label,
            Self::IconButton | Self::Badge | Self::Progress => TextRole::Caption,
            Self::Switch | Self::Slider => TextRole::Body,
        }
    }

    pub const fn default_width(self) -> i64 {
        match self {
            Self::AppBar | Self::Tabs | Self::ListItem => 320,
            Self::Card | Self::Dialog => 280,
            Self::Input => 240,
            Self::Button => 120,
            Self::IconButton => 44,
            Self::Switch => 52,
            Self::Slider => 200,
            Self::Badge => 32,
            Self::Progress => 240,
        }
    }

    pub const fn default_height(self) -> i64 {
        match self {
            Self::AppBar => 56,
            Self::Card | Self::Dialog => 120,
            Self::ListItem => 64,
            Self::Input | Self::Button => 44,
            Self::IconButton | Self::Switch | Self::Badge => 44,
            Self::Slider | Self::Progress => 24,
            Self::Tabs => 48,
        }
    }

    pub const fn default_padding(self) -> i64 {
        match self {
            Self::AppBar | Self::Tabs => 16,
            Self::Card | Self::Dialog => 20,
            Self::ListItem => 12,
            Self::Button | Self::Input => 12,
            Self::IconButton | Self::Switch | Self::Badge | Self::Progress => 4,
            Self::Slider => 8,
        }
    }

    pub const fn default_gap(self) -> i64 {
        match self {
            Self::AppBar | Self::Tabs | Self::ListItem => 8,
            Self::Card | Self::Dialog => 12,
            Self::Button | Self::Input => 6,
            Self::Slider | Self::Progress => 4,
            Self::IconButton | Self::Switch | Self::Badge => 0,
        }
    }

    pub const fn default_radius(self) -> i64 {
        match self {
            Self::AppBar | Self::ListItem | Self::Progress => 0,
            Self::Card | Self::Dialog => 16,
            Self::Button | Self::Input | Self::Tabs => 12,
            Self::IconButton | Self::Badge | Self::Switch => 22,
            Self::Slider => 8,
        }
    }

    pub const fn elevation(self) -> i64 {
        match self {
            Self::Card | Self::Button | Self::IconButton | Self::Dialog => 3,
            _ => 0,
        }
    }
}

/// Interaction states required by the product visual gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComponentState {
    Default,
    Hover,
    Active,
    Focus,
    Disabled,
}

impl ComponentState {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Hover => "hover",
            Self::Active => "active",
            Self::Focus => "focus",
            Self::Disabled => "disabled",
        }
    }
}

pub const COMPONENT_STATES: [ComponentState; 5] = [
    ComponentState::Default,
    ComponentState::Hover,
    ComponentState::Active,
    ComponentState::Focus,
    ComponentState::Disabled,
];

/// Variants shared by actionable recipes. Not every recipe uses every
/// variant; the contract keeps one stable lowering vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComponentVariant {
    Primary,
    Secondary,
    Text,
    Danger,
    Ghost,
    Fab,
}

impl ComponentVariant {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Secondary => "secondary",
            Self::Text => "text",
            Self::Danger => "danger",
            Self::Ghost => "ghost",
            Self::Fab => "fab",
        }
    }
}

pub const COMPONENT_VARIANTS: [ComponentVariant; 6] = [
    ComponentVariant::Primary,
    ComponentVariant::Secondary,
    ComponentVariant::Text,
    ComponentVariant::Danger,
    ComponentVariant::Ghost,
    ComponentVariant::Fab,
];

/// Stable color slot names used by the design-system token resolver.
pub const COLOR_SLOTS: [&str; 6] = ["primary", "surface", "card", "ink", "muted", "divider"];

/// Stable accessibility role names.
pub const A11Y_ROLES: [&str; 7] = [
    "button",
    "textbox",
    "switch",
    "slider",
    "tab",
    "dialog",
    "progressbar",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_twelve_recipes_parse_with_stable_names() {
        let all = [
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
        assert_eq!(all.len(), 12);
        for recipe in all {
            assert_eq!(RecipeKind::parse(recipe.name()), Some(recipe));
            assert!(recipe.default_width() > 0);
            assert!(recipe.default_height() > 0);
            assert!(recipe.default_radius() >= 0);
        }
    }

    #[test]
    fn every_recipe_has_positive_touch_target_dimensions() {
        for recipe in [
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
        ] {
            assert!(recipe.default_width() >= 32, "{}", recipe.name());
            assert!(recipe.default_height() >= 24, "{}", recipe.name());
        }
    }

    #[test]
    fn state_and_variant_vocabularies_are_frozen() {
        assert_eq!(COMPONENT_STATES.len(), 5);
        assert_eq!(COMPONENT_VARIANTS.len(), 6);
        let states: Vec<&str> = COMPONENT_STATES.iter().map(|s| s.name()).collect();
        assert_eq!(states, ["default", "hover", "active", "focus", "disabled"]);
        let variants: Vec<&str> = COMPONENT_VARIANTS.iter().map(|v| v.name()).collect();
        assert_eq!(
            variants,
            ["primary", "secondary", "text", "danger", "ghost", "fab"]
        );
    }

    #[test]
    fn recipe_typography_roles_are_product_roles() {
        assert_eq!(RecipeKind::AppBar.typography_role(), TextRole::Title);
        assert_eq!(RecipeKind::Button.typography_role(), TextRole::Label);
        assert_eq!(RecipeKind::Card.typography_role(), TextRole::Body);
    }
}
