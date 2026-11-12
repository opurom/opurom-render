//! IR v0.1 parsing and validation into typed node/state/theme structs.
//!
//! JSON is parsed into `serde_json::Value` and then checked field by field, so a
//! malformed document always produces a readable `IrError` instead of a panic.

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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateDecl {
    pub name: String,
    pub typ: ValueType,
    pub init: StateValue,
}

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
        }
    }

    fn parse(s: &str) -> Option<Kind> {
        Some(match s {
            "Column" => Kind::Column,
            "Row" => Kind::Row,
            "Container" => Kind::Container,
            "Text" => Kind::Text,
            "Button" => Kind::Button,
            "Input" => Kind::Input,
            "Image" => Kind::Image,
            "Spacer" => Kind::Spacer,
            _ => return None,
        })
    }

    pub fn is_leaf(self) -> bool {
        matches!(
            self,
            Kind::Text | Kind::Button | Kind::Input | Kind::Image | Kind::Spacer
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
    pub align: Align,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub radius: Option<i64>,
    pub color: Option<Rgba>,
    pub value: Option<String>,
    pub label: Option<String>,
    pub placeholder: Option<String>,
    pub bind: Option<String>,
    pub src: Option<String>,
    pub size: Option<i64>,
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub name: String,
    pub action: Action,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub id: u32,
    pub kind: Kind,
    pub props: Props,
    pub children: Vec<Node>,
    pub events: Vec<Event>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
    System,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Theme {
    pub mode: ThemeMode,
    pub accent: Rgba,
    pub radius: i64,
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

fn opt_string(o: &Map<String, Value>, key: &str, what: &str) -> Result<Option<String>, IrError> {
    match o.get(key) {
        Some(v) => v
            .as_str()
            .map(|s| Some(s.to_string()))
            .ok_or_else(|| bad(format!("{what}.{key} must be a string"))),
        None => Ok(None),
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
    let color = match opt_string(o, "color", what)? {
        Some(s) => Some(
            Rgba::from_hex(&s)
                .ok_or_else(|| bad(format!("{what}.color must be #RRGGBB, got {s:?}")))?,
        ),
        None => None,
    };
    Ok(Props {
        gap: opt_integer(o, "gap", what)?.unwrap_or(0),
        padding: opt_integer(o, "padding", what)?.unwrap_or(0),
        align,
        width: opt_integer(o, "width", what)?,
        height: opt_integer(o, "height", what)?,
        radius: opt_integer(o, "radius", what)?,
        color,
        value: opt_string(o, "value", what)?,
        label: opt_string(o, "label", what)?,
        placeholder: opt_string(o, "placeholder", what)?,
        bind: opt_string(o, "bind", what)?,
        src: opt_string(o, "src", what)?,
        size: opt_integer(o, "size", what)?,
    })
}

fn events(o: &Map<String, Value>, decls: &[StateDecl]) -> Result<Vec<Event>, IrError> {
    let mut keys: Vec<&String> = o.keys().collect();
    keys.sort(); // deterministic ordering regardless of JSON object order
    let mut out = Vec::with_capacity(keys.len());
    for name in keys {
        let what = format!("events.{name}");
        let a = object(&o[name], &what)?;
        let op = match string_field(a, "op", &what)?.as_str() {
            "set" => Op::Set,
            "add" => Op::Add,
            "toggle" => Op::Toggle,
            other => return Err(bad(format!("{what}.op has unknown value {other:?}"))),
        };
        let target = string_field(a, "target", &what)?;
        let decl = decls.iter().find(|d| d.name == target).ok_or_else(|| {
            bad(format!(
                "{what}.target refers to undeclared state {target:?}"
            ))
        })?;
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
        out.push(Event {
            name: name.clone(),
            action: Action { op, target, value },
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
    let kind = Kind::parse(&kind_s).ok_or_else(|| bad(format!("unknown node kind {kind_s:?}")))?;
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
    Ok(Node {
        id,
        kind,
        props: p,
        children,
        events: e,
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
    Ok(Theme {
        mode,
        accent,
        radius,
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
}
