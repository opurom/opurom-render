//! Data-only accessibility tree: pre-order JSON with integer bounds.
//!
//! This is a deterministic description of the rendered layout, not an OS
//! screen-reader integration. Hidden `when` nodes are included with
//! `hidden: true`; their bounds come from a layout with every bool state
//! forced true when that layout can place them, otherwise from zeros.

use crate::ir::{Ir, Kind, Node, Role, StateValue, StateView, ValueType};
use crate::layout::{
    layout_at, node_text, text_size, visible, with_theme_font_scale, Rect, SETTLED_MS,
};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// One accessibility node. `bounds` is `[x, y, w, h]` in logical pixels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct A11yNode {
    pub id: u32,
    pub role: Role,
    pub label: String,
    pub bounds: [i64; 4],
    pub hidden: bool,
}

/// Text at or above this logical-pixel size is a heading by default.
const HEADING_SIZE: i64 = 24;

fn default_role(n: &Node) -> Role {
    match n.kind {
        Kind::Button => Role::Button,
        Kind::Text if text_size(&n.props, 16) >= HEADING_SIZE => Role::Heading,
        Kind::Text => Role::Text,
        Kind::Image => Role::Image,
        Kind::Switch | Kind::Checkbox => Role::Checkbox,
        Kind::Slider => Role::Slider,
        // Data primitives: explicit defaults from the standard role set.
        Kind::Chart | Kind::Map => Role::Image,
        Kind::RichText | Kind::Tooltip => Role::Text,
        Kind::Player => Role::Button,
        Kind::Table | Kind::Calendar | Kind::Menu => Role::None,
        // Extended primitives: each spec documents its default role.
        Kind::Extended(code) => crate::primitives::role_of(code),
        _ => Role::None,
    }
}

/// Effective role of a node: an explicit `role` prop wins, else the kind's
/// default. Shared by the a11y tree and the SVG `data-role` backend.
pub fn role_of(n: &Node) -> Role {
    n.props.role.unwrap_or_else(|| default_role(n))
}

fn label_of(n: &Node, state: &StateView) -> String {
    if let Some(label) = &n.props.label {
        return state.interpolate(label);
    }
    let drawn = node_text(n, state);
    if !drawn.is_empty() {
        return drawn;
    }
    if !n.props.labels.is_empty() {
        return state.interpolate(&n.props.labels.join(", "));
    }
    for candidate in [
        n.props.value.as_deref(),
        n.props.title.as_deref(),
        n.props.name.as_deref(),
        n.props.placeholder.as_deref(),
    ] {
        if let Some(s) = candidate {
            if !s.is_empty() {
                return state.interpolate(s);
            }
        }
    }
    String::new()
}

fn rect_map(
    ir: &Ir,
    state: &StateView,
    width: u32,
    height: u32,
    elapsed_ms: i64,
) -> BTreeMap<u32, Rect> {
    layout_at(ir, state, width, height, elapsed_ms)
        .into_iter()
        .map(|n| (n.id, n.rect))
        .collect()
}

fn walk(
    n: &Node,
    state: &StateView,
    visible_bounds: &BTreeMap<u32, Rect>,
    expanded_bounds: &BTreeMap<u32, Rect>,
    parent_hidden: bool,
    out: &mut Vec<A11yNode>,
) {
    let hidden = parent_hidden || !visible(n, state);
    let map = if hidden {
        expanded_bounds
    } else {
        visible_bounds
    };
    let r = map.get(&n.id).copied().unwrap_or(Rect {
        x: 0,
        y: 0,
        w: 0,
        h: 0,
    });
    out.push(A11yNode {
        id: n.id,
        role: role_of(n),
        label: label_of(n, state),
        bounds: [r.x, r.y, r.w, r.h],
        hidden,
    });
    for c in &n.children {
        walk(c, state, visible_bounds, expanded_bounds, hidden, out);
    }
}

/// Pre-order accessibility tree at a given animation clock position.
pub fn a11y_tree_at(
    ir: &Ir,
    state: &StateView,
    width: u32,
    height: u32,
    elapsed_ms: i64,
) -> Vec<A11yNode> {
    let visible_bounds = rect_map(ir, state, width, height, elapsed_ms);
    // A second layout with every bool state true can still place `when` nodes.
    let mut expanded = state.clone();
    for d in &ir.state {
        if d.typ == ValueType::Bool {
            expanded.set(d.name.clone(), StateValue::Bool(true));
        }
    }
    let expanded_bounds = rect_map(ir, &expanded, width, height, elapsed_ms);
    let tree = with_theme_font_scale(&ir.tree, ir.theme.font_scale);
    let mut out = Vec::new();
    walk(
        &tree,
        state,
        &visible_bounds,
        &expanded_bounds,
        false,
        &mut out,
    );
    out
}

/// Pre-order accessibility tree at the settled animation clock position.
pub fn a11y_tree(ir: &Ir, state: &StateView, width: u32, height: u32) -> Vec<A11yNode> {
    a11y_tree_at(ir, state, width, height, SETTLED_MS)
}

/// Deterministic JSON: one object per line, keys in the documented order.
pub fn a11y_json_at(
    ir: &Ir,
    state: &StateView,
    width: u32,
    height: u32,
    elapsed_ms: i64,
) -> String {
    let nodes = a11y_tree_at(ir, state, width, height, elapsed_ms);
    let mut out = String::from("[\n");
    for (i, n) in nodes.iter().enumerate() {
        let label = serde_json::to_string(&n.label).expect("a string is always serializable");
        let comma = if i + 1 == nodes.len() { "" } else { "," };
        writeln!(
            out,
            "  {{\"id\":{},\"role\":\"{}\",\"label\":{},\"bounds\":[{},{},{},{}],\"hidden\":{}}}{}",
            n.id,
            n.role.name(),
            label,
            n.bounds[0],
            n.bounds[1],
            n.bounds[2],
            n.bounds[3],
            n.hidden,
            comma
        )
        .expect("writing to a String cannot fail");
    }
    out.push_str("]\n");
    out
}

/// Deterministic JSON at the settled animation clock position.
pub fn a11y_json(ir: &Ir, state: &StateView, width: u32, height: u32) -> String {
    a11y_json_at(ir, state, width, height, SETTLED_MS)
}
