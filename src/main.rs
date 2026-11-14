//! `opurom-render <ir.json> <width> <height> <out.ppm> [--state name=value ...] [--tick 0..4000] [--transition slide|fade|scale, defaults to the IR] [--a11y out.json] [--svg out.svg] [--commands out.json] [--frame-budget 0..1000]`
//!
//! Writes a P6 PPM snapshot (RGB; the opaque alpha channel is dropped) and
//! prints `digest=<16 hex>` (FNV-1a 64 over RGBA bytes) plus `dirty=<count>`.
//! `--a11y` additionally writes the deterministic accessibility tree,
//! `--svg` a deterministic SVG 1.1 vector snapshot, and `--commands` the exact
//! raster display list plus optional effect commands. `--transition` composes the frame at tick 0 with the
//! settled frame; the stdout shape is unchanged. `--transition` is optional: it
//! takes precedence over the optional top-level `transition` key of the IR, which
//! selects the same slide/fade/scale tier when the flag is absent. The optional
//! `--frame-budget` applies the caller-supplied quality policy (fewer particles,
//! effect passes off) and prints one extra `quality=` line; without it the
//! frame is unchanged.

use opurom_render::{
    a11y_json_at, choose_quality, digest_hex, parse_ir, svg_at, Frame, IrError, Node, Renderer,
    StateValue, StateView, Transition, TransitionKind, ValueType, SETTLED_MS,
};
use std::{env, fs, process};

/// Measured frame budgets above this are not meaningful; the policy saturates
/// at 16 ms anyway.
const MAX_FRAME_BUDGET_MS: i64 = 1000;

fn main() {
    if let Err(e) = run() {
        eprintln!("opurom-render: error: {e}");
        process::exit(1);
    }
}

/// Per-field particle count under a measured frame budget. The field's own
/// count is the base, so every field degrades proportionally.
fn degraded_particle_count(budget_ms: i64, base: i64) -> i64 {
    choose_quality(budget_ms, base.max(0) as usize, true).particle_count as i64
}

