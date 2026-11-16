//! IR 0.1.0 parsing and validation into typed node/state/theme structs.
//!
//! JSON is parsed into `serde_json::Value` and then checked field by field, so a
//! malformed document always produces a readable `IrError` instead of a panic.
//!
//! The interactive UI and component catalog layers are additive: `theme.tokens`,
//! the optional node field `when`, the canonical style props, sixteen new node
//! kinds and the accessibility props are accepted. The visual fidelity layer
//! adds an optional `effects` array of typed software effects.
//! Unknown props are ignored and an unknown kind falls back to Container/Text,
//! so a file from a newer compiler still renders instead of failing.

use crate::anim::{Curve, TransitionKind};
use crate::effects::{
    VisualEffect, VisualEffectKind, EFFECT_MAX_OFFSET, EFFECT_MAX_RADIUS, EFFECT_UNIT,
    GAUSSIAN_MAX_RADIUS, GAUSSIAN_MIN_RADIUS,
};
use crate::font::FontMetrics;
use crate::gestures::GestureKind;
use crate::icons::IconPaths;
use crate::image::ImageFit;
use crate::interaction::{NodeState, Tone, Variant};
use crate::particles::{Particles, PARTICLE_MAX_COUNT, PARTICLE_MAX_RADIUS, PARTICLE_MAX_SEED};
use crate::Rgba;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrError(pub String);

impl std::fmt::Display for IrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for IrError {}

fn bad(msg: impl Into<String>) -> IrError {
    IrError(msg.into())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ir {
    pub version: String,
    pub app: String,
    pub state: Vec<StateDecl>,
    pub tree: Node,
    pub theme: Theme,
    /// Optional page-transition kind read from the optional top-level `transition`
    /// key. `None` means the IR declares no transition.
    pub transition: Option<TransitionKind>,
    /// Declared routes keyed by unique route name. Empty for a single-page
    /// document, where `tree` is the only page. Non-empty when the document
    /// declares `routes`; there is then no top-level tree and `tree` mirrors
    /// the initial route's content, so every existing single-tree consumer
    /// renders the entry page unchanged.
    pub routes: BTreeMap<String, Route>,
    /// The route `tree` mirrors when `routes` is non-empty; `None` otherwise.
    pub initial_route: Option<String>,
    /// Package icons from the optional top-level `icons` table, keyed by name
    /// and parsed once to integer 24x24 geometry. Empty when the IR carries no
    /// package icons, in which case the raster uses the built-in set.
    pub icons: BTreeMap<String, IconPaths>,
    /// Opt-in kernel visual-quality signals from the root `renderQuality`
    /// object. `None` keeps every pixel on the pinned default path.
    pub render_quality: Option<RenderQuality>,
    /// Root text direction from the optional `directionality` key. `Ltr` is the
    /// default when absent; there are no v1 subtree overrides.
    pub directionality: Directionality,
}

/// Root text direction (IR_ADDITIONS §15.2). `Ltr` is the default; the enum is
/// total so the raster can branch on it without an optional unwrap.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Directionality {
    #[default]
    Ltr,
    Rtl,
}

impl Directionality {
    pub fn parse(s: &str) -> Option<Directionality> {
        match s {
            "ltr" => Some(Directionality::Ltr),
            "rtl" => Some(Directionality::Rtl),
            _ => None,
        }
    }
}

/// Opt-in kernel visual-quality signals declared at the IR root. Each field is
/// independent and absent means the pinned default path. Unknown subfields are
/// ignored so a newer emitter can add hints without breaking this parser.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RenderQuality {
    /// 4x4 edge coverage; 4 is the only implemented sample count.
    pub msaa: Option<i64>,
    /// Gradient intermediate precision; 16 is the only dithered path.
    pub gradient_bits: Option<i64>,
    /// Vector path smoothing; "bezier" is the only implemented curve mode.
    pub curve: Option<String>,
    /// Stroke scaling; "physical" is the only implemented stroke mode.
    pub stroke: Option<String>,
    /// Text vertical placement; "grid" is the only implemented baseline mode.
    pub text_baseline: Option<String>,
}

impl RenderQuality {
    /// True when no quality signal is declared.
    pub fn is_default(&self) -> bool {
        self.msaa.is_none()
            && self.gradient_bits.is_none()
            && self.curve.is_none()
            && self.stroke.is_none()
            && self.text_baseline.is_none()
    }
}

/// One declared route: its content tree and optional title.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Route {
    pub tree: Node,
    pub title: Option<String>,
}

impl Ir {
    /// True when any node, visible or hidden, carries an effect prop.
    pub fn has_visual_effects(&self) -> bool {
        self.tree.has_visual_effects() || self.routes.values().any(|r| r.tree.has_visual_effects())
    }

    /// The declared route named `name`, with its tree and optional title.
    pub fn route(&self, name: &str) -> Option<&Route> {
        self.routes.get(name)
    }

    /// A copy of this IR whose root `tree` is the declared route `name`'s
    /// tree, so the existing single-tree layout/raster pipeline renders that
    /// page. `None` when the document has no routes or `name` is undeclared.
    pub fn with_route(&self, name: &str) -> Option<Ir> {
        let route = self.routes.get(name)?;
        let mut ir = self.clone();
        ir.tree = route.tree.clone();
        Some(ir)
    }

