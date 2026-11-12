//! IR v0.1 parsing and validation into typed node/state/theme structs.
//!
//! JSON is parsed into `serde_json::Value` and then checked field by field, so a
//! malformed document always produces a readable `IrError` instead of a panic.
//!
//! R2 is additive: `theme.tokens`, the optional node field `when`, the canonical
//! style props and sixteen new node kinds are accepted. Unknown props are
//! ignored and an unknown kind falls back to Container/Text, so a file from a
//! newer compiler still renders instead of failing.

use crate::anim::Curve;
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueType {
    Int,
    Bool,
    Str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StateValue {
    Int(i64),
    Bool(bool),
    Str(String),
}

impl StateValue {
    pub fn typ(&self) -> ValueType {
        match self {
            StateValue::Int(_) => ValueType::Int,
            StateValue::Bool(_) => ValueType::Bool,
            StateValue::Str(_) => ValueType::Str,
        }
    }

    /// Text form used by `{name}` interpolation.
    pub fn as_text(&self) -> String {
        match self {
            StateValue::Int(n) => n.to_string(),
            StateValue::Bool(b) => b.to_string(),
            StateValue::Str(s) => s.clone(),
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

/// The 24 IR node kinds. `Other` is a kind this renderer does not know: it is
/// rendered with the documented fallback (Container with children, else Text).
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
            Kind::Other => "Other",
        }
    }

    /// Unknown kinds never fail the parse: they degrade to `Other`.
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
            _ => Kind::Other,
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
        )
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
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub radius: Option<i64>,
    pub color: Option<Rgba>,
    pub value: Option<String>,
    /// Integer form of `value` (R2 `Progress` percentage).
    pub value_num: Option<i64>,
    /// Fractional form of `value` as fixed point (R2 `Progress` fraction).
    pub value_frac: Option<i64>,
    pub label: Option<String>,
    pub title: Option<String>,
    pub name: Option<String>,
    pub placeholder: Option<String>,
    pub bind: Option<String>,
    pub src: Option<String>,
    pub size: Option<i64>,
    // R2 canonical effects.
    pub shadow: Option<i64>,
    pub gradient: Option<(Rgba, Rgba)>,
    pub glass: bool,
    /// Alpha in fixed point: 0 = transparent, 1024 = opaque.
    pub opacity: Option<i64>,
    pub border: Option<Rgba>,
    pub animate: Curve,
    // R2 kind props.
    pub columns: Option<i64>,
    pub labels: Vec<String>,
    pub min: Option<i64>,
    pub max: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Set,
    Add,
    Toggle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    pub op: Op,
    pub target: String,
    pub value: Option<StateValue>,
    /// R2: read the value from another state at execution time.
    pub value_from: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub name: String,
    /// R2: handlers may hold several actions; v0.1 handlers hold exactly one.
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
    /// R2: the node is visible only while this bool state is true.
    pub when: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
    System,
}

