//! Deterministic incremental-render data helpers (INCREMENTAL_RENDER_PIPELINE
//! §2-§6). Pure functions only: no runtime tree, no wall clock, no floats.
//! Every helper is opt-in and additive; programs that declare no boundary
//! props and never enable the cache keep byte-identical output.

use crate::{Node, StateValue, StateView};

/// Memo metadata carried on a laid-out node. Its `PartialEq` is intentionally
/// always true: identity and hashes are caches, not node semantics, so existing
/// frame-diff code comparing `LaidOut` values stays byte-compatible.
#[derive(Clone, Debug, Default)]
pub struct IncrementalMeta {
    pub identity: Option<NodeIdentity>,
    pub subtree_hash: u64,
    pub paint_hash: u64,
}

impl PartialEq for IncrementalMeta {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for IncrementalMeta {}

/// FNV-1a over a stream of tagged bytes. Stable across runs and platforms.
pub struct Hasher(u64);

impl Hasher {
    pub fn new() -> Hasher {
        Hasher(0xcbf2_9ce4_8422_2325)
    }

    /// Feed one tagged integer; the tag separates field boundaries so distinct
    /// prop layouts cannot collide by concatenation.
    pub fn int(&mut self, tag: u8, value: i64) -> &mut Self {
        self.bytes(&[tag]);
        self.bytes(&value.to_le_bytes());
        self
    }

    pub fn text(&mut self, tag: u8, value: &str) -> &mut Self {
        self.bytes(&[tag]);
        self.bytes(&(value.len() as u64).to_le_bytes());
        self.bytes(value.as_bytes());
        self
    }

    pub fn bytes(&mut self, values: &[u8]) -> &mut Self {
        for &b in values {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
        self
    }

    pub fn finish(self) -> u64 {
        self.0
    }
}

/// Stable runtime identity of a laid-out node. Static nodes use the IR id;
/// generated `For` rows pair the node id with their row key (the element value),
/// so inserting a row before another keeps surviving rows' identity.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeIdentity {
    pub node_id: u32,
    pub row_key: Option<String>,
}

impl NodeIdentity {
    pub fn node(node_id: u32) -> NodeIdentity {
        NodeIdentity {
            node_id,
            row_key: None,
        }
    }

    pub fn row(node_id: u32, row_key: impl Into<String>) -> NodeIdentity {
        NodeIdentity {
            node_id,
            row_key: Some(row_key.into()),
        }
    }