    /// Validate a `push` navigation target against the declared route set.
    /// A single-page document has no route set and accepts any target; a
    /// routes document rejects a target that is not a declared route.
    pub fn check_push_target(&self, target: &str) -> Result<(), IrError> {
        if self.routes.is_empty() || self.routes.contains_key(target) {
            return Ok(());
        }
        Err(bad(format!(
            "push target {target:?} is not a declared route (declared: {})",
            self.routes.keys().cloned().collect::<Vec<_>>().join(", ")
        )))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueType {
    Int,
    Bool,
    Str,
    /// Compile-time string list; expanded to literals before rendering, so it
    /// carries no runtime value and never appears in interpolation.
    List,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StateValue {
    Int(i64),
    Bool(bool),
    Str(String),
    List(Vec<String>),
}

impl StateValue {
    pub fn typ(&self) -> ValueType {
        match self {
            StateValue::Int(_) => ValueType::Int,
            StateValue::Bool(_) => ValueType::Bool,
            StateValue::Str(_) => ValueType::Str,
            StateValue::List(_) => ValueType::List,
        }
    }

    /// Text form used by `{name}` interpolation.
    pub fn as_text(&self) -> String {
        match self {
            StateValue::Int(n) => n.to_string(),
            StateValue::Bool(b) => b.to_string(),
            StateValue::Str(s) => s.clone(),
            // List state never interpolates; subscripts are lowered to literals.
            StateValue::List(_) => String::new(),
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            StateValue::Int(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            StateValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            StateValue::Str(s) => Some(s),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateDecl {
    pub name: String,
    pub typ: ValueType,
    pub init: StateValue,
}

/// The 32 IR node kinds (0.1.0 eight + interactive UI sixteen + data eight),
/// plus 68
/// extended primitive kinds carried as `Extended(code)` where `code` indexes
/// `crate::primitives::CONTRACTS`. `Other` is a kind this renderer does not know:
/// it is rendered with the documented fallback (Container with children, else
/// Text).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Column,
    Row,
    Container,
    Text,
    Button,
    Input,
    Image,
    Spacer,
    Card,
    Divider,
    Icon,
    Badge,
    Progress,
    Switch,
    Slider,
    Checkbox,
    Radio,
    Dialog,
    Tabs,
    List,
    Grid,
    Scroll,
    Avatar,
    Spinner,
    // Data primitives (drawn, not delegated to the fallback).
    Table,
    Chart,
    Calendar,
    RichText,
    Player,
    Map,
    Menu,
    Tooltip,
    /// Runtime list loop: lays out one template per element of the `source`
    /// list state, binding each element to the `item` name (DYNAMIC_LISTS).
    For,
    // Extended primitives, resolved through `crate::primitives`.
    Extended(u16),
    Other,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Column => "Column",
            Kind::Row => "Row",
            Kind::Container => "Container",
            Kind::Text => "Text",
            Kind::Button => "Button",
            Kind::Input => "Input",
            Kind::Image => "Image",
            Kind::Spacer => "Spacer",
            Kind::Card => "Card",
            Kind::Divider => "Divider",
            Kind::Icon => "Icon",
            Kind::Badge => "Badge",
            Kind::Progress => "Progress",
            Kind::Switch => "Switch",
            Kind::Slider => "Slider",
            Kind::Checkbox => "Checkbox",
            Kind::Radio => "Radio",
            Kind::Dialog => "Dialog",
            Kind::Tabs => "Tabs",
            Kind::List => "List",
            Kind::Grid => "Grid",
            Kind::Scroll => "Scroll",
            Kind::Avatar => "Avatar",
            Kind::Spinner => "Spinner",
            Kind::Table => "Table",
            Kind::Chart => "Chart",
            Kind::Calendar => "Calendar",
            Kind::RichText => "RichText",
            Kind::Player => "Player",
            Kind::Map => "Map",
            Kind::Menu => "Menu",
            Kind::Tooltip => "Tooltip",
            Kind::Extended(code) => crate::primitives::name_of(code),
            Kind::For => "For",
            Kind::Other => "Other",
        }
    }

    /// Unknown kinds never fail the parse: they degrade to `Other`. An
    /// extended primitive resolves to `Extended(code)` first.
    pub fn parse(s: &str) -> Kind {
        match s {
            "Column" => Kind::Column,
            "Row" => Kind::Row,
            "Container" => Kind::Container,
            "Text" => Kind::Text,
            "Button" => Kind::Button,
            "Input" => Kind::Input,
            "Image" => Kind::Image,
            "Spacer" => Kind::Spacer,
            "Card" => Kind::Card,
            "Divider" => Kind::Divider,
            "Icon" => Kind::Icon,
            "Badge" => Kind::Badge,
            "Progress" => Kind::Progress,
            "Switch" => Kind::Switch,
            "Slider" => Kind::Slider,
            "Checkbox" => Kind::Checkbox,
            "Radio" => Kind::Radio,
            "Dialog" => Kind::Dialog,
            "Tabs" => Kind::Tabs,
            "List" => Kind::List,
            "Grid" => Kind::Grid,
            "Scroll" => Kind::Scroll,
            "Avatar" => Kind::Avatar,
            "Spinner" => Kind::Spinner,
            "Table" => Kind::Table,
            "Chart" => Kind::Chart,
            "Calendar" => Kind::Calendar,
            "RichText" => Kind::RichText,
            "Player" => Kind::Player,
            "Map" => Kind::Map,
            "Menu" => Kind::Menu,
            "Tooltip" => Kind::Tooltip,
            "For" => Kind::For,
            _ => match crate::primitives::parse(s) {
                Some(code) => Kind::Extended(code),
                None => Kind::Other,
            },
        }
    }

    /// Kinds without children of their own.
    pub fn is_leaf(self) -> bool {
        matches!(
            self,
            Kind::Text
                | Kind::Button
                | Kind::Input
                | Kind::Image
                | Kind::Spacer
                | Kind::Divider
                | Kind::Icon
                | Kind::Badge
                | Kind::Progress
                | Kind::Switch
                | Kind::Slider
                | Kind::Checkbox
                | Kind::Radio
                | Kind::Avatar
                | Kind::Spinner
                | Kind::Table
                | Kind::Chart
                | Kind::Calendar
                | Kind::RichText
                | Kind::Player
                | Kind::Map
                | Kind::Menu
                | Kind::Tooltip
                | Kind::Extended(_)
        )
    }
}

/// Accessible role of a node. `None` is also an explicit `role: "none"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Button,
    Text,
    Image,
    Checkbox,
    Slider,
    Heading,
    None,
}

impl Role {
    pub fn name(self) -> &'static str {
        match self {
            Role::Button => "button",
            Role::Text => "text",
            Role::Image => "image",
            Role::Checkbox => "checkbox",
            Role::Slider => "slider",
            Role::Heading => "heading",
            Role::None => "none",
        }
    }

    pub fn parse(s: &str) -> Option<Role> {
        match s {
            "button" => Some(Role::Button),
            "text" => Some(Role::Text),
            "image" => Some(Role::Image),
            "checkbox" => Some(Role::Checkbox),
            "slider" => Some(Role::Slider),
            "heading" => Some(Role::Heading),
            "none" => Some(Role::None),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
}

/// Known props are typed; unknown props are ignored (IR forward compatibility).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Props {
    pub gap: i64,
    pub padding: i64,
    pub margin: i64,
    pub align: Align,
    /// Explicit text alignment. `None` keeps the historical inherited layout alignment.
    pub text_align: Option<Align>,
    /// Optional maximum wrapped line count for `Text` nodes (ellipsis on overflow).
    pub max_lines: Option<usize>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub radius: Option<i64>,
    pub color: Option<Rgba>,
    pub value: Option<String>,
    /// Integer form of `value` (`Progress` percentage).
    pub value_num: Option<i64>,
    /// Fractional form of `value` as fixed point (`Progress` fraction).
    pub value_frac: Option<i64>,
    pub label: Option<String>,
    pub title: Option<String>,
    pub name: Option<String>,
    pub placeholder: Option<String>,
    pub bind: Option<String>,
    /// Mask the bound `Input` value on screen (`secure: true`); kernel keeps it.
    pub secure: bool,
    /// PNG resource path for an `Image` node (relative paths use the process CWD).
    pub src: Option<String>,
    /// Integer aspect-ratio policy for an `Image` node (default `contain`).
    pub fit: ImageFit,
    pub size: Option<i64>,
    // Canonical style effects.
    pub shadow: Option<i64>,
    pub gradient: Option<(Rgba, Rgba)>,
    /// `gradientDirection: "horizontal"` sweeps the gradient along the rect's
    /// horizontal axis; the default (absent, or `"vertical"`) keeps top to
    /// bottom.
    pub gradient_horizontal: bool,
    pub glass: bool,
    /// Alpha in fixed point: 0 = transparent, 1024 = opaque.
    pub opacity: Option<i64>,
    pub border: Option<Rgba>,
    pub animate: Curve,
    /// Implicit-animation duration in milliseconds (`animate` prop).
    pub duration: Option<i64>,
    /// Implicit-animation start delay in milliseconds (`animate` prop).
    pub delay: Option<i64>,
    /// Shared-clock id; nodes with the same id share one animation timeline.
    pub anim_clock: Option<String>,
    /// `[begin,end]` slice of the shared clock period, in 0..1000.
    pub anim_interval: Option<(i64, i64)>,
    // Kind props.
    pub columns: Option<i64>,
    /// Column count the compact tier should keep; `None` stacks to one column.
    pub compact_columns: Option<i64>,
    /// Columns a direct Grid child spans; `None` means one column.
    pub column_span: Option<i64>,
    /// Exact text typography, opt-in and orthogonal to the quantized raster path.
    pub weight: Option<String>,
    pub letter_spacing: Option<i64>,
    pub font_px: Option<i64>,
    pub labels: Vec<String>,
    pub min: Option<i64>,
    pub max: Option<i64>,
    // Accessibility props.
    /// Fixed-point font multiplier: 1024 == 1.0.
    pub font_scale: Option<i64>,
    /// Per-node text metrics override; `None` inherits `theme.fontMetrics`.
    pub font_metrics: Option<FontMetrics>,
    pub high_contrast: bool,
    pub role: Option<Role>,
    /// Optional interactive gesture: `tap`, `doubleTap`, `longPress` or `drag`.
    pub gesture: Option<GestureKind>,
    /// `For` node: the list state name to iterate (DYNAMIC_LISTS).
    pub source: Option<String>,
    /// `For` node: the loop variable name bound per element inside the template.
    pub item: Option<String>,
    /// Resolved five-state matrix state; `Default` when the prop is absent.
    pub state: NodeState,
    /// `disabled` wins over `state` and over a caller-supplied override.
    pub disabled: bool,
    /// Optional design-system variant and semantic feedback tone.
    pub variant: Option<Variant>,
    pub tone: Option<Tone>,
    // Primitive data props.
    /// Integer series for `Chart` bar heights (also usable by other primitives).
    pub values: Vec<i64>,
    /// Row count for `Table` / `Calendar` grids.
    pub rows: Option<i64>,
    // Optional software visual effects.
    pub effects: Vec<VisualEffect>,
    // Optional deterministic particle field.
    pub particles: Option<Particles>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Set,
    Add,
    Sub,
    Toggle,
    /// Append one element to a list state (`items.append(value)`).
    Append,
    /// Remove one element of a list state by index (`items.removeAt(i)`).
    RemoveAt,
    /// Replace a list state wholesale (`items = ...`); `set` may not target a
    /// list (DYNAMIC_LISTS aligns with the core kernel's op contract).
    Assign,
    /// Navigate forward to the route named in `target`; never changes state.
    Push,
    /// Navigate back one page; carries no target and never changes state.
    Pop,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    pub op: Op,
    pub target: String,
    pub value: Option<StateValue>,
    /// Read the value from another state at execution time.
    pub value_from: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub name: String,
    /// Handlers may hold several actions; 0.1.0 handlers hold exactly one.
    pub actions: Vec<Action>,
}

impl Event {
    pub fn action(&self) -> &Action {
        &self.actions[0]
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub id: u32,
    pub kind: Kind,
    pub props: Props,
    pub children: Vec<Node>,
    pub events: Vec<Event>,
    /// The node is visible only while this bool state is true.
    pub when: Option<String>,
    /// Inverted `when`, so one bool state can select between two sibling subtrees.
    pub unless: Option<String>,
}

impl Node {
    pub fn has_visual_effects(&self) -> bool {
        !self.props.effects.is_empty() || self.children.iter().any(|c| c.has_visual_effects())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
    System,
}

/// Theme tokens; missing tokens fall back to `mode`/`accent`/`radius`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Contrast {
    Normal,
    High,
}

impl Default for Contrast {
    fn default() -> Self {
        Contrast::Normal
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThemeTokens {
    pub surface: Option<Rgba>,
    pub on_surface: Option<Rgba>,
    /// Flat tinted page backdrop a glass lamina refracts. Distinct from
    /// `surface`, which remains the contrast reference for on-surface ink.
    pub stage: Option<Rgba>,
    /// Semantic tone colours consumed by `Palette::tone_color`; missing tokens
    /// fall back to the measured interaction constants.
    pub success: Option<Rgba>,
    pub warning: Option<Rgba>,
    pub error: Option<Rgba>,
    pub info: Option<Rgba>,
    /// Optional brand tokens; reserved for callers that paint from them.
    pub primary: Option<Rgba>,
    pub secondary: Option<Rgba>,
    pub font: Option<String>,
    pub contrast: Contrast,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Theme {
    pub mode: ThemeMode,
    pub accent: Rgba,
    pub radius: i64,
    pub tokens: ThemeTokens,
    /// Global text scale (fixed point, 1024 == 1.0); nodes without their own
    /// `fontScale` prop inherit this value.
    pub font_scale: Option<i64>,
    /// Global text metrics; nodes without their own `fontMetrics` prop inherit
    /// it. `Fixed` keeps committed frames byte-identical.
    pub font_metrics: FontMetrics,
}

/// Flat, ordered state map consumed by text interpolation and by the CLI.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StateView {
    pub values: BTreeMap<String, StateValue>,
}

impl StateView {
    pub fn from_ir(ir: &Ir) -> Self {
        let mut values = BTreeMap::new();
        for decl in &ir.state {
            values.insert(decl.name.clone(), decl.init.clone());
        }
        StateView { values }
    }

    pub fn get(&self, name: &str) -> Option<&StateValue> {
        self.values.get(name)
    }

    pub fn set(&mut self, name: impl Into<String>, value: StateValue) {
        self.values.insert(name.into(), value);
    }

    /// Resolve `{name}` against current state; `{{` is a literal `{` (and `}}` a `}`).
    /// Unknown names are left verbatim instead of panicking.
    pub fn interpolate(&self, text: &str) -> String {
        let b = text.as_bytes();
        let mut out = String::with_capacity(text.len());
        let mut i = 0usize;
        while i < b.len() {
            if b[i] == b'{' {
                if i + 1 < b.len() && b[i + 1] == b'{' {
                    out.push('{');
                    i += 2;
                    continue;
                }
                if let Some(rel) = text[i + 1..].find('}') {
                    let end = i + 1 + rel;
                    let name = &text[i + 1..end];
                    match self.values.get(name) {
                        Some(v) => out.push_str(&v.as_text()),
                        None => {
                            out.push('{');
                            out.push_str(name);
                            out.push('}');
                        }
                    }
                    i = end + 1;
                    continue;
                }
            } else if b[i] == b'}' && i + 1 < b.len() && b[i + 1] == b'}' {
                out.push('}');
                i += 2;
                continue;
            }
            let ch = text[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
        out
    }
}

fn object<'a>(v: &'a Value, what: &str) -> Result<&'a Map<String, Value>, IrError> {
    v.as_object()
        .ok_or_else(|| bad(format!("{what} must be a JSON object")))
}

fn required<'a>(o: &'a Map<String, Value>, key: &str, what: &str) -> Result<&'a Value, IrError> {
    o.get(key)
        .ok_or_else(|| bad(format!("{what} is missing required field {key:?}")))
}

fn string_field(o: &Map<String, Value>, key: &str, what: &str) -> Result<String, IrError> {
    required(o, key, what)?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| bad(format!("{what}.{key} must be a string")))
}

/// Integers only; an integral JSON float is accepted at the boundary and
/// immediately reduced to i64 so layout/raster never see a float.
fn integer(v: &Value, what: &str) -> Result<i64, IrError> {
    if let Some(n) = v.as_i64() {
        return Ok(n);
    }
    if let Some(f) = v.as_f64() {
        if f.is_finite() && f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64 {
            return Ok(f as i64);
        }
    }
    Err(bad(format!("{what} must be an integer")))
}

fn opt_integer(o: &Map<String, Value>, key: &str, what: &str) -> Result<Option<i64>, IrError> {
    match o.get(key) {
        Some(v) => {
            let n = integer(v, &format!("{what}.{key}"))?;
            if n < 0 {
                return Err(bad(format!("{what}.{key} must be non-negative")));
            }
            Ok(Some(n))
        }
        None => Ok(None),
    }
}

/// Like `opt_integer` but negative values are allowed (slider bounds).
fn opt_signed_integer(
    o: &Map<String, Value>,
    key: &str,
    what: &str,
) -> Result<Option<i64>, IrError> {
    match o.get(key) {
        Some(v) => Ok(Some(integer(v, &format!("{what}.{key}"))?)),
        None => Ok(None),
    }
}

/// Optional implicit-animation `duration`/`delay` prop: a non-negative
/// millisecond count within `ANIMATE_MAX_MS`. An absent prop is `None`, so the
/// renderer applies its default duration and a zero delay.
fn anim_duration_prop(
    o: &Map<String, Value>,
    key: &str,
    what: &str,
) -> Result<Option<i64>, IrError> {
    match opt_integer(o, key, what)? {
        Some(n) if n > crate::anim::ANIMATE_MAX_MS => Err(bad(format!(
            "{what}.{key} must be between 0 and {}",
            crate::anim::ANIMATE_MAX_MS
        ))),
        other => Ok(other),
    }
}

fn opt_string(o: &Map<String, Value>, key: &str, what: &str) -> Result<Option<String>, IrError> {
    match o.get(key) {
        Some(v) => v
            .as_str()
            .map(|s| Some(s.to_string()))
            .ok_or_else(|| bad(format!("{what}.{key} must be a string"))),
        None => Ok(None),
    }
}

/// A text prop: strings and numbers are both legal (numbers keep their literal
/// spelling, so `value: 50` becomes "50").
fn opt_text(o: &Map<String, Value>, key: &str, what: &str) -> Result<Option<String>, IrError> {
    match o.get(key) {
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(v @ Value::Number(_)) => Ok(Some(v.to_string())),
        Some(_) => Err(bad(format!("{what}.{key} must be a string or a number"))),
        None => Ok(None),
    }
}

/// A numeric `value` prop: integers become percentages, fractional literals
/// become a 0..1 fraction. Both are stored as fixed point.
fn numeric_value(
    o: &Map<String, Value>,
    key: &str,
    what: &str,
) -> Result<(Option<i64>, Option<i64>), IrError> {
    let Some(v) = o.get(key) else {
        return Ok((None, None));
    };
    let Some(number) = v.as_number() else {
        return Ok((None, None));
    };
    let text = number.to_string();
    let (negative, mant, scale) =
        decimal_parts(&text).ok_or_else(|| bad(format!("{what}.{key} must be a number")))?;
    let _ = (negative, mant);
    let fixed = fixed_from_parts((negative, mant, scale), 1024)
        .ok_or_else(|| bad(format!("{what}.{key} is too large")))?;
    let int = fixed / 1024;
    // A fractional literal is a 0..1 fraction; a whole number is a percentage.
    let frac = if scale > 0 {
        fixed.clamp(0, 1024)
    } else {
        (fixed * 1024 / 100).clamp(0, 1024)
    };
    Ok((Some(int), Some(frac)))
}

fn opt_bool(o: &Map<String, Value>, key: &str, what: &str) -> Result<bool, IrError> {
    match o.get(key) {
        Some(v) => v
            .as_bool()
            .ok_or_else(|| bad(format!("{what}.{key} must be a boolean"))),
        None => Ok(false),
    }
}

fn color_prop(o: &Map<String, Value>, key: &str, what: &str) -> Result<Option<Rgba>, IrError> {
    match opt_string(o, key, what)? {
        Some(s) => Ok(Some(Rgba::from_hex(&s).ok_or_else(|| {
            bad(format!("{what}.{key} must be #RRGGBB, got {s:?}"))
        })?)),
        None => Ok(None),
    }
}

/// Split a decimal literal into `(negative, mantissa, scale)` so `0.95` becomes
/// an exact fixed-point value without touching floating point.
fn decimal_parts(text: &str) -> Option<(bool, i128, i32)> {
    let b = text.as_bytes();
    let mut i = 0usize;
    let negative = match b.first() {
        Some(b'-') => {
            i = 1;
            true
        }
        Some(b'+') => {
            i = 1;
            false
        }
        _ => false,
    };
    let mut mant: i128 = 0;
    let mut scale: i32 = 0;
    let mut digits = 0usize;
    let mut any = false;
    while i < b.len() && b[i].is_ascii_digit() {
        mant = mant.checked_mul(10)?.checked_add((b[i] - b'0') as i128)?;
        digits += 1;
        any = true;
        i += 1;
        if digits > 18 {
            return None;
        }
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            mant = mant.checked_mul(10)?.checked_add((b[i] - b'0') as i128)?;
            scale += 1;
            digits += 1;
            any = true;
            i += 1;
            if digits > 18 {
                return None;
            }
        }
    }
    if !any {
        return None;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        let exp_neg = match b.get(i) {
            Some(b'-') => {
                i += 1;
                true
            }
            Some(b'+') => {
                i += 1;
                false
            }
            _ => false,
        };
        let mut exp: i32 = 0;
        while i < b.len() && b[i].is_ascii_digit() {
            exp = exp.checked_mul(10)?.checked_add((b[i] - b'0') as i32)?;
            i += 1;
        }
        if i != b.len() {
            return None;
        }
        scale = if exp_neg {
            scale.checked_add(exp)?
        } else {
            scale.checked_sub(exp)?
        };
    } else if i != b.len() {
        return None;
    }
    Some((negative, mant, scale))
}

/// Convert a decimal literal into fixed point with the given unit
/// (`unit == 1024` means 1.0 becomes 1024). Integer-only, exact for the
/// literals the IR allows.
fn fixed_from_parts(parts: (bool, i128, i32), unit: i64) -> Option<i64> {
    let (negative, mant, scale) = parts;
    let mut fixed: i128 = if scale >= 0 {
        let mut den: i128 = 1;
        for _ in 0..scale {
            den = den.checked_mul(10)?;
        }
        let num = mant.checked_mul(unit as i128)?;
        (num + den / 2) / den
    } else {
        let mut mul: i128 = 1;
        for _ in 0..(-scale) {
            mul = mul.checked_mul(10)?;
        }
        mant.checked_mul(unit as i128)?.checked_mul(mul)?
    };
    if negative {
        fixed = -fixed;
    }
    if fixed > i64::MAX as i128 || fixed < i64::MIN as i128 {
        return None;
    }
    Some(fixed as i64)
}

/// Fixed-point unit value (0..=1024) from a JSON number in 0..1.
fn unit_prop(o: &Map<String, Value>, key: &str, what: &str) -> Result<Option<i64>, IrError> {
    let Some(v) = o.get(key) else {
        return Ok(None);
    };
    let text = v
        .as_number()
        .map(|n| n.to_string())
        .ok_or_else(|| bad(format!("{what}.{key} must be a number between 0 and 1")))?;
    let parts = decimal_parts(&text)
        .ok_or_else(|| bad(format!("{what}.{key} must be a number between 0 and 1")))?;
    let fixed = fixed_from_parts(parts, 1024)
        .ok_or_else(|| bad(format!("{what}.{key} must be between 0 and 1")))?;
    if !(0..=1024).contains(&fixed) {
        return Err(bad(format!("{what}.{key} must be between 0 and 1")));
    }
    Ok(Some(fixed))
}

fn font_scale_diag(what: &str, key: &str) -> IrError {
    bad(format!("{what}.{key} must be a number between 0.5 and 3.0"))
}

/// `fontScale` is a JSON number in 0.5..3.0, stored as exact fixed point
/// (1024 == 1.0). The range test compares the decimal parts, not the rounded
/// fixed-point value, so a value just outside the range is rejected.
fn font_scale_prop(o: &Map<String, Value>, key: &str, what: &str) -> Result<Option<i64>, IrError> {
    let Some(v) = o.get(key) else {
        return Ok(None);
    };
    let text = v
        .as_number()
        .map(|n| n.to_string())
        .ok_or_else(|| font_scale_diag(what, key))?;
    let parts = decimal_parts(&text).ok_or_else(|| font_scale_diag(what, key))?;
    if !font_scale_in_range(parts) {
        return Err(font_scale_diag(what, key));
    }
    Ok(Some(
        fixed_from_parts(parts, 1024).ok_or_else(|| font_scale_diag(what, key))?,
    ))
}

/// Exact comparison of `mant / 10^scale` with 0.5 and 3 (no float rounding).
fn font_scale_in_range(parts: (bool, i128, i32)) -> bool {
    let (negative, mant, scale) = parts;
    if negative || mant == 0 || !(0..=18).contains(&scale) {
        return false;
    }
    let mut den: i128 = 1;
    for _ in 0..scale {
        den *= 10;
    }
    2 * mant >= den && mant <= 3 * den
}

/// `#RRGGBB,#RRGGBB` -> the two gradient stops.
fn gradient_prop(
    o: &Map<String, Value>,
    key: &str,
    what: &str,
) -> Result<Option<(Rgba, Rgba)>, IrError> {
    let Some(s) = opt_string(o, key, what)? else {
        return Ok(None);
    };
    let (a, b) = s
        .split_once(',')
        .ok_or_else(|| bad(format!("{what}.{key} must be \"#RRGGBB,#RRGGBB\"")))?;
    let parse = |part: &str| {
        Rgba::from_hex(part.trim())
            .ok_or_else(|| bad(format!("{what}.{key} must be \"#RRGGBB,#RRGGBB\"")))
    };
    Ok(Some((parse(a)?, parse(b)?)))
}

/// Tab labels: an array of strings or one comma-separated string.
fn labels_prop(o: &Map<String, Value>, key: &str, what: &str) -> Result<Vec<String>, IrError> {
    match o.get(key) {
        None => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| bad(format!("{what}.{key} must be an array of strings")))
            })
            .collect(),
        Some(Value::String(s)) => Ok(s
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect()),
        Some(_) => Err(bad(format!("{what}.{key} must be an array of strings"))),
    }
}

