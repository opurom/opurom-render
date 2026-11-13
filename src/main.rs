//! `opurom-render <ir.json> <width> <height> <out.ppm> [--state name=value ...] [--tick 0..4000] [--transition slide|fade|scale] [--a11y out.json] [--svg out.svg] [--commands out.json]`
//!
//! Writes a P6 PPM snapshot (RGB; the opaque alpha channel is dropped) and
//! prints `digest=<16 hex>` (FNV-1a 64 over RGBA bytes) plus `dirty=<count>`.
//! `--a11y` additionally writes the deterministic accessibility tree,
//! `--svg` a deterministic SVG 1.1 vector snapshot, and `--commands` the exact
//! raster display list plus optional effect commands. `--transition` composes the frame at tick 0 with the
//! settled frame; the stdout shape is unchanged.

use opurom_render::{
    a11y_json_at, digest_hex, parse_ir, svg_at, Frame, IrError, Renderer, StateValue, StateView,
    Transition, TransitionKind, ValueType, SETTLED_MS,
};
use std::{env, fs, process};

fn main() {
    if let Err(e) = run() {
        eprintln!("opurom-render: error: {e}");
        process::exit(1);
    }
}

fn parse_value(typ: ValueType, raw: &str) -> Result<StateValue, String> {
    match typ {
        ValueType::Int => raw
            .parse::<i64>()
            .map(StateValue::Int)
            .map_err(|_| format!("state value {raw:?} is not an int")),
        ValueType::Bool => match raw {
            "true" | "1" => Ok(StateValue::Bool(true)),
            "false" | "0" => Ok(StateValue::Bool(false)),
            _ => Err(format!("state value {raw:?} is not a bool")),
        },
        ValueType::Str => Ok(StateValue::Str(raw.to_string())),
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() < 4 {
        return Err(
            "usage: opurom-render <ir.json> <width> <height> <out.ppm> [--state name=value ...] [--tick 0..4000] [--transition slide|fade|scale] [--a11y out.json] [--svg out.svg] [--commands out.json]"
                .to_string(),
        );
    }
    let width: u32 = args[1]
        .parse()
        .map_err(|_| format!("invalid width {:?}: expected a positive integer", args[1]))?;
    let height: u32 = args[2]
        .parse()
        .map_err(|_| format!("invalid height {:?}: expected a positive integer", args[2]))?;
    let text = fs::read_to_string(&args[0])
        .map_err(|e| format!("cannot read IR file {:?}: {e}", args[0]))?;
    let ir = parse_ir(&text).map_err(|e: IrError| e.to_string())?;
    let mut state = StateView::from_ir(&ir);
    // Animation clock position in milliseconds; also drives transitions.
    let mut elapsed_ms: i64 = SETTLED_MS;
    let mut transition_kind: Option<TransitionKind> = None;
    let mut a11y_path: Option<String> = None;
    let mut svg_path: Option<String> = None;
    let mut commands_path: Option<String> = None;

    let mut i = 4usize;
    while i < args.len() {
        match args[i].as_str() {
            "--state" => {
                let spec = args
                    .get(i + 1)
                    .ok_or_else(|| "--state requires a name=value argument".to_string())?;
                let (name, raw) = spec
                    .split_once('=')
                    .ok_or_else(|| format!("--state expects name=value, got {spec:?}"))?;
                let decl = ir
                    .state
                    .iter()
                    .find(|d| d.name == name)
                    .ok_or_else(|| format!("unknown state name {name:?}"))?;
                state.set(name.to_string(), parse_value(decl.typ, raw)?);
                i += 2;
            }
            "--tick" => {
                let spec = args
                    .get(i + 1)
                    .ok_or_else(|| "--tick requires a millisecond value".to_string())?;
                let value: i64 = spec.parse().map_err(|_| {
                    format!("invalid tick {spec:?}: expected 0..={SETTLED_MS} milliseconds")
                })?;
                if !(0..=SETTLED_MS).contains(&value) {
                    return Err(format!(
                        "invalid tick {spec:?}: expected 0..={SETTLED_MS} milliseconds"
                    ));
                }
                elapsed_ms = value;
                i += 2;
            }
            "--transition" => {
                let spec = args
                    .get(i + 1)
                    .ok_or_else(|| "--transition requires slide, fade or scale".to_string())?;
                transition_kind = Some(TransitionKind::parse(spec).ok_or_else(|| {
                    format!("unknown transition {spec:?}: expected slide, fade or scale")
                })?);
                i += 2;
            }
            "--a11y" => {
                let spec = args
                    .get(i + 1)
                    .ok_or_else(|| "--a11y requires an output JSON path".to_string())?;
                a11y_path = Some(spec.clone());
                i += 2;
            }
            "--svg" => {
                let spec = args
                    .get(i + 1)
                    .ok_or_else(|| "--svg requires an output SVG path".to_string())?;
                svg_path = Some(spec.clone());
                i += 2;
            }
            "--commands" => {
                let spec = args
                    .get(i + 1)
                    .ok_or_else(|| "--commands requires an output JSON path".to_string())?;
                commands_path = Some(spec.clone());
                i += 2;
            }
            other => {
                return Err(format!(
                    "unexpected argument {other:?}: expected --state name=value, --tick 0..={SETTLED_MS}, --transition slide|fade|scale, --a11y out.json, --svg out.svg or --commands out.json"
                ));
            }
        }
    }

    if commands_path.is_some() && transition_kind.is_some() {
        return Err(
            "--commands is unavailable with --transition: transitions composite whole CPU frames"
                .to_string(),
        );
    }
    let mut renderer = Renderer::try_new(width, height).map_err(|e| e.to_string())?;
    let mut frame = renderer.render_at(&ir, &state, elapsed_ms);
    if let Some(kind) = transition_kind {
        let mut transition = Transition::new(kind);
        transition.tick(elapsed_ms);
        let outgoing = Frame {
            width,
            height,
            pixels: renderer.pixels_at(&ir, &state, 0),
            dirty: Vec::new(),
        };
        let incoming = Frame {
            width,
            height,
            pixels: renderer.pixels_at(&ir, &state, SETTLED_MS),
            dirty: Vec::new(),
        };
        frame = transition.compose(&outgoing, &incoming)?;
    }

    let mut ppm = Vec::with_capacity(frame.width as usize * frame.height as usize * 3 + 32);
    ppm.extend_from_slice(format!("P6\n{} {}\n255\n", frame.width, frame.height).as_bytes());
    for px in frame.pixels.chunks_exact(4) {
        ppm.extend_from_slice(&px[..3]);
    }
    fs::write(&args[3], ppm).map_err(|e| format!("cannot write {:?}: {e}", args[3]))?;
    if let Some(path) = commands_path {
        let json = renderer
            .display_list_at(&ir, &state, elapsed_ms)
            .to_json()
            .map_err(|e| e.to_string())?;
        fs::write(&path, json).map_err(|e| format!("cannot write {:?}: {e}", path))?;
    }
    if let Some(path) = a11y_path {
        let json = a11y_json_at(&ir, &state, width, height, elapsed_ms);
        fs::write(&path, json).map_err(|e| format!("cannot write {:?}: {e}", path))?;
    }
    if let Some(path) = svg_path {
        let doc = svg_at(&ir, &state, width, height, elapsed_ms);
        fs::write(&path, doc).map_err(|e| format!("cannot write {:?}: {e}", path))?;
    }

    println!("digest={}", digest_hex(&frame.pixels));
    println!("dirty={}", frame.dirty.len());
    Ok(())
}