/// R2 theme tokens; missing tokens fall back to `mode`/`accent`/`radius`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThemeTokens {
    pub surface: Option<Rgba>,
    pub on_surface: Option<Rgba>,
    pub font: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Theme {
    pub mode: ThemeMode,
    pub accent: Rgba,
    pub radius: i64,
    pub tokens: ThemeTokens,
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

fn state_value(v: &Value, expected: Option<ValueType>, what: &str) -> Result<StateValue, IrError> {
    let value = if let Some(s) = v.as_str() {
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
    let align = match opt_string(o, "align", what)?.as_deref() {
        None | Some("start") | Some("left") | Some("top") => Align::Start,
        Some("center") | Some("middle") => Align::Center,
        Some("end") | Some("right") | Some("bottom") => Align::End,
        Some(other) => return Err(bad(format!("{what}.align has unknown value {other:?}"))),
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
    Ok(Props {
        gap: opt_integer(o, "gap", what)?.unwrap_or(0),
        padding: opt_integer(o, "padding", what)?.unwrap_or(0),
        margin: opt_integer(o, "margin", what)?.unwrap_or(0),
        align,
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
        src: opt_string(o, "src", what)?,
        size: opt_integer(o, "size", what)?,
        shadow,
        gradient: gradient_prop(o, "gradient", what)?,
        glass: opt_bool(o, "glass", what)?,
        opacity: unit_prop(o, "opacity", what)?,
        border: color_prop(o, "border", what)?,
        animate,
        columns: opt_integer(o, "columns", what)?,
        labels: labels_prop(o, "labels", what)?,
        min: opt_signed_integer(o, "min", what)?,
        max: opt_signed_integer(o, "max", what)?,
    })
}

fn action(a: &Map<String, Value>, what: &str, decls: &[StateDecl]) -> Result<Action, IrError> {
    let op = match string_field(a, "op", what)?.as_str() {
        "set" => Op::Set,
        "add" => Op::Add,
        "toggle" => Op::Toggle,
        other => return Err(bad(format!("{what}.op has unknown value {other:?}"))),
    };
    let target = string_field(a, "target", what)?;
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
            if op == Op::Add && decl.typ != ValueType::Int {
                return Err(bad(format!("{what}: add needs an int target")));
            }
            Some(name)
        }
        None => None,
    };
    let value = match a.get("value") {
        Some(v) => Some(state_value(v, None, &format!("{what}.value"))?),
        None => None,
    };
    match op {
        Op::Add => {
            if decl.typ != ValueType::Int || !matches!(value, Some(StateValue::Int(_))) {
                return Err(bad(format!(
                    "{what}: add needs an int target and numeric value"
                )));
            }
        }
        Op::Toggle => {
            if decl.typ != ValueType::Bool {
                return Err(bad(format!("{what}: toggle needs a bool target")));
            }
        }
        Op::Set => {
            if let Some(ref v) = value {
                if v.typ() != decl.typ {
                    return Err(bad(format!("{what}: set value type mismatch")));
                }
            }
        }
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
    let mut out = Vec::with_capacity(keys.len());
    for name in keys {
        let what = format!("events.{name}");
        let raw = object(&o[name], &what)?;
        // R2: one action object, or {"actions": [...]}.
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
    let when = match o.get("when") {
        Some(v) => {
            let name = v
                .as_str()
                .ok_or_else(|| bad("tree node.when must be a string"))?
                .to_string();
            let decl = decls.iter().find(|d| d.name == name).ok_or_else(|| {
                bad(format!(
                    "tree node.when refers to undeclared state {name:?}"
                ))
            })?;
            if decl.typ != ValueType::Bool {
                return Err(bad(format!(
                    "tree node.when must reference a bool state, but {name:?} is not"
                )));
            }
            Some(name)
        }
        None => None,
    };
    Ok(Node {
        id,
        kind,
        props: p,
        children,
        events: e,
        when,
    })
}

fn tokens(v: &Value) -> Result<ThemeTokens, IrError> {
    let Some(o) = v.as_object() else {
        return Err(bad("theme.tokens must be a JSON object"));
    };
    Ok(ThemeTokens {
        surface: color_prop(o, "surface", "theme.tokens")?,
        on_surface: color_prop(o, "onSurface", "theme.tokens")?,
        font: opt_string(o, "font", "theme.tokens")?,
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
    Ok(Theme {
        mode,
        accent,
        radius,
        tokens: tk,
    })
}

/// Parse and fully validate one IR v0.1 document.
pub fn parse_ir(text: &str) -> Result<Ir, IrError> {
    let v: Value = serde_json::from_str(text).map_err(|e| bad(format!("invalid IR JSON: {e}")))?;
    let o = object(&v, "IR root")?;
    let version = string_field(o, "ir", "IR root")?;
    if version != "0.1" {
        return Err(bad(format!(
            "unsupported IR version {version:?}: expected \"0.1\""
        )));
    }
    let app = string_field(o, "app", "IR root")?;
    let state = state_decls(required(o, "state", "IR root")?)?;
    let mut next_id = 0u32;
    let tree = node(required(o, "tree", "IR root")?, &mut next_id, &state)?;
    let theme = theme(required(o, "theme", "IR root")?)?;
    Ok(Ir {
        version,
        app,
        state,
        tree,
        theme,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolation_handles_escapes_and_unknown_names() {
        let mut s = StateView::default();
        s.set("count", StateValue::Int(7));
        s.set("name", StateValue::Str("world".into()));
        assert_eq!(s.interpolate("Count: {count} / {name}"), "Count: 7 / world");
        assert_eq!(s.interpolate("{{count}} {missing}"), "{count} {missing}");
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
}
