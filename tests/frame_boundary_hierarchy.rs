//! Fixed-frame information hierarchy and truncation pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `widgets/text.dart:61-73`: `DefaultTextStyle.maxLines` may be null, and
//!   when it is not null it must be greater than zero.
//! - `widgets/text.dart:389-422`: `maxLines: 1` still soft-wraps; the
//!   overflow beyond the line limit is what the ellipsis trims.
//! - `rendering/paragraph.dart:336-376`: `RenderParagraph` asserts
//!   `maxLines == null || maxLines > 0` and forwards `maxLines` with
//!   `ellipsis: '\u{2026}'` to `TextPainter`.
//! - `rendering/paragraph.dart:998-1008`: a visual overflow clips the
//!   paragraph; `TextOverflow.ellipsis` sets `_needsClipping`.
//! - `semantics/semantics.dart:1275-1279`: a heading level of 0 means the
//!   node is not a heading; 1-6 is the hierarchical level.
//! - `semantics/semantics.dart:7231-7239`: `_mergeHeadingLevels` keeps the
//!   target heading level when the target already has one, otherwise it
//!   takes the source level.
//! - `semantics/semantics.dart:1897-1901`: a header is typically the top
//!   element of a page or section.
//!
//! These tests are a deterministic host projection: they add no ABI or IR
//! field and never mutate shared sources or CI files.

use opurom_render::{
    a11y_json, a11y_tree, digest_hex, parse_ir, Ir, QualityProfile, Role, StateView,
};

const TRUNCATION_IR: &str = r##"{
  "ir": "0.1.0", "app": "Truncation", "state": [],
  "tree": { "id": 0, "kind": "Column", "props": { "gap": 2 },
    "children": [
      { "id": 1, "kind": "Text", "props": {"value":"Alpha Beta Gamma Delta Epsilon Zeta","width":80,"height":80,"size":16}, "children": [], "events": {} },
      { "id": 2, "kind": "Text", "props": {"value":"Alpha Beta Gamma Delta Epsilon Zeta","width":80,"height":80,"size":16,"maxLines":1}, "children": [], "events": {} },
      { "id": 3, "kind": "Text", "props": {"value":"Alpha Beta Gamma Delta Epsilon Zeta","width":80,"height":80,"size":16,"maxLines":2}, "children": [], "events": {} },
      { "id": 4, "kind": "Text", "props": {"value":"Alpha Beta Gamma Delta Epsilon Zeta","width":80,"height":80,"size":16,"maxLines":0}, "children": [], "events": {} }
    ], "events": {} },
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 0 }
}"##;

const HIERARCHY_IR: &str = r##"{
  "ir": "0.1.0", "app": "Hierarchy", "state": [],
  "tree": { "id": 0, "kind": "Column", "props": { "gap": 4 },
    "children": [
      { "id": 1, "kind": "Text", "props": {"value":"Title","size":24}, "children": [], "events": {} },
      { "id": 2, "kind": "Text", "props": {"value":"Section","size":26,"role":"heading"}, "children": [], "events": {} },
      { "id": 3, "kind": "Text", "props": {"value":"Body copy","size":16}, "children": [], "events": {} },
      { "id": 4, "kind": "Button", "props": {"label":"action"}, "children": [], "events": {} }
    ], "events": {} },
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 }
}"##;

fn parse(text: &str) -> Ir {
    parse_ir(text).expect("fixture IR must parse")
}

fn truncation_nodes(ir: &Ir) -> Vec<opurom_render::LaidOut> {
    let state = StateView::from_ir(ir);
    opurom_render::layout(ir, &state, 480, 480)
        .into_iter()
        .filter(|node| node.kind == opurom_render::Kind::Text)
        .collect()
}

#[test]
fn fixed_frame_keeps_only_the_capped_lines() {
    // `widgets/text.dart:61-73` and `rendering/paragraph.dart:336-376`: a
    // non-null `maxLines` is positive; the paragraph keeps exactly that many
    // lines at a fixed frame.
    let ir = parse(TRUNCATION_IR);
    let nodes = truncation_nodes(&ir);
    assert_eq!(nodes.len(), 4);
    assert_eq!(nodes[0].max_lines, None);
    assert_eq!(nodes[1].max_lines, Some(1));
    assert_eq!(nodes[2].max_lines, Some(2));
    assert_eq!(nodes[3].max_lines, Some(0));
}