/// Integer series: an array of JSON integers or one comma-separated string.
/// Deterministic: no floats reach the raster; a fractional literal is rejected.
fn values_prop(o: &Map<String, Value>, key: &str, what: &str) -> Result<Vec<i64>, IrError> {
    match o.get(key) {
        None => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .enumerate()
            .map(|(i, v)| integer(v, &format!("{what}.{key}[{i}]")))
            .collect(),
        Some(Value::String(s)) => s
            .split(',')
            .map(|p| p.trim())
            .filter(|p| !p.is_empty())
            .map(|p| {
                p.parse::<i64>()
                    .map_err(|_| bad(format!("{what}.{key} must be a list of integers")))
            })
            .collect(),
        Some(_) => Err(bad(format!(
            "{what}.{key} must be an array of integers or a comma-separated string"
        ))),
    }
}

fn effect_integer(
    o: &Map<String, Value>,
    key: &str,
    what: &str,
    default: i64,
) -> Result<i64, IrError> {
    match o.get(key) {
        Some(v) => integer(v, &format!("{what}.{key}")),
        None => Ok(default),
    }
}

/// Optional `[begin,end]` shared-clock slice in 0..1000. Invalid slices are
/// ignored here (the compiler rejects them); render degrades safely to no slice.
fn anim_interval_prop(o: &Map<String, Value>, what: &str) -> Result<Option<(i64, i64)>, IrError> {
    let Some(v) = o.get("animInterval") else {
        return Ok(None);
    };
    let Some(items) = v.as_array() else {
        return Ok(None);
    };
    if items.len() != 2 {
        return Ok(None);
    }
    let b = match integer(&items[0], &format!("{what}.animInterval[0]")) {
        Ok(n) => n,
        Err(_) => return Ok(None),
    };
    let e = match integer(&items[1], &format!("{what}.animInterval[1]")) {
        Ok(n) => n,
        Err(_) => return Ok(None),
    };
    if (0..=1000).contains(&b) && (0..=1000).contains(&e) && e > b {
        Ok(Some((b, e)))
    } else {
        Ok(None)
    }
}

