//! `opurom-render <ir.json> <width> <height> <out.ppm> [--state name=value ...] [--tick ms] [--a11y out.json] [--svg out.svg]`
//!
//! Writes a P6 PPM snapshot (RGB; the opaque alpha channel is dropped) and
//! prints `digest=<16 hex>` (FNV-1a 64 over RGBA bytes) plus `dirty=<count>`.
//! `--a11y` additionally writes the deterministic R3 accessibility tree and
//! `--svg` a deterministic SVG 1.1 vector snapshot of the same layout.

use opurom_render::{
    a11y_json_at, digest_hex, parse_ir, svg_at, IrError, Renderer, StateValue, StateView,
    ValueType, SETTLED_MS,
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
            "usage: opurom-render <ir.json> <width> <height> <out.ppm> [--state name=value ...] [--tick ms] [--a11y out.json] [--svg out.svg]"
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
    // Animation clock position in milliseconds (R2).
    let mut elapsed_ms: i64 = SETTLED_MS;
    let mut a11y_path: Option<String> = None;
    let mut svg_path: Option<String> = None;

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
                elapsed_ms = spec
                    .parse()
                    .map_err(|_| format!("invalid tick {:?}: expected milliseconds", spec))?;
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
            other => {
                return Err(format!(
                    "unexpected argument {other:?}: expected --state name=value, --tick ms, --a11y out.json or --svg out.svg"
                ));
            }
        }
    }

    let mut renderer = Renderer::try_new(width, height).map_err(|e| e.to_string())?;
    let frame = renderer.render_at(&ir, &state, elapsed_ms);

    let mut ppm = Vec::with_capacity(frame.width as usize * frame.height as usize * 3 + 32);
    ppm.extend_from_slice(format!("P6\n{} {}\n255\n", frame.width, frame.height).as_bytes());
    for px in frame.pixels.chunks_exact(4) {
        ppm.extend_from_slice(&px[..3]);
    }
    fs::write(&args[3], ppm).map_err(|e| format!("cannot write {:?}: {e}", args[3]))?;
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