    /// Canonical string key for keyed memo caches.
    pub fn key(&self) -> String {
        match &self.row_key {
            Some(k) => format!("{}#{}", self.node_id, k),
            None => format!("{}", self.node_id),
        }
    }
}

/// Collect `{name}` interpolation keys from a prop string, in first-seen order,
/// skipping `{{` escaped braces. Mirrors `StateView::interpolate` exactly.
fn interpolation_keys(text: &str, out: &mut Vec<String>) {
    let b = text.as_bytes();
    let mut i = 0usize;
    while i < b.len() {
        if b[i] == b'{' {
            if i + 1 < b.len() && b[i + 1] == b'{' {
                i += 2;
                continue;
            }
            if let Some(rel) = text[i + 1..].find('}') {
                let end = i + 1 + rel;
                let name = &text[i + 1..end];
                if !out.iter().any(|k| k == name) {
                    out.push(name.to_string());
                }
                i = end + 1;
                continue;
            }
        }
        i += 1;
    }
}

fn push_key(out: &mut Vec<String>, name: Option<&str>) {
    if let Some(name) = name {
        if !out.iter().any(|k| k == name) {
            out.push(name.to_string());
        }
    }
}

/// Ordered set of state keys this node reads through its resolved props,
/// visibility or controls. A `For` node contributes its `source` key; the row
/// template's own keys are accounted per row by the caller, never as the whole
/// list here.
pub fn deps(node: &Node) -> Vec<String> {
    let mut out = Vec::new();
    push_key(&mut out, node.when.as_deref());
    push_key(&mut out, node.unless.as_deref());
    let p = &node.props;
    push_key(&mut out, p.bind.as_deref());
    push_key(&mut out, p.source.as_deref());
    for text in [
        p.value.as_deref(),
        p.label.as_deref(),
        p.title.as_deref(),
        p.name.as_deref(),
        p.placeholder.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        interpolation_keys(text, &mut out);
    }
    out
}

/// Resolved values of `deps(node)` in deterministic order, hashed with their
/// keys. Used by the layout memo key so a state change outside deps is invisible.
pub fn dep_values(node: &Node, state: &StateView) -> u64 {
    let mut h = Hasher::new();
    for key in deps(node) {
        h.text(1, &key);
        match state.get(&key) {
            Some(StateValue::Int(v)) => {
                h.int(2, *v);
            }
            Some(StateValue::Bool(v)) => {
                h.int(3, i64::from(*v));
            }
            Some(StateValue::Str(v)) => {
                h.text(4, v);
            }
            Some(StateValue::List(v)) => {
                h.int(5, v.len() as i64);
                for item in v {
                    h.text(6, item);
                }
            }
            None => {
                h.int(7, 0);
            }
        }
    }
    h.finish()
}

/// Feed the appearance-bearing fields of a node into `h`.
fn appearance(node: &Node, h: &mut Hasher) {
    let p = &node.props;
    h.text(10, &format!("{:?}", node.kind));
    h.int(11, p.width.unwrap_or(0));
    h.int(12, p.height.unwrap_or(0));
    h.int(13, p.radius.unwrap_or(0));
    h.int(14, p.align as i64);
    h.int(15, p.text_align.map(|a| a as i64).unwrap_or(-1));
    h.int(16, p.size.unwrap_or(0));
    h.int(17, p.opacity.unwrap_or(0));
    h.int(18, p.shadow.unwrap_or(0));
    h.int(19, p.max_lines.unwrap_or(0) as i64);
    h.int(20, i64::from(p.glass));
    h.int(21, i64::from(p.secure));
    h.int(22, i64::from(p.high_contrast));
    h.int(23, i64::from(p.disabled));
    if let Some(c) = p.color {
        h.int(
            24,
            ((c.r as i64) << 24) | ((c.g as i64) << 16) | ((c.b as i64) << 8) | c.a as i64,
        );
    }
    if let Some(c) = p.border {
        h.int(
            25,
            ((c.r as i64) << 24) | ((c.g as i64) << 16) | ((c.b as i64) << 8) | c.a as i64,
        );
    }
    for text in [
        p.value.as_deref(),
        p.label.as_deref(),
        p.title.as_deref(),
        p.name.as_deref(),
        p.placeholder.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        h.text(26, text);
    }
    h.text(27, &format!("{:?}", p.effects));
    h.text(28, &format!("{:?}", p.particles));
    h.text(29, &format!("{:?}", p.gradient));
    h.int(30, i64::from(p.gradient_horizontal));
}

/// Paint hash: only appearance-bearing fields, no geometry of children.
pub fn paint_hash(node: &Node) -> u64 {
    let mut h = Hasher::new();
    appearance(node, &mut h);
    h.finish()
}

/// Subtree hash: own appearance, this node's resolved dep values, and the
/// ordered child identities with their subtree hashes. Pure and deterministic:
/// same IR and state always produce the same value on every platform.
pub fn subtree_hash(node: &Node, state: &StateView) -> u64 {
    let mut h = Hasher::new();
    h.int(40, node.id as i64);
    appearance(node, &mut h);
    h.int(41, dep_values(node, state) as i64);
    h.int(42, node.children.len() as i64);
    for c in &node.children {
        h.int(43, c.id as i64);
        h.int(44, subtree_hash(c, state) as i64);
    }
    h.finish()
}