/// Validate the optional `effects` array. Unknown effect fields are ignored
/// like other IR props; numeric fields are clamped here so raster arithmetic
/// never sees an unbounded value.
fn effects_prop(o: &Map<String, Value>, what: &str) -> Result<Vec<VisualEffect>, IrError> {
    let Some(v) = o.get("effects") else {
        return Ok(Vec::new());
    };
    let items = v
        .as_array()
        .ok_or_else(|| bad(format!("{what}.effects must be an array")))?;
    let mut out = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let path = format!("{what}.effects[{i}]");
        let fx = object(item, &path)?;
        let kind_s = string_field(fx, "kind", &path)?;
        let kind = VisualEffectKind::parse(&kind_s)
            .ok_or_else(|| bad(format!("{path}.kind has unknown value {kind_s:?}")))?;
        let radius = effect_integer(fx, "radius", &path, 3)?.clamp(0, EFFECT_MAX_RADIUS);
        if kind == VisualEffectKind::Gaussian
            && !(GAUSSIAN_MIN_RADIUS..=GAUSSIAN_MAX_RADIUS).contains(&radius)
        {
            return Err(bad(format!(
                "{path}: gaussian radius {radius} must be between {GAUSSIAN_MIN_RADIUS} and {GAUSSIAN_MAX_RADIUS}"
            )));
        }
        let dx = effect_integer(fx, "dx", &path, radius / 2)?
            .clamp(-EFFECT_MAX_OFFSET, EFFECT_MAX_OFFSET);
        let dy = effect_integer(fx, "dy", &path, radius / 2)?
            .clamp(-EFFECT_MAX_OFFSET, EFFECT_MAX_OFFSET);
        let alpha = effect_integer(fx, "alpha", &path, EFFECT_UNIT / 2)?.clamp(0, EFFECT_UNIT);
        let color = color_prop(fx, "color", &path)?;
        out.push(VisualEffect {
            kind,
            radius,
            dx,
            dy,
            alpha,
            color,
        });
    }
    Ok(out)
}