/// Applies `choose_quality` to the parsed IR tree in place, so the render
/// pipeline and its signatures stay untouched. When the tier disables effects
/// the software effect passes (blur/glow/glass/liquid from the `effects` prop)
/// are dropped; static shadow/gradient/opacity props remain. Returns the
/// deterministic `quality=` line.
fn apply_quality(tree: &mut Node, budget_ms: i64) -> String {
    fn walk(node: &mut Node, budget_ms: i64, effects_enabled: bool) -> (usize, usize) {
        let mut fields = 0usize;
        let mut dots = 0usize;
        if let Some(p) = node.props.particles.as_mut() {
            p.count = degraded_particle_count(budget_ms, p.count);
            fields += 1;
            dots += p.count.max(0) as usize;
        }
        if !effects_enabled {
            node.props.effects.clear();
        }
        for child in &mut node.children {
            let (f, d) = walk(child, budget_ms, effects_enabled);
            fields += f;
            dots += d;
        }
        (fields, dots)
    }
    // The effects decision depends only on the budget, not the particle count.
    let effects_enabled = choose_quality(budget_ms, 0, true).effects_enabled;
    let (fields, dots) = walk(tree, budget_ms, effects_enabled);
    format!(
        "quality=budget={}ms effects={} fields={} particles={}",
        budget_ms,
        if effects_enabled { "on" } else { "off" },
        fields,
        dots
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const IR: &str = r##"{"ir":"0.1","app":"Q","state":[],"tree":{"id":0,"kind":"Column","props":{"padding":4},"children":[{"id":1,"kind":"Container","props":{"width":48,"height":32,"color":"#223344","radius":4,"particles":{"count":64,"seed":7},"effects":[{"kind":"blur","radius":2}]},"children":[],"events":{}}],"events":{}},"theme":{"mode":"dark","accent":"#097FEB","radius":4}}"##;

    #[test]
    fn degraded_particle_count_is_deterministic_and_monotonic() {
        assert_eq!(degraded_particle_count(4, 64), 32);
        assert_eq!(degraded_particle_count(16, 64), 64);
        assert_eq!(degraded_particle_count(4, -5), 0);
        let mut previous = 0;
        for budget in 0..=24 {
            assert_eq!(
                degraded_particle_count(budget, 64),
                degraded_particle_count(budget, 64)
            );
            let n = degraded_particle_count(budget, 64);
            assert!(n <= 64, "budget {budget} exceeded the base count");
            assert!(n >= previous, "budget {budget} reduced the count");
            previous = n;
        }
    }

    #[test]
    fn apply_quality_scales_particles_and_clears_effect_passes() {
        let mut ir = parse_ir(IR).unwrap();
        let line = apply_quality(&mut ir.tree, 4);
        assert_eq!(ir.tree.children[0].props.particles.unwrap().count, 32);
        assert!(ir.tree.children[0].props.effects.is_empty());
        assert_eq!(line, "quality=budget=4ms effects=off fields=1 particles=32");

        let mut ir = parse_ir(IR).unwrap();
        let line = apply_quality(&mut ir.tree, 16);
        assert_eq!(ir.tree.children[0].props.particles.unwrap().count, 64);
        assert_eq!(ir.tree.children[0].props.effects.len(), 1);
        assert_eq!(line, "quality=budget=16ms effects=on fields=1 particles=64");
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
            "usage: opurom-render <ir.json> <width> <height> <out.ppm> [--state name=value ...] [--tick 0..4000] [--transition slide|fade|scale, defaults to the IR] [--a11y out.json] [--svg out.svg] [--commands out.json] [--frame-budget 0..1000]"
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
    let mut ir = parse_ir(&text).map_err(|e: IrError| e.to_string())?;
    let mut state = StateView::from_ir(&ir);
    // Animation clock position in milliseconds; also drives transitions.
    let mut elapsed_ms: i64 = SETTLED_MS;
    let mut transition_kind: Option<TransitionKind> = None;
    let mut a11y_path: Option<String> = None;
    let mut svg_path: Option<String> = None;
    let mut commands_path: Option<String> = None;
    let mut frame_budget_ms: Option<i64> = None;

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
            "--frame-budget" => {
                let spec = args
                    .get(i + 1)
                    .ok_or_else(|| "--frame-budget requires a millisecond value".to_string())?;
                let value: i64 = spec.parse().map_err(|_| {
                    format!("invalid frame budget {spec:?}: expected 0..={MAX_FRAME_BUDGET_MS} milliseconds")
                })?;
                if !(0..=MAX_FRAME_BUDGET_MS).contains(&value) {
                    return Err(format!(
                        "invalid frame budget {spec:?}: expected 0..={MAX_FRAME_BUDGET_MS} milliseconds"
                    ));
                }
                frame_budget_ms = Some(value);
                i += 2;
            }
            other => {
                return Err(format!(
                    "unexpected argument {other:?}: expected --state name=value, --tick 0..={SETTLED_MS}, --transition slide|fade|scale (defaults to the IR), --a11y out.json, --svg out.svg, --commands out.json or --frame-budget 0..={MAX_FRAME_BUDGET_MS}"
                ));
            }
        }
    }

    // The flag wins when present; otherwise the optional IR `transition` key
    // selects the kind.
    transition_kind = transition_kind.or(ir.transition);

    if commands_path.is_some() && transition_kind.is_some() {
        return Err(
            "--commands is unavailable with --transition: transitions composite whole CPU frames"
                .to_string(),
        );
    }
    // Opt-in only: without the flag the IR tree is untouched, so the default
    // path is byte-for-byte identical to previous releases.
    let quality_line = frame_budget_ms.map(|ms| apply_quality(&mut ir.tree, ms));
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
    if let Some(line) = quality_line {
        println!("{line}");
    }
    Ok(())
}
