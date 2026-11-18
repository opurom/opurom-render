//! App-level MotionSpec contract.
//!
//! Motion is lowered from the compiler into target schedules. The renderer
//! keeps a caller-owned logical clock; no wall clock or runtime IR
//! interpreter is introduced. Compat disables app motion; Product enables it.

use crate::quality::QualityProfile;

/// Built-in semantic motion events from the product visual contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionEvent {
    RouteEnter,
    RouteExit,
    StateChange,
    ListEnter,
    Press,
    Dialog,
    Switch,
    Progress,
}

impl MotionEvent {
    pub const fn name(self) -> &'static str {
        match self {
            Self::RouteEnter => "routeEnter",
            Self::RouteExit => "routeExit",
            Self::StateChange => "stateChange",
            Self::ListEnter => "listEnter",
            Self::Press => "press",
            Self::Dialog => "dialog",
            Self::Switch => "switch",
            Self::Progress => "progress",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "routeEnter" => Some(Self::RouteEnter),
            "routeExit" => Some(Self::RouteExit),
            "stateChange" => Some(Self::StateChange),
            "listEnter" => Some(Self::ListEnter),
            "press" => Some(Self::Press),
            "dialog" => Some(Self::Dialog),
            "switch" => Some(Self::Switch),
            "progress" => Some(Self::Progress),
            _ => None,
        }
    }

    /// Stable curve name lowered to `crate::anim::Curve`.
    pub const fn curve(self) -> &'static str {
        match self {
            Self::RouteEnter | Self::ListEnter | Self::Dialog => "ease",
            Self::RouteExit | Self::Progress => "ease",
            Self::StateChange | Self::Switch => "spring",
            Self::Press => "fast",
        }
    }

    pub const fn duration_ms(self) -> i64 {
        match self {
            Self::RouteEnter => 260,
            Self::RouteExit => 160,
            Self::StateChange => 220,
            Self::ListEnter => 320,
            Self::Press => 90,
            Self::Dialog => 240,
            Self::Switch => 200,
            Self::Progress => 280,
        }
    }

    pub const fn delay_ms(self) -> i64 {
        match self {
            Self::ListEnter => 40,
            _ => 0,
        }
    }

    /// Stagger step in milliseconds. Only list entry uses it by default.
    pub const fn stagger_ms(self) -> i64 {
        match self {
            Self::ListEnter => 30,
            _ => 0,
        }
    }

    pub const fn effects(self) -> &'static [&'static str] {
        match self {
            Self::RouteEnter => &["fade", "translateY"],
            Self::RouteExit => &["fade"],
            Self::StateChange => &["spring"],
            Self::ListEnter => &["stagger", "fade"],
            Self::Press => &["ripple", "elevation"],
            Self::Dialog => &["scale", "fade"],
            Self::Switch => &["thumbSpring"],
            Self::Progress => &["easedSweep"],
        }
    }
}

/// Complete lowered motion specification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MotionSpec {
    pub enabled: bool,
    pub reduced_motion: bool,
}

impl MotionSpec {
    pub const fn from_profile(profile: QualityProfile, reduced_motion: bool) -> Self {
        Self {
            enabled: matches!(profile, QualityProfile::Product) && !reduced_motion,
            reduced_motion,
        }
    }

    /// Duration after reduced-motion resolution. Reduced motion keeps semantic
    /// events but collapses them to a non-animated transition.
    pub const fn resolved_duration_ms(self, event: MotionEvent) -> i64 {
        if self.enabled {
            event.duration_ms()
        } else {
            0
        }
    }

    pub const fn resolved_delay_ms(self, event: MotionEvent) -> i64 {
        if self.enabled {
            event.delay_ms()
        } else {
            0
        }
    }

    /// Whether a target should emit keyframes for this event.
    pub const fn animates(self, event: MotionEvent) -> bool {
        self.enabled && self.resolved_duration_ms(event) > 0
    }
}

#[cfg(test)]
mod tests {
    use super::{MotionEvent, MotionSpec};
    use crate::QualityProfile;

    const EVENTS: [MotionEvent; 8] = [
        MotionEvent::RouteEnter,
        MotionEvent::RouteExit,
        MotionEvent::StateChange,
        MotionEvent::ListEnter,
        MotionEvent::Press,
        MotionEvent::Dialog,
        MotionEvent::Switch,
        MotionEvent::Progress,
    ];

    #[test]
    fn all_semantic_events_parse_with_positive_durations() {
        assert_eq!(EVENTS.len(), 8);
        for event in EVENTS {
            assert_eq!(MotionEvent::parse(event.name()), Some(event));
            assert!(event.duration_ms() > 0);
            assert!(!event.effects().is_empty());
        }
    }

    #[test]
    fn compat_motion_is_static() {
        let spec = MotionSpec::from_profile(QualityProfile::Compat, false);
        assert!(!spec.enabled);
        for event in EVENTS {
            assert_eq!(spec.resolved_duration_ms(event), 0);
            assert!(!spec.animates(event));
        }
    }

    #[test]
    fn product_motion_is_enabled_without_reduced_motion() {
        let spec = MotionSpec::from_profile(QualityProfile::Product, false);
        assert!(spec.enabled);
        assert!(spec.animates(MotionEvent::RouteEnter));
        assert_eq!(
            spec.resolved_duration_ms(MotionEvent::RouteEnter),
            MotionEvent::RouteEnter.duration_ms()
        );
    }

    #[test]
    fn reduced_motion_collapses_duration_but_keeps_semantics() {
        let spec = MotionSpec::from_profile(QualityProfile::Product, true);
        assert!(spec.reduced_motion);
        assert!(!spec.enabled);
        for event in EVENTS {
            assert_eq!(spec.resolved_duration_ms(event), 0);
            assert_eq!(
                event.name(),
                MotionEvent::parse(event.name()).unwrap().name()
            );
        }
    }
}