/// The optional `particles` prop. Every numeric field is a bounded integer
/// rejected at parse time when out of range, so raster arithmetic never sees
/// an unbounded value and malformed documents get a readable diagnostic.
fn particles_prop(o: &Map<String, Value>, what: &str) -> Result<Option<Particles>, IrError> {
    let Some(v) = o.get("particles") else {
        return Ok(None);
    };
    let path = format!("{what}.particles");
    let p = object(v, &path)?;
    let count = match p.get("count") {
        Some(v) => {
            let n = integer(v, &format!("{path}.count"))?;
            if !(0..=PARTICLE_MAX_COUNT).contains(&n) {
                return Err(bad(format!(
                    "{path}.count {n} must be 0..={PARTICLE_MAX_COUNT}"
                )));
            }
            n
        }
        None => return Err(bad(format!("{path} is missing required field \"count\""))),
    };
    let field = |key: &str, lo: i64, hi: i64, default: i64| -> Result<i64, IrError> {
        match p.get(key) {
            Some(v) => {
                let n = integer(v, &format!("{path}.{key}"))?;
                if !(lo..=hi).contains(&n) {
                    return Err(bad(format!("{path}.{key} {n} must be {lo}..={hi}")));
                }
                Ok(n)
            }
            None => Ok(default),
        }
    };
    Ok(Some(Particles {
        count,
        seed: field("seed", 0, PARTICLE_MAX_SEED, 1)?,
        radius: field("radius", 0, PARTICLE_MAX_RADIUS, 2)?,
        alpha: field("alpha", 0, EFFECT_UNIT, EFFECT_UNIT)?,
        color: color_prop(p, "color", &path)?,
    }))
}

fn state_value(v: &Value, expected: Option<ValueType>, what: &str) -> Result<StateValue, IrError> {
    let value = if let Some(arr) = v.as_array() {
        let mut items = Vec::with_capacity(arr.len());
        for (i, item) in arr.iter().enumerate() {
            items.push(
                item.as_str()
                    .ok_or_else(|| bad(format!("{what}[{i}] must be a string")))?
                    .to_string(),
            );
        }
        StateValue::List(items)
    } else if let Some(s) = v.as_str() {
        StateValue::Str(s.to_string())
    } else if let Some(b) = v.as_bool() {
        StateValue::Bool(b)
    } else {
        StateValue::Int(integer(v, what)?)
    };
    if let Some(t) = expected {
        if value.typ() != t {
            return Err(bad(format!(
                "{what} type does not match declared type {t:?}"
            )));
        }
    }
    Ok(value)
}

fn state_decls(v: &Value) -> Result<Vec<StateDecl>, IrError> {
    let arr = v
        .as_array()
        .ok_or_else(|| bad("IR root.state must be an array"))?;
    let mut seen = BTreeSet::new();
    let mut out = Vec::with_capacity(arr.len());
    for (i, item) in arr.iter().enumerate() {
        let what = format!("state[{i}]");
        let o = object(item, &what)?;
        let name = string_field(o, "name", &what)?;
        if name.is_empty() {
            return Err(bad(format!("{what}.name must not be empty")));
        }
        if !seen.insert(name.clone()) {
            return Err(bad(format!("duplicate state name {name:?}")));
        }
        let typ = match string_field(o, "type", &what)?.as_str() {
            "int" => ValueType::Int,
            "bool" => ValueType::Bool,
            "string" => ValueType::Str,
            "list" => ValueType::List,
            other => return Err(bad(format!("{what}.type has unknown value {other:?}"))),
        };
        let init = state_value(
            required(o, "init", &what)?,
            Some(typ),
            &format!("{what}.init"),
        )?;
        out.push(StateDecl { name, typ, init });
    }
    Ok(out)
}

fn props(o: &Map<String, Value>) -> Result<Props, IrError> {
    let what = "props";
    let align_value = opt_string(o, "align", what)?;
    let align = match align_value.as_deref() {
        None | Some("start") | Some("left") | Some("top") => Align::Start,
        Some("center") | Some("middle") => Align::Center,
        Some("end") | Some("right") | Some("bottom") => Align::End,
        Some(other) => return Err(bad(format!("{what}.align has unknown value {other:?}"))),
    };
    let text_align = align_value.map(|_| align);
    let fit = match opt_string(o, "fit", what)?.as_deref() {
        None => ImageFit::default(),
        Some("fill") => ImageFit::Fill,
        Some("contain") => ImageFit::Contain,
        Some("cover") => ImageFit::Cover,
        Some(other) => return Err(bad(format!("{what}.fit has unknown value {other:?}"))),
    };
    let shadow = match opt_integer(o, "shadow", what)? {
        Some(n) if n > 8 => {
            return Err(bad(format!("{what}.shadow must be between 0 and 8")));
        }
        other => other,
    };
    let animate = match opt_string(o, "animate", what)?.as_deref() {
        None => Curve::None,
        Some(s) => {
            Curve::parse(s).ok_or_else(|| bad(format!("{what}.animate has unknown value {s:?}")))?
        }
    };
    let (value_num, value_frac) = numeric_value(o, "value", what)?;
    let role = match opt_string(o, "role", what)? {
        Some(s) => Some(
            Role::parse(&s).ok_or_else(|| bad(format!("{what}.role has unknown value {s:?}")))?,
        ),
        None => None,
    };
    let gesture = match opt_string(o, "gesture", what)? {
        Some(s) => Some(
            GestureKind::parse(&s)
                .ok_or_else(|| bad(format!("{what}.gesture has unknown value {s:?}")))?,
        ),
        None => None,
    };
    let source = match o.get("source") {
        None => None,
        Some(v) => Some(
            v.as_str()
                .ok_or_else(|| bad(format!("{what}.source must be a string")))?
                .to_string(),
        ),
    };
    let item = match o.get("item") {
        None => None,
        Some(v) => Some(
            v.as_str()
                .ok_or_else(|| bad(format!("{what}.item must be a string")))?
                .to_string(),
        ),
    };
    let state = match opt_string(o, "state", what)? {
        Some(s) => NodeState::parse(&s).ok_or_else(|| {
            bad(format!(
                "{what}.state has unknown value {s:?}: expected default, hover, active, focus or disabled"
            ))
        })?,
        None => NodeState::default(),
    };
    let variant = match opt_string(o, "variant", what)? {
        Some(s) => Some(Variant::parse(&s).ok_or_else(|| {
            bad(format!(
                "{what}.variant has unknown value {s:?}: expected primary, secondary, text, danger, ghost or fab"
            ))
        })?),
        None => None,
    };
    let tone = match opt_string(o, "tone", what)? {
        Some(s) => Some(Tone::parse(&s).ok_or_else(|| {
            bad(format!(
                "{what}.tone has unknown value {s:?}: expected success, warning, error, info or neutral"
            ))
        })?),
        None => None,
    };
    let font_metrics = match opt_string(o, "fontMetrics", what)? {
        Some(s) => Some(FontMetrics::parse(&s).ok_or_else(|| {
            bad(format!(
                "{what}.fontMetrics has unknown value {s:?}: expected fixed or proportional"
            ))
        })?),
        None => None,
    };
    let max_lines =
        opt_integer(o, "maxLines", what)?.map(|n| usize::try_from(n).unwrap_or(usize::MAX));
    Ok(Props {
        gap: opt_integer(o, "gap", what)?.unwrap_or(0),
        padding: opt_integer(o, "padding", what)?.unwrap_or(0),
        margin: opt_integer(o, "margin", what)?.unwrap_or(0),
        align,
        text_align,
        max_lines,
        width: opt_integer(o, "width", what)?,
        height: opt_integer(o, "height", what)?,
        radius: opt_integer(o, "radius", what)?,
        color: color_prop(o, "color", what)?,
        value: opt_text(o, "value", what)?,
        value_num,
        value_frac,
        label: opt_string(o, "label", what)?,
        title: opt_string(o, "title", what)?,
        name: opt_string(o, "name", what)?,
        placeholder: opt_string(o, "placeholder", what)?,
        bind: opt_string(o, "bind", what)?,
        secure: opt_bool(o, "secure", what)?,
        src: opt_string(o, "src", what)?,
        fit,
        size: opt_integer(o, "size", what)?,
        shadow,
        gradient: gradient_prop(o, "gradient", what)?,
        gradient_horizontal: match opt_string(o, "gradientDirection", what)?.as_deref() {
            None => false,
            Some("horizontal") => true,
            Some("vertical") => false,
            Some(other) => {
                return Err(bad(format!(
                    "{what}.gradientDirection has unknown value {other:?}: expected vertical or horizontal"
                )))
            }
        },
        glass: opt_bool(o, "glass", what)?,
        opacity: unit_prop(o, "opacity", what)?,
        border: color_prop(o, "border", what)?,
        animate,
        duration: anim_duration_prop(o, "duration", what)?,
        delay: anim_duration_prop(o, "delay", what)?,
        anim_clock: opt_string(o, "animClock", what)?,
        anim_interval: anim_interval_prop(o, what)?,
        columns: opt_integer(o, "columns", what)?,
        compact_columns: opt_integer(o, "compactColumns", what)?,
        column_span: opt_integer(o, "columnSpan", what)?,
        weight: opt_string(o, "weight", what)?,
        letter_spacing: opt_signed_integer(o, "letterSpacing", what)?,
        font_px: match opt_integer(o, "fontPx", what)? {
            Some(0) => return Err(bad(format!("{what}.fontPx must be a positive integer"))),
            other => other,
        },
        labels: labels_prop(o, "labels", what)?,
        min: opt_signed_integer(o, "min", what)?,
        max: opt_signed_integer(o, "max", what)?,
        font_scale: font_scale_prop(o, "fontScale", what)?,
        font_metrics,
        high_contrast: opt_bool(o, "highContrast", what)?,
        role,
        gesture,
        state,
        disabled: opt_bool(o, "disabled", what)?,
        variant,
        tone,
        values: values_prop(o, "values", what)?,
        rows: opt_integer(o, "rows", what)?,
        effects: effects_prop(o, what)?,
        particles: particles_prop(o, what)?,
        source,
        item,
    })
}