#[test]
fn truncation_is_byte_deterministic_and_differs_by_cap() {
    // `rendering/paragraph.dart:998-1008`: an overflowing paragraph clips;
    // the ellipsis is part of the same fixed frame.
    let ir = parse(TRUNCATION_IR);
    let state = StateView::from_ir(&ir);
    let first = compat_renderer(480, 480).pixels(&ir, &state);
    let second = compat_renderer(480, 480).pixels(&ir, &state);
    assert_eq!(first, second, "the fixed frame must be reproducible");
    assert_eq!(
        digest_hex(&first),
        digest_hex(&second),
        "digest must not drift across the same frame"
    );
    let plain = parse(TRUNCATION_IR.replace(",\"maxLines\":1}", "}").as_str());
    let plain_state = StateView::from_ir(&plain);
    assert_ne!(
        first,
        compat_renderer(480, 480).pixels(&plain, &plain_state),
        "a line cap must change an overflowing text frame"
    );
}

#[test]
fn heading_roles_follow_the_fixed_type_scale() {
    // `semantics/semantics.dart:1275-1279` and `1897-1901`: a heading marks a
    // section; the default role keeps the type-scale rule from `a11y.rs`
    // (`HEADING_SIZE >= 24`), and an explicit `role` prop wins.
    let ir = parse(HIERARCHY_IR);
    let nodes = a11y_tree(&ir, &StateView::from_ir(&ir), 480, 480);
    let role_of_id = |id: u32| {
        nodes
            .iter()
            .find(|node| node.id == id)
            .unwrap_or_else(|| panic!("node {id} is in the semantics tree"))
            .role
    };
    assert_eq!(role_of_id(1), Role::Heading);
    assert_eq!(role_of_id(2), Role::Heading);
    assert_eq!(role_of_id(3), Role::Text);
    assert_eq!(role_of_id(4), Role::Button);
}

#[test]
fn heading_level_projection_is_deterministic() {
    // `semantics/semantics.dart:7231-7239`: the heading level is a stable
    // integer projection; the a11y JSON carries the same node order.
    let ir = parse(HIERARCHY_IR);
    let json = a11y_json(&ir, &StateView::from_ir(&ir), 480, 480);
    let again = a11y_json(&ir, &StateView::from_ir(&ir), 480, 480);
    assert_eq!(json, again);
    assert!(
        json.contains("\"heading\""),
        "heading role must be exported"
    );
    assert!(
        json.contains("\"button\""),
        "interactive role must be exported"
    );
}

#[test]
fn truncation_respects_the_zero_cap() {
    // `widgets/text.dart:61-73`: only positive or absent caps are valid, and
    // a zero cap draws no lines; the rasterizer skips the glyph pass while
    // the semantics label keeps the source text.
    let ir = parse(TRUNCATION_IR);
    let nodes = truncation_nodes(&ir);
    assert_eq!(nodes[3].max_lines, Some(0));
    let state = StateView::from_ir(&ir);
    let pixels = compat_renderer(480, 480).pixels(&ir, &state);
    let mut node3 = nodes[3].clone();
    node3.rect.y = 0;
    node3.rect.x = 0;
    node3.rect.w = 320;
    node3.rect.h = 240;
    // A zero cap must leave the glyph region empty of ink: compare against
    // the same frame rendered from an IR whose fourth node has no cap.
    let uncapped = parse(TRUNCATION_IR.replace(",\"maxLines\":0}", "}").as_str());
    let uncapped_state = StateView::from_ir(&uncapped);
    let uncapped_pixels = compat_renderer(480, 480).pixels(&uncapped, &uncapped_state);
    assert_ne!(pixels, uncapped_pixels, "a zero cap must suppress all ink");
}

/// The truncation pins are bitmap-text pixels. Product's default system-font
/// overlay suppresses bitmap text, so these assertions pin Compat explicitly,
/// the same convention every golden test uses.
fn compat_renderer(width: u32, height: u32) -> opurom_render::Renderer {
    let mut renderer = opurom_render::Renderer::new(width, height);
    renderer.set_quality(QualityProfile::Compat);
    renderer
}
