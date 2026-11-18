//! Deterministic, IR-independent rules for a future retained layer compositor.
//!
//! Source of truth: Flutter stable 3.50.0 (framework `ab2912035e16`,
//! engine `371f77202a73e`), `packages/flutter/lib/src/rendering/layer.dart`.
//! Only its dirtiness/state machine is ported; no Widget/RenderObject layer
//! object model is introduced. The renderer stays IR-driven and deterministic.

/// One immutable layer-composition decision for a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerDirtyState {
    /// `Layer._needsAddToScene` (layer.dart:351-372): a new layer starts dirty.
    pub needs_add_to_scene: bool,
    /// `alwaysNeedsAddToScene` (layer.dart:409): such layers bypass the
    /// short-circuit and are always recomposited.
    pub always_needs_add_to_scene: bool,
    /// True when this layer may reuse its retained engine layer (cache).
    pub cacheable: bool,
}

impl LayerDirtyState {
    /// A new retained layer is dirty once and cacheable when fully composited.
    pub fn new(always_needs_add_to_scene: bool) -> Self {
        Self {
            needs_add_to_scene: true,
            always_needs_add_to_scene,
            cacheable: !always_needs_add_to_scene,
        }
    }

    /// `Layer.markNeedsAddToScene` (layer.dart:374-389): if already dirty it
    /// short-circuits; otherwise the layer becomes dirty. `alwaysNeedsAddToScene`
    /// layers never call this method in Flutter (assert at :380-382).
    pub fn mark_needs_add_to_scene(&mut self) {
        if !self.always_needs_add_to_scene {
            self.needs_add_to_scene = true;
        }
    }

    /// `Layer.engineLayer` setter (layer.dart:444-478): only a parent that can
    /// retain its layer is marked dirty by a child's new engine layer. An
    /// `alwaysNeedsAddToScene` parent ignores the child's notification, so a
    /// local child change does not propagate into an unrelated parent layer.
    pub fn on_child_engine_layer_changed(parent: &mut Self) {
        if !parent.always_needs_add_to_scene {
            parent.mark_needs_add_to_scene();
        }
    }

    /// `ContainerLayer.updateSubtreeNeedsAddToScene` (layer.dart:1158-1168):
    /// a container is dirty if it is itself dirty OR any child is dirty.
    pub fn after_update_subtree(parent: &mut Self, child_dirty: bool) {
        if child_dirty {
            parent.mark_needs_add_to_scene();
        }
    }

    /// `Layer.buildScene` clears the flag only after `addToScene`
    /// (layer.dart:371-372, :694-713). A clean, retained layer is a static
    /// layer that can be cached for later frames.
    pub fn finish_add_to_scene(&mut self) {
        self.needs_add_to_scene = self.always_needs_add_to_scene;
        self.cacheable = !self.needs_add_to_scene;
    }
}

#[cfg(test)]
mod tests {
    use super::LayerDirtyState;

    #[test]
    fn local_child_change_does_not_touch_unrelated_parent() {
        let mut parent = LayerDirtyState::new(true); // e.g. FollowerLayer
        parent.finish_add_to_scene();
        assert!(parent.needs_add_to_scene, "always-dirty parent stays dirty");
        LayerDirtyState::on_child_engine_layer_changed(&mut parent);
        assert!(!parent.cacheable, "always-dirty layers are never cached");

        let mut retained = LayerDirtyState::new(false);
        retained.finish_add_to_scene();
        assert!(retained.cacheable, "static retained layer can be cached");
        LayerDirtyState::on_child_engine_layer_changed(&mut retained);
        assert!(retained.needs_add_to_scene);
        // `cacheable` describes the last completed composition; it only
        // changes after the next `finish_add_to_scene`.
        assert!(
            retained.cacheable,
            "retained layer still owns its cached engine layer"
        );
    }

    #[test]
    fn container_dirtiness_is_or_of_self_and_children() {
        let mut container = LayerDirtyState::new(false);
        container.finish_add_to_scene();
        LayerDirtyState::after_update_subtree(&mut container, false);
        assert!(container.cacheable, "clean child keeps parent cacheable");
        LayerDirtyState::after_update_subtree(&mut container, true);
        assert!(container.needs_add_to_scene);
        container.finish_add_to_scene();
        assert!(
            container.cacheable,
            "recomposited static layer is cacheable again"
        );
    }
}