/// BCP-47 primary subtag plus optional 2-8 char subtags (IR_ADDITIONS §15.1).
/// Mirrors the core/kernel validator so `set @locale` accepts the same tags.
fn valid_locale_tag(tag: &str) -> bool {
    let mut parts = tag.split('-');
    let Some(primary) = parts.next() else {
        return false;
    };
    (2..=3).contains(&primary.len())
        && primary.bytes().all(|b| b.is_ascii_lowercase())
        && parts.all(|part| {
            (2..=8).contains(&part.len()) && part.bytes().all(|b| b.is_ascii_alphanumeric())
        })
}

fn action(a: &Map<String, Value>, what: &str, decls: &[StateDecl]) -> Result<Action, IrError> {
    let op = match string_field(a, "op", what)?.as_str() {
        "set" => Op::Set,
        "add" => Op::Add,
        "sub" => Op::Sub,
        "toggle" => Op::Toggle,
        "append" => Op::Append,
        "removeAt" => Op::RemoveAt,
        "assign" => Op::Assign,
        "push" => Op::Push,
        "pop" => Op::Pop,
        other => return Err(bad(format!("{what}.op has unknown value {other:?}"))),
    };
    if matches!(op, Op::Push | Op::Pop) {
        // Navigation actions address pages, not state: push carries the route
        // name in `target`, pop carries nothing, and neither has a value.
        if a.get("value").is_some() || a.get("valueFrom").is_some() {
            return Err(bad(format!(
                "{what}: {} takes no value",
                if op == Op::Push { "push" } else { "pop" }
            )));
        }
        let target = match op {
            Op::Push => {
                let target = string_field(a, "target", what)?;
                if target.is_empty() {
                    return Err(bad(format!("{what}: push needs a non-empty route target")));
                }
                target
            }
            _ => {
                if a.get("target").is_some() {
                    return Err(bad(format!("{what}: pop takes no target")));
                }
                String::new()
            }
        };
        return Ok(Action {
            op,
            target,
            value: None,
            value_from: None,
        });
    }
    let target = string_field(a, "target", what)?;
    // `set @locale` is the compiler's event spelling of the reserved locale
    // patch (IR_ADDITIONS §15.4). It is not a state name, so it bypasses the
    // declared-state lookup; every other op keeps that lookup unchanged.
    if op == Op::Set && target == "@locale" {
        if a.get("valueFrom").is_some() {
            return Err(bad(format!(
                "{what}: @locale takes a literal tag, not valueFrom"
            )));
        }
        let tag = a
            .get("value")
            .and_then(Value::as_str)
            .ok_or_else(|| bad(format!("{what}: @locale value must be a string")))?;
        if !valid_locale_tag(tag) {
            return Err(bad(format!("{what}: {tag:?} is not a BCP-47 tag")));
        }
        return Ok(Action {
            op,
            target,
            value: Some(StateValue::Str(tag.to_string())),
            value_from: None,
        });
    }
    let decl = decls.iter().find(|d| d.name == target).ok_or_else(|| {
        bad(format!(
            "{what}.target refers to undeclared state {target:?}"
        ))
    })?;
    let value_from = match a.get("valueFrom") {
        Some(v) => {
            let name = v
                .as_str()
                .ok_or_else(|| bad(format!("{what}.valueFrom must be a string")))?
                .to_string();
            let src = decls.iter().find(|d| d.name == name).ok_or_else(|| {
                bad(format!(
                    "{what}.valueFrom refers to undeclared state {name:?}"
                ))
            })?;
            if src.typ != decl.typ {
                return Err(bad(format!(
                    "{what}.valueFrom type does not match {target:?}"
                )));
            }
            if matches!(op, Op::Add | Op::Sub) && decl.typ != ValueType::Int {
                return Err(bad(format!(
                    "{what}: {} needs an int target",
                    if op == Op::Sub { "sub" } else { "add" }
                )));
            }
            Some(name)
        }
        None => None,
    };
    let expected = if op == Op::Assign {
        Some(ValueType::List)
    } else {
        None
    };
    let value = match a.get("value") {
        Some(v) => Some(state_value(v, expected, &format!("{what}.value"))?),
        None => None,
    };
    match op {
        Op::Add | Op::Sub => {
            let label = if op == Op::Sub { "sub" } else { "add" };
            let numeric = match &value {
                Some(StateValue::Int(_)) => true,
                Some(_) => false,
                None => value_from.is_some(),
            };
            if decl.typ != ValueType::Int || !numeric {
                return Err(bad(format!(
                    "{what}: {label} needs an int target and an int value or valueFrom"
                )));
            }
        }
        Op::Toggle => {
            if decl.typ != ValueType::Bool {
                return Err(bad(format!("{what}: toggle needs a bool target")));
            }
        }
        Op::Append => {
            if decl.typ != ValueType::List {
                return Err(bad(format!("{what}: append needs a list target")));
            }
            match (&value, &value_from) {
                // Phase A lists are string arrays: append a string literal or
                // the current value of a string state.
                (Some(StateValue::Str(_)), None) => {}
                (None, Some(name)) => {
                    let src = decls
                        .iter()
                        .find(|d| d.name == *name)
                        .expect("checked above");
                    if src.typ != ValueType::Str {
                        return Err(bad(format!(
                            "{what}: append valueFrom must reference a string state"
                        )));
                    }
                }
                _ => {
                    return Err(bad(format!(
                        "{what}: append needs a string value or valueFrom"
                    )))
                }
            }
        }
        Op::RemoveAt => {
            if decl.typ != ValueType::List {
                return Err(bad(format!("{what}: removeAt needs a list target")));
            }
            match &value {
                Some(StateValue::Int(n)) if *n >= 0 => {}
                _ => {
                    return Err(bad(format!(
                        "{what}: removeAt needs a non-negative integer value"
                    )))
                }
            }
        }
        Op::Set => {
            // A list is replaced whole by `assign` (core e0dc310), so `set`
            // never reaches a list target in any spelling.
            if decl.typ == ValueType::List {
                return Err(bad(format!(
                    "{what}: set cannot replace a list target; use assign"
                )));
            }
            if let Some(ref v) = value {
                if v.typ() != decl.typ {
                    return Err(bad(format!("{what}: set value type mismatch")));
                }
            }
        }
        Op::Assign => {
            if decl.typ != ValueType::List {
                return Err(bad(format!("{what}: assign needs a list target")));
            }
            match (&value, &value_from) {
                // Exactly one spelling: a whole string array, or another list
                // state's current value; the generic valueFrom parse above has
                // already matched its type to this list.
                (Some(StateValue::List(_)), None) => {}
                // Unreachable through the typed parse above; kept so a future
                // change cannot silently accept a non-list value.
                (Some(_), None) => {
                    return Err(bad(format!("{what}: assign value must be a string array")))
                }
                (None, Some(_)) => {}
                (None, None) => return Err(bad(format!("{what}: assign needs a value"))),
                (Some(_), Some(_)) => {
                    return Err(bad(format!(
                        "{what}: assign takes value and valueFrom exclusively"
                    )))
                }
            }
        }
        // Navigation actions returned before reaching here.
        Op::Push | Op::Pop => unreachable!("navigation actions return early"),
    }
    Ok(Action {
        op,
        target,
        value,
        value_from,
    })
}

fn events(o: &Map<String, Value>, decls: &[StateDecl]) -> Result<Vec<Event>, IrError> {
    let mut keys: Vec<&String> = o.keys().collect();
    keys.sort(); // deterministic ordering regardless of JSON object order
                 // the IR format declares a closed event-name set; reject a typo here rather
                 // than storing a binding that interaction dispatch would silently ignore.
    const EVENT_NAMES: &[&str] = &["tap", "input", "doubleTap", "longPress", "drag"];
    if let Some(unknown) = keys.iter().find(|k| !EVENT_NAMES.contains(&k.as_str())) {
        return Err(bad(format!(
            "events.{unknown}: unknown event name (expected {})",
            EVENT_NAMES.join("|")
        )));
    }
    let mut out = Vec::with_capacity(keys.len());
    for name in keys {
        let what = format!("events.{name}");
        let raw = object(&o[name], &what)?;
        // One action object, or {"actions": [...]}.
        let actions = match raw.get("actions") {
            Some(list) => {
                let items = list
                    .as_array()
                    .ok_or_else(|| bad(format!("{what}.actions must be an array")))?;
                let mut acts = Vec::with_capacity(items.len());
                for (i, item) in items.iter().enumerate() {
                    let a = object(item, &format!("{what}.actions[{i}]"))?;
                    acts.push(action(a, &format!("{what}.actions[{i}]"), decls)?);
                }
                acts
            }
            None => vec![action(raw, &what, decls)?],
        };
        out.push(Event {
            name: name.clone(),
            actions,
        });
    }
    Ok(out)
}

/// Both conditional fields are validated here so layout can trust the state type.
fn bool_condition(
    o: &Map<String, Value>,
    key: &str,
    decls: &[StateDecl],
) -> Result<Option<String>, IrError> {
    let Some(v) = o.get(key) else {
        return Ok(None);
    };
    let name = v
        .as_str()
        .ok_or_else(|| bad(format!("tree node.{key} must be a string")))?
        .to_string();
    let decl = decls.iter().find(|d| d.name == name).ok_or_else(|| {
        bad(format!(
            "tree node.{key} refers to undeclared state {name:?}"
        ))
    })?;
    if decl.typ != ValueType::Bool {
        return Err(bad(format!(
            "tree node.{key} must reference a bool state, but {name:?} is not"
        )));
    }
    Ok(Some(name))
}

