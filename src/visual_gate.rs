//! Pure visual quality checks used by CI gates on every target.
//!
//! These checks consume lowered layout/geometry facts, never IR at runtime.
//! They report machine-readable findings; target CI decides how to archive
//! screenshots and perceptual diffs. No visual feature is disabled here.

use crate::layout::Rect;
use crate::recipes::{ComponentState, RecipeKind, COMPONENT_STATES};
use crate::Rgba;

/// Machine-readable visual finding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisualFinding {
    pub code: &'static str,
    pub pass: bool,
    pub detail: &'static str,
}

/// Layout facts for one frame. Only the fields needed by the pure checks are
/// included so every target can lower the same structure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutFact {
    pub rect: Rect,
    pub role: Option<RecipeKind>,
    pub state: ComponentState,
    pub focus_visible: bool,
    pub touch_target: bool,
}

/// Text facts for overlap detection. Rectangles are already measured by the
/// target's text engine with the active typography profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextFact {
    pub rect: Rect,
    pub min_contrast_ratio_x100: i64,
}

impl Rect {
    /// Strict overlap with a one-pixel tolerance: touching edges are legal.
    fn overlaps_conservatively(self, other: Self) -> bool {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);
        let x1 = (self.x + self.w).min(other.x + other.w) - 1;
        let y1 = (self.y + self.h).min(other.y + other.h) - 1;
        x1 > x0 && y1 > y0
    }
}

pub fn check_text_overlap(texts: &[TextFact]) -> VisualFinding {
    for (i, a) in texts.iter().enumerate() {
        for b in texts.iter().skip(i + 1) {
            if a.rect.overlaps_conservatively(b.rect) {
                return VisualFinding {
                    code: "TEXT_OVERLAP",
                    pass: false,
                    detail: "text rectangles overlap beyond a one-pixel tolerance",
                };
            }
        }
    }
    VisualFinding {
        code: "TEXT_OVERLAP",
        pass: true,
        detail: "no text rectangle overlap",
    }
}

pub fn check_contrast(texts: &[TextFact], minimum_x100: i64) -> VisualFinding {
    let pass = texts
        .iter()
        .all(|text| text.min_contrast_ratio_x100 >= minimum_x100);
    VisualFinding {
        code: "CONTRAST",
        pass,
        detail: "all text meets the minimum contrast ratio",
    }
}

pub fn check_min_touch_targets(items: &[LayoutFact]) -> VisualFinding {
    let pass = items.iter().all(|item| item.touch_target);
    VisualFinding {
        code: "MIN_TOUCH_TARGET",
        pass,
        detail: "all interactive targets meet the minimum size",
    }
}

pub fn check_focus_states(items: &[LayoutFact]) -> VisualFinding {
    let pass = items
        .iter()
        .all(|item| !item.focus_visible || item.touch_target);
    VisualFinding {
        code: "FOCUS_STATE",
        pass,
        detail: "visible focus states are paired with actionable targets",
    }
}

pub fn check_component_states(items: &[LayoutFact]) -> VisualFinding {
    let mut seen = [false; 5];
    for item in items {
        if item.role.is_some() {
            for (index, state) in COMPONENT_STATES.iter().enumerate() {
                if item.state == *state {
                    seen[index] = true;
                }
            }
        }
    }
    let pass = seen.iter().all(|present| *present);
    VisualFinding {
        code: "COMPONENT_STATES",
        pass,
        detail: "all component states are represented in the visual matrix",
    }
}

/// Integer luminance helper for deterministic contrast calculations.
pub fn relative_luminance_x100(color: Rgba) -> i64 {
    let channel = |v: u8| -> i64 {
        let s = v as i64;
        let linear = if s <= 10 {
            s * 10
        } else {
            ((s + 14) * (s + 14) * 100) / 5644
        };
        linear
    };
    (2126 * channel(color.r) + 7152 * channel(color.g) + 722 * channel(color.b)) / 10_000
}

pub fn contrast_ratio_x100(foreground: Rgba, background: Rgba) -> i64 {
    let a = relative_luminance_x100(foreground);
    let b = relative_luminance_x100(background);
    let (hi, lo) = if a >= b { (a, b) } else { (b, a) };
    (hi + 5) * 100 / (lo + 5)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i64, y: i64, w: i64, h: i64) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn touching_text_edges_are_allowed_but_real_overlap_fails() {
        let a = TextFact {
            rect: rect(0, 0, 10, 10),
            min_contrast_ratio_x100: 700,
        };
        let b = TextFact {
            rect: rect(10, 0, 10, 10),
            min_contrast_ratio_x100: 700,
        };
        assert!(check_text_overlap(&[a, b]).pass);
        let overlap = TextFact {
            rect: rect(8, 5, 10, 10),
            min_contrast_ratio_x100: 700,
        };
        assert!(!check_text_overlap(&[a, overlap]).pass);
    }

    #[test]
    fn contrast_and_touch_gates_fail_closed() {
        let low = TextFact {
            rect: rect(0, 0, 10, 10),
            min_contrast_ratio_x100: 300,
        };
        assert!(!check_contrast(&[low], 450).pass);
        let fact = LayoutFact {
            rect: rect(0, 0, 20, 20),
            role: Some(RecipeKind::Button),
            state: ComponentState::Default,
            focus_visible: true,
            touch_target: false,
        };
        assert!(!check_min_touch_targets(&[fact]).pass);
        assert!(!check_focus_states(&[fact]).pass);
    }

    #[test]
    fn component_state_matrix_requires_all_five_states() {
        let mut facts: Vec<LayoutFact> = COMPONENT_STATES
            .iter()
            .map(|state| LayoutFact {
                rect: rect(0, 0, 44, 44),
                role: Some(RecipeKind::Button),
                state: *state,
                focus_visible: false,
                touch_target: true,
            })
            .collect();
        assert!(check_component_states(&facts).pass);
        facts.pop();
        assert!(!check_component_states(&facts).pass);
    }

    #[test]
    fn contrast_ratio_is_monotone_with_luminance_distance() {
        let black = Rgba::rgb(0, 0, 0);
        let white = Rgba::rgb(255, 255, 255);
        let grey = Rgba::rgb(128, 128, 128);
        assert!(contrast_ratio_x100(black, white) > contrast_ratio_x100(black, grey));
        assert!(relative_luminance_x100(white) > relative_luminance_x100(black));
    }
}
