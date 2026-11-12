//! `opurom-render <ir.json> <width> <height> <out.ppm> [--state name=value ...]`
//!
//! Writes a P6 PPM snapshot (RGB; the opaque alpha channel is dropped) and
//! prints `digest=<16 hex>` (FNV-1a 64 over RGBA bytes) plus `dirty=<count>`.

use opurom_render::{digest_hex, parse_ir, IrError, Renderer, StateValue, StateView, ValueType};
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
            "usage: opurom-render <ir.json> <width> <height> <out.ppm> [--state name=value ...]"
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

    let mut i = 4usize;
    while i < args.len() {
        if args[i] != "--state" {
            return Err(format!(
                "unexpected argument {:?}: expected --state name=value",
                args[i]
            ));
        }
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

    let mut renderer = Renderer::try_new(width, height).map_err(|e| e.to_string())?;
    let frame = renderer.render(&ir, &state);

    let mut ppm = Vec::with_capacity(frame.width as usize * frame.height as usize * 3 + 32);
    ppm.extend_from_slice(format!("P6\n{} {}\n255\n", frame.width, frame.height).as_bytes());
    for px in frame.pixels.chunks_exact(4) {
        ppm.extend_from_slice(&px[..3]);
    }
    fs::write(&args[3], ppm).map_err(|e| format!("cannot write {:?}: {e}", args[3]))?;

    println!("digest={}", digest_hex(&frame.pixels));
    println!("dirty={}", frame.dirty.len());
    Ok(())
}