fn node(v: &Value, next_id: &mut u32, decls: &[StateDecl]) -> Result<Node, IrError> {
    let o = object(v, "tree node")?;
    let id = integer(required(o, "id", "tree node")?, "tree node.id")?;
    if id < 0 || id > u32::MAX as i64 {
        return Err(bad(format!("tree node.id {id} is out of u32 range")));
    }
    let id = id as u32;
    if id != *next_id {
        return Err(bad(format!(
            "tree node.id {id} breaks pre-order (expected {next_id})"
        )));
    }
    *next_id = (*next_id)
        .checked_add(1)
        .ok_or_else(|| bad("tree node id overflow"))?;
    let kind_s = string_field(o, "kind", "tree node")?;
    let kind = Kind::parse(&kind_s);
    let p = props(object(required(o, "props", "tree node")?, "props")?)?;
    let children_v = required(o, "children", "tree node")?
        .as_array()
        .ok_or_else(|| bad("tree node.children must be an array"))?;
    let mut children = Vec::with_capacity(children_v.len());
    for c in children_v {
        children.push(node(c, next_id, decls)?);
    }
    let e = events(
        object(required(o, "events", "tree node")?, "events")?,
        decls,
    )?;
    let when = bool_condition(o, "when", decls)?;
    let unless = bool_condition(o, "unless", decls)?;
    Ok(Node {
        id,
        kind,
        props: p,
        children,
        events: e,
        when,
        unless,
    })
}

fn tokens(v: &Value) -> Result<ThemeTokens, IrError> {
    let Some(o) = v.as_object() else {
        return Err(bad("theme.tokens must be a JSON object"));
    };
    let contrast = match opt_string(o, "contrast", "theme.tokens")?.as_deref() {
        None | Some("normal") => Contrast::Normal,
        Some("high") => Contrast::High,
        Some(other) => {
            return Err(bad(format!(
                "theme.tokens.contrast has unknown value {other:?}"
            )));
        }
    };
    Ok(ThemeTokens {
        surface: color_prop(o, "surface", "theme.tokens")?,
        on_surface: color_prop(o, "onSurface", "theme.tokens")?,
        stage: color_prop(o, "stage", "theme.tokens")?,
        success: color_prop(o, "success", "theme.tokens")?,
        warning: color_prop(o, "warning", "theme.tokens")?,
        error: color_prop(o, "error", "theme.tokens")?,
        info: color_prop(o, "info", "theme.tokens")?,
        primary: color_prop(o, "primary", "theme.tokens")?,
        secondary: color_prop(o, "secondary", "theme.tokens")?,
        font: opt_string(o, "font", "theme.tokens")?,
        contrast,
    })
}

fn theme(v: &Value) -> Result<Theme, IrError> {
    let o = object(v, "theme")?;
    let mode = match string_field(o, "mode", "theme")?.as_str() {
        "light" => ThemeMode::Light,
        "dark" => ThemeMode::Dark,
        "system" => ThemeMode::System,
        other => return Err(bad(format!("theme.mode has unknown value {other:?}"))),
    };
    let accent_s = string_field(o, "accent", "theme")?;
    let accent = Rgba::from_hex(&accent_s)
        .ok_or_else(|| bad(format!("theme.accent must be #RRGGBB, got {accent_s:?}")))?;
    let radius = integer(required(o, "radius", "theme")?, "theme.radius")?;
    if radius < 0 {
        return Err(bad("theme.radius must be non-negative"));
    }
    let tk = match o.get("tokens") {
        Some(t) => tokens(t)?,
        None => ThemeTokens::default(),
    };
    let font_scale = font_scale_prop(o, "fontScale", "theme")?;
    let font_metrics = match opt_string(o, "fontMetrics", "theme")? {
        Some(s) => FontMetrics::parse(&s).ok_or_else(|| {
            bad(format!(
                "theme.fontMetrics has unknown value {s:?}: expected fixed or proportional"
            ))
        })?,
        None => FontMetrics::Fixed,
    };
    Ok(Theme {
        mode,
        accent,
        radius,
        tokens: tk,
        font_scale,
        font_metrics,
    })
}

/// `highContrast` on a node applies to its whole subtree (additive prop).
fn inherit_high_contrast(n: &mut Node, parent: bool) {
    let high = parent || n.props.high_contrast;
    n.props.high_contrast = high;
    for c in &mut n.children {
        inherit_high_contrast(c, high);
    }
}

/// Parse a JSON object of state overrides against one IR document.
///
/// Names must be declared in `ir.state` and values must match the declared
/// type; undeclared names and type mismatches are errors, not silent drops.
/// Omitted names keep the IR initial value.
pub fn parse_state_json(ir: &Ir, text: &str) -> Result<StateView, IrError> {
    let v: Value =
        serde_json::from_str(text).map_err(|e| bad(format!("invalid state JSON: {e}")))?;
    let o = object(&v, "state")?;
    let mut view = StateView::from_ir(ir);
    for (name, value) in o {
        // A host may carry overrides for states the IR no longer declares
        // (host and UI evolve on different cadences). Fault isolation keeps the
        // frame rendering from the IR defaults instead of failing the call; a
        // name the IR does declare with a mismatched value type still errors.
        let Some(decl) = ir.state.iter().find(|d| &d.name == name) else {
            continue;
        };
        let parsed = state_value(value, Some(decl.typ), &format!("state.{name}"))?;
        view.set(name.clone(), parsed);
    }
    Ok(view)
}

/// Parse and fully validate one IR 0.1.0 document.
pub fn parse_ir(text: &str) -> Result<Ir, IrError> {
    let v: Value = serde_json::from_str(text).map_err(|e| bad(format!("invalid IR JSON: {e}")))?;
    let o = object(&v, "IR root")?;
    let version = string_field(o, "ir", "IR root")?;
    if version != "0.1.0" {
        return Err(bad(format!(
            "unsupported IR version {version:?}: expected \"0.1.0\""
        )));
    }
    let app = string_field(o, "app", "IR root")?;
    let state = state_decls(required(o, "state", "IR root")?)?;
    let mut next_id = 0u32;
    // Multi-page IR is additive: a `routes` object replaces the top-level
    // tree and `tree` mirrors the initial route, so single-tree consumers
    // keep working; without it the document is exactly the 0.1.0 single page.
    let (tree, routes, initial_route) = if o.contains_key("routes") {
        if o.get("tree").is_some() {
            return Err(bad(
                "IR root must not declare both \"routes\" and a top-level \"tree\"",
            ));
        }
        let routes = routes_map(required(o, "routes", "IR root")?, &state)?;
        let initial = match opt_string(o, "initialRoute", "IR root")? {
            Some(name) if routes.contains_key(&name) => name,
            Some(name) => {
                let declared = routes.keys().cloned().collect::<Vec<_>>().join(", ");
                return Err(bad(format!(
                    "IR root.initialRoute {name:?} is not a declared route (declared: {declared})"
                )));
            }
            // No explicit entry route: the first declared route wins.
            None => routes.keys().next().cloned().expect("non-empty"),
        };
        for route in routes.values() {
            check_node_pushes(&route.tree, &routes)?;
        }
        let tree = routes[initial.as_str()].tree.clone();
        (tree, routes, Some(initial))
    } else {
        let mut tree = node(required(o, "tree", "IR root")?, &mut next_id, &state)?;
        inherit_high_contrast(&mut tree, false);
        (tree, BTreeMap::new(), None)
    };
    let theme = theme(required(o, "theme", "IR root")?)?;
    // Optional page-transition key; unparsable values are rejected by name.
    let transition = match o.get("transition") {
        Some(v) => {
            let s = string_field(o, "transition", "IR root")?;
            Some(
                TransitionKind::parse(&s).ok_or_else(|| {
                    bad(format!(
                        "IR root.transition {v:?} is invalid: expected \"slide\", \"fade\" or \"scale\""
                    ))
                })?,
            )
        }
        None => None,
    };
    let icons = parse_icons(o.get("icons"))?;
    let render_quality = parse_render_quality(o.get("renderQuality"))?;
    // Optional root direction; an unknown value is a readable typed error so a
    // misspelt direction never silently renders left-to-right.
    let directionality = match o.get("directionality") {
        Some(v) => Directionality::parse(&string_field(o, "directionality", "IR root")?)
            .ok_or_else(|| {
                bad(format!(
                    "IR root.directionality {v:?} is invalid: expected \"ltr\" or \"rtl\""
                ))
            })?,
        None => Directionality::Ltr,
    };
    Ok(Ir {
        version,
        app,
        state,
        tree,
        theme,
        transition,
        routes,
        initial_route,
        icons,
        render_quality,
        directionality,
    })
}

