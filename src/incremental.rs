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
    h.int(31, i64::from(p.repaint_boundary));
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
    h.int(45, i64::from(node.props.relayout_boundary));
    h.int(46, i64::from(node.props.fill_height));
    for c in &node.children {
        h.int(43, c.id as i64);
        h.int(44, subtree_hash(c, state) as i64);
    }
    h.finish()
}

/// Counters for the incremental pipeline. Asserted by tests, never timed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecomputeCounters {
    /// Nodes whose layout was actually recomputed this pass.
    pub layout_recomputed: usize,
    /// Nodes whose laid-out subtree was reused from cache this pass.
    pub layout_reused: usize,
    /// Nodes scheduled for repaint by the paint plan.
    pub paint_recomputed: usize,
    /// Nodes whose raster was kept by a repaint boundary.
    pub paint_reused: usize,
}

/// A bounded, deterministic LRU cache of laid-out subtrees. Keyed by stable
/// identity plus the layout memo key (§6); a miss affects speed only, never
/// output bytes. Eviction uses an access counter, not a wall clock.
pub struct LayoutCache {
    entries: std::collections::BTreeMap<String, (u64, Vec<crate::layout::LaidOut>, u64)>,
    order: Vec<String>,
    capacity: usize,
    tick: u64,
    /// Row key set by `place_for` for the current generated row.
    pub row_key: Option<String>,
    pub counters: RecomputeCounters,
    active: bool,
}

impl LayoutCache {
    pub fn new(capacity: usize) -> LayoutCache {
        LayoutCache {
            entries: std::collections::BTreeMap::new(),
            order: Vec::new(),
            capacity: capacity.max(1),
            tick: 0,
            row_key: None,
            counters: RecomputeCounters::default(),
            active: false,
        }
    }

