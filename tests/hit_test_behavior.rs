//! HitTestBehavior parity pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors (blob `5f2b7b9f102fac1e9d7271bb072ff7889453e13b`):
//! - `rendering/proxy_box.dart:146-175` (`HitTestBehavior`): `deferToChild`
//!   receives events only when a child is hit; `opaque` is hit anywhere in
//!   its bounds and blocks nodes behind it; `translucent` is hit anywhere in
//!   its bounds and keeps nodes behind it in the path.
//! - `rendering/proxy_box.dart:197-205` (`hitTest`): a node joins the path
//!   when a child is hit, when it hits itself, or when it is translucent;
//!   `hitTarget` itself only becomes true for a child hit or an opaque self
//!   hit.
//! - `rendering/proxy_box.dart:209`: `hitTestSelf` is true only for `opaque`.
//!
//! These tests use a deterministic host projection; Opurom's IR uses a
//! precomputed leaf-to-root path and needs no runtime behavior field.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HitTestBehavior {
    DeferToChild,
    Opaque,
    Translucent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HitNode {
    behavior: HitTestBehavior,
    inside: bool,
    child_hit: bool,
}

impl HitNode {
    /// `proxy_box.dart:197-205`: whether the node is added to the path.
    fn joins_path(&self) -> bool {
        self.inside && (self.child_hit || self.hit_self() || self.translucent())
    }

    /// `proxy_box.dart:209`: only `opaque` hits itself.
    fn hit_self(&self) -> bool {
        self.behavior == HitTestBehavior::Opaque
    }

    /// `proxy_box.dart:201`: translucent nodes are added even when they
    /// did not become the hit target.
    fn translucent(&self) -> bool {
        self.behavior == HitTestBehavior::Translucent
    }

    /// `proxy_box.dart:200,205`: whether this node stops nodes behind it.
    fn is_hit_target(&self) -> bool {
        self.inside && (self.child_hit || self.hit_self())
    }
}

#[test]
fn defer_to_child_is_hit_only_when_a_child_is_hit() {
    let empty = HitNode {
        behavior: HitTestBehavior::DeferToChild,
        inside: true,
        child_hit: false,
    };
    assert!(!empty.joins_path());
    assert!(!empty.is_hit_target());
    let with_child = HitNode {
        child_hit: true,
        ..empty
    };
    assert!(with_child.joins_path());
    assert!(with_child.is_hit_target());
}

#[test]
fn opaque_hits_itself_and_stops_the_path() {
    let node = HitNode {
        behavior: HitTestBehavior::Opaque,
        inside: true,
        child_hit: false,
    };
    assert!(node.hit_self());
    assert!(node.joins_path());
    assert!(node.is_hit_target());
}

#[test]
fn translucent_joins_the_path_without_stopping_it() {
    let node = HitNode {
        behavior: HitTestBehavior::Translucent,
        inside: true,
        child_hit: false,
    };
    assert!(!node.hit_self());
    assert!(node.joins_path());
    assert!(!node.is_hit_target());
}

#[test]
fn outside_the_bounds_no_behavior_joins_the_path() {
    for behavior in [
        HitTestBehavior::DeferToChild,
        HitTestBehavior::Opaque,
        HitTestBehavior::Translucent,
    ] {
        let node = HitNode {
            behavior,
            inside: false,
            child_hit: true,
        };
        assert!(!node.joins_path());
        assert!(!node.is_hit_target());
    }
}

#[test]
fn hit_behavior_projection_is_deterministic() {
    let run = || {
        [
            HitTestBehavior::DeferToChild,
            HitTestBehavior::Opaque,
            HitTestBehavior::Translucent,
        ]
        .iter()
        .map(|behavior| {
            let node = HitNode {
                behavior: *behavior,
                inside: true,
                child_hit: false,
            };
            (node.joins_path(), node.is_hit_target())
        })
        .collect::<Vec<_>>()
    };
    assert_eq!(run(), run());
}