/// Parse the optional root `renderQuality` object. A declared subfield is
/// checked against the single value this renderer implements; an unknown
/// subfield is ignored for forward compatibility. Absence and JSON null are
/// both the default path.
fn parse_render_quality(v: Option<&Value>) -> Result<Option<RenderQuality>, IrError> {
    let Some(v) = v else {
        return Ok(None);
    };
    if v.is_null() {
        return Ok(None);
    }
    let what = "IR root.renderQuality";
    let o = object(v, what)?;
    let msaa = match o.get("msaa") {
        Some(value) => {
            let n = integer(value, &format!("{what}.msaa"))?;
            if n != 4 {
                return Err(bad(format!("{what}.msaa {n} is unsupported: expected 4")));
            }
            Some(n)
        }
        None => None,
    };
    let gradient_bits = match o.get("gradientBits") {
        Some(value) => {
            let n = integer(value, &format!("{what}.gradientBits"))?;
            if n != 16 {
                return Err(bad(format!(
                    "{what}.gradientBits {n} is unsupported: expected 16"
                )));
            }
            Some(n)
        }
        None => None,
    };
    let curve = match opt_string(o, "curve", what)? {
        Some(s) if s == "bezier" => Some(s),
        Some(s) => {
            return Err(bad(format!(
                "{what}.curve {s:?} is unsupported: expected \"bezier\""
            )))
        }
        None => None,
    };
    let stroke = match opt_string(o, "stroke", what)? {
        Some(s) if s == "physical" => Some(s),
        Some(s) => {
            return Err(bad(format!(
                "{what}.stroke {s:?} is unsupported: expected \"physical\""
            )))
        }
        None => None,
    };
    let text_baseline = match opt_string(o, "textBaseline", what)? {
        Some(s) if s == "grid" => Some(s),
        Some(s) => {
            return Err(bad(format!(
                "{what}.textBaseline {s:?} is unsupported: expected \"grid\""
            )))
        }
        None => None,
    };
    Ok(Some(RenderQuality {
        msaa,
        gradient_bits,
        curve,
        stroke,
        text_baseline,
    }))
}

/// Parse the optional top-level `icons` table. It may be a JSON array of
/// `{name, paths: [d...], viewBox}` entries (the compiler's shape) or an object
/// keyed by icon name with the same record. A malformed entry is skipped: an
/// unusable package glyph degrades to the built-in set, never a hard error.
fn parse_icons(v: Option<&Value>) -> Result<BTreeMap<String, IconPaths>, IrError> {
    let mut table = BTreeMap::new();
    let entries: Vec<&Value> = match v {
        None | Some(Value::Null) => return Ok(table),
        Some(Value::Array(items)) => items.iter().collect(),
        Some(Value::Object(map)) => map.values().collect(),
        Some(_) => return Err(bad("IR root.icons must be an array or object")),
    };
    for entry in entries {
        let Ok(rec) = object(entry, "icon entry") else {
            continue;
        };
        let Some(name) = rec.get("name").and_then(Value::as_str) else {
            continue;
        };
        let view_box = rec
            .get("viewBox")
            .and_then(Value::as_str)
            .unwrap_or("0 0 24 24");
        let Some(paths) = rec.get("paths").and_then(Value::as_array) else {
            continue;
        };
        let ds: Vec<String> = paths
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        if let Some(icon) = IconPaths::from_paths(name, view_box, &ds) {
            table.insert(name.to_string(), icon);
        }
    }
    Ok(table)
}

/// Parse one route tree. The id counter seeds from the tree's root id, so a
/// routes document may number each route independently or continue one global
/// pre-order across routes; only in-tree contiguity is enforced, because only
/// one route's tree is ever laid out at a time.
fn route_tree(v: &Value, decls: &[StateDecl]) -> Result<Node, IrError> {
    let o = object(v, "tree node")?;
    let root = integer(required(o, "id", "tree node")?, "tree node.id")?;
    if root < 0 || root > u32::MAX as i64 {
        return Err(bad(format!("tree node.id {root} is out of u32 range")));
    }
    let mut next_id = root as u32;
    node(v, &mut next_id, decls)
}

/// Parse the optional top-level `routes` object: unique non-empty route names
/// mapping to `{ "tree": <node>, "title": <optional string> }`. JSON object
/// keys are unique and `Map` iterates sorted, so the parsed map is
/// deterministic regardless of authored order.
fn routes_map(v: &Value, decls: &[StateDecl]) -> Result<BTreeMap<String, Route>, IrError> {
    let o = object(v, "routes")?;
    if o.is_empty() {
        return Err(bad("\"routes\" must declare at least one route"));
    }
    let mut out = BTreeMap::new();
    for (name, rv) in o {
        if name.is_empty() {
            return Err(bad("\"routes\" keys must be non-empty route names"));
        }
        let what = format!("routes.{name}");
        let ro = object(rv, &what)?;
        let mut tree = route_tree(required(ro, "tree", &what)?, decls)?;
        inherit_high_contrast(&mut tree, false);
        let title = opt_string(ro, "title", &what)?;
        out.insert(name.clone(), Route { tree, title });
    }
    Ok(out)
}

/// Every `push` in a routes document must target a declared route.
fn check_node_pushes(n: &Node, routes: &BTreeMap<String, Route>) -> Result<(), IrError> {
    for event in &n.events {
        for action in &event.actions {
            if action.op == Op::Push && !routes.contains_key(&action.target) {
                let declared = routes.keys().cloned().collect::<Vec<_>>().join(", ");
                return Err(bad(format!(
                    "events.{}: push target {:?} is not a declared route (declared: {declared})",
                    event.name, action.target
                )));
            }
        }
    }
    for child in &n.children {
        check_node_pushes(child, routes)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_IR: &str = r##"{
        "ir": "0.1.0", "app": "Sample",
        "state": [
            {"name": "count", "type": "int", "init": 0},
            {"name": "title", "type": "string", "init": "idle"},
            {"name": "flag", "type": "bool", "init": false}
        ],
        "tree": {"id": 0, "kind": "Text", "props": {"value": "{count}"},
                 "children": [], "events": {}},
        "theme": {"mode": "light", "accent": "#097FEB", "radius": 8}
    }"##;

    #[test]
    fn interpolation_handles_escapes_and_unknown_names() {
        let mut s = StateView::default();
        s.set("count", StateValue::Int(7));
        s.set("name", StateValue::Str("world".into()));
        assert_eq!(s.interpolate("Count: {count} / {name}"), "Count: 7 / world");
        assert_eq!(s.interpolate("{{count}} {missing}"), "{count} {missing}");
    }

    #[test]
    fn state_json_overrides_keep_undeclared_defaults() {
        let ir = parse_ir(SAMPLE_IR).unwrap();
        let view = parse_state_json(&ir, r#"{"count":7,"title":"done","flag":true}"#).unwrap();
        assert_eq!(view.get("count"), Some(&StateValue::Int(7)));
        assert_eq!(view.get("title"), Some(&StateValue::Str("done".into())));
        assert_eq!(view.get("flag"), Some(&StateValue::Bool(true)));
        let empty = parse_state_json(&ir, "{}").unwrap();
        assert_eq!(empty, StateView::from_ir(&ir));
    }

    #[test]
    fn state_json_ignores_names_the_ir_no_longer_declares() {
        // A host override for a retired state name keeps the frame rendering
        // from the IR defaults instead of failing the call (fault isolation).
        let ir = parse_ir(SAMPLE_IR).unwrap();
        let view = parse_state_json(&ir, r#"{"missing":1,"count":7}"#).unwrap();
        assert_eq!(view.get("count"), Some(&StateValue::Int(7)));
        assert_eq!(view.get("missing"), None);
    }

    #[test]
    fn state_json_rejects_malformed_and_mismatched_values() {
        let ir = parse_ir(SAMPLE_IR).unwrap();
        for bad_json in [
            "",
            "{",
            "[]",
            r#"{"count":"seven"}"#,
            r#"{"flag":1}"#,
            r#"{"count":1.5}"#,
        ] {
            assert!(parse_state_json(&ir, bad_json).is_err(), "{bad_json:?}");
        }
    }

    #[test]
    fn opacity_uses_exact_fixed_point() {
        let mut o = Map::new();
        o.insert("opacity".into(), Value::from(0.95));
        assert_eq!(unit_prop(&o, "opacity", "props").unwrap(), Some(973));
        let mut o = Map::new();
        o.insert("opacity".into(), Value::from(1));
        assert_eq!(unit_prop(&o, "opacity", "props").unwrap(), Some(1024));
        let mut o = Map::new();
        o.insert("opacity".into(), Value::from(0));
        assert_eq!(unit_prop(&o, "opacity", "props").unwrap(), Some(0));
        let mut o = Map::new();
        o.insert("opacity".into(), Value::from(2));
        assert!(unit_prop(&o, "opacity", "props").is_err());
    }

    #[test]
    fn gesture_prop_parses_known_values_and_rejects_unknown() {
        for (value, expected) in [
            ("tap", GestureKind::Tap),
            ("doubleTap", GestureKind::DoubleTap),
            ("longPress", GestureKind::LongPress),
            ("drag", GestureKind::Drag),
        ] {
            let mut o = Map::new();
            o.insert("gesture".into(), Value::from(value));
            assert_eq!(props(&o).unwrap().gesture, Some(expected), "{value}");
        }
        let mut o = Map::new();
        o.insert("gesture".into(), Value::from("swipe"));
        assert!(props(&o).is_err());
        let mut o = Map::new();
        o.insert("gesture".into(), Value::from(true));
        assert!(props(&o).is_err());
    }
}