    pub fn activate(&mut self) {
        self.active = true;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn reset_counters(&mut self) {
        self.counters = RecomputeCounters::default();
    }

    /// Layout memo key (§6): identity, subtree hash, resolved dep values,
    /// incoming constraint, viewport clip, rtl, breakpoint and clock phase.
    pub fn key(
        &self,
        id: &NodeIdentity,
        subtree: u64,
        deps: u64,
        rect: crate::layout::Rect,
        clip: Option<crate::layout::Rect>,
        rtl: bool,
        bp: u8,
        phase: i64,
    ) -> String {
        let c = clip
            .map(|r| format!("{},{},{},{}", r.x, r.y, r.w, r.h))
            .unwrap_or_else(|| "none".into());
        // The incoming constraint is the parent-given width/height, not the
        // absolute position. Absolute y is applied as a translation on a cache
        // hit, so inserting a For row reuses surviving rows' memo entries.
        format!(
            "{}|{:016x}|{:016x}|{},{}|{}|{}|{}|{}",
            id.key(),
            subtree,
            deps,
            rect.w,
            rect.h,
            c,
            u8::from(rtl),
            bp,
            phase
        )
    }

    /// Look up a cacheable subtree. Returns the stored segment and the delta-y
    /// to translate it by (rows shift when a sibling is inserted before them).
    pub fn get(&mut self, key: &str, hash: u64, new_y: i64) -> Option<Vec<crate::layout::LaidOut>> {
        let (stored_hash, seg, _last) = self.entries.get(key)?.clone();
        if stored_hash != hash || seg.is_empty() {
            return None;
        }
        self.tick += 1;
        if let Some(entry) = self.entries.get_mut(key) {
            entry.2 = self.tick;
        }
        let delta = new_y - seg[0].rect.y;
        let mut out = seg;
        if delta != 0 {
            for l in &mut out {
                l.rect.y += delta;
                if let Some((_, y)) = l.text_origin.as_mut() {
                    *y += delta;
                }
                if let Some(mut c) = l.clip {
                    c.y += delta;
                    l.clip = Some(c);
                }
            }
        }
        Some(out)
    }

    pub fn insert(&mut self, key: String, hash: u64, seg: Vec<crate::layout::LaidOut>) {
        self.tick += 1;
        self.entries.insert(key.clone(), (hash, seg, self.tick));
        if !self.order.contains(&key) {
            self.order.push(key.clone());
        }
        if self.order.len() > self.capacity {
            // Stable LRU: evict the oldest-accessed key; BTreeMap keeps the
            // remaining iteration order deterministic.
            let mut oldest = None;
            let mut oldest_tick = u64::MAX;
            for k in &self.order {
                let t = self.entries.get(k).map(|e| e.2).unwrap_or(0);
                if t < oldest_tick {
                    oldest_tick = t;
                    oldest = Some(k.clone());
                }
            }
            if let Some(k) = oldest {
                self.entries.remove(&k);
                self.order.retain(|x| x != &k);
            }
        }
    }

    /// Number of live memo entries (for tests and observability).
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Apply a changed-key set to a tree, honouring boundaries (§4 / §2.3).
/// Layout marks propagate upward to the nearest relayout boundary and stop;
/// paint marks propagate upward to the nearest repaint boundary and stop.
/// `layoutDirty` implies `paintDirty`; paint never promotes to layout.
/// A boundary node itself is dirty when a mark reaches it, but does not pass
/// the mark to its parent. Returns pre-order id lists of dirty nodes.
pub fn plan_dirty(
    node: &Node,
    changed: &std::collections::BTreeSet<String>,
) -> (Vec<String>, Vec<String>) {
    fn walk(
        node: &Node,
        changed: &std::collections::BTreeSet<String>,
        layout: &mut Vec<String>,
        paint: &mut Vec<String>,
    ) -> (bool, bool) {
        let own = deps(node).iter().any(|k| changed.contains(k));
        let mut child_layout = false;
        let mut child_paint = false;
        for c in &node.children {
            let (cl, cp) = walk(c, changed, layout, paint);
            child_layout |= cl;
            child_paint |= cp;
        }
        // The node is dirty if it changed directly or a child propagated up.
        let layout_dirty = own || child_layout;
        let paint_dirty = layout_dirty || child_paint;
        if layout_dirty {
            layout.push(format!("{}", node.id));
        }
        if paint_dirty {
            paint.push(format!("{}", node.id));
        }
        // A boundary consumes the mark: it does not escape to the parent.
        let up_layout = layout_dirty && !node.props.relayout_boundary;
        let up_paint = paint_dirty && !node.props.repaint_boundary;
        (up_layout, up_paint)
    }
    let mut layout = Vec::new();
    let mut paint = Vec::new();
    walk(node, changed, &mut layout, &mut paint);
    (layout, paint)
}

/// One node participating in the §2.1 two-level key diff. Pure data: matching
/// reads these records and never reassigns `static_id` or edits the IR.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyedNode {
    /// Global static id from the IR; stable and never changed by matching.
    pub static_id: u32,
    /// Node kind tag; cross-position key matching only pairs equal kinds.
    pub kind: &'static str,
    /// Additive reuse hint. `None` keeps the default index alignment.
    pub key: Option<String>,
}

/// §2.1 two-level alignment over one parent's children. Returns, for each
/// position in `next`, the position in `prev` whose cached layout/raster may be
/// reused (`None` = no match, lay out fresh).
///
/// - Default path (no keys): align by parent-child index against the static id,
///   exactly as today, so every golden stays byte-identical.
/// - Enhanced path: same-parent, same-kind nodes with equal, non-empty keys
///   match across positions; each previous node is consumed at most once.
/// - A key that finds no partner falls back to index alignment; a missing or
///   mismatched hint never errors. Static ids are never reassigned.
pub fn align_by_key(prev: &[KeyedNode], next: &[KeyedNode]) -> Vec<Option<usize>> {
    let mut used = vec![false; prev.len()];
    let mut out = vec![None; next.len()];

    // Enhanced pass: keyed, same-kind, cross-position matches.
    for (ni, node) in next.iter().enumerate() {
        let Some(key) = node.key.as_deref().filter(|k| !k.is_empty()) else {
            continue;
        };
        if let Some(pi) = prev.iter().enumerate().find_map(|(i, p)| {
            (!used[i] && p.kind == node.kind && p.key.as_deref() == Some(key)).then_some(i)
        }) {
            used[pi] = true;
            out[ni] = Some(pi);
        }
    }

    // Default pass: remaining slots align by index against the static id.
    for (ni, node) in next.iter().enumerate() {
        if out[ni].is_some() {
            continue;
        }
        if let Some(p) = prev.get(ni) {
            if !used[ni] && p.kind == node.kind {
                used[ni] = true;
                out[ni] = Some(ni);
            }
        }
    }
    out
}
