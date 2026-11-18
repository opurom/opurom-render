//! `opurom-render <ir.json> <width> <height> <out.ppm> [--state name=value ...] [--interactive-state default|hover|active|focus|disabled|loading|success|error] [--tick 0..4000] [--transition slide|fade|scale, defaults to the IR] [--a11y out.json] [--svg out.svg] [--commands out.json] [--frame-budget 0..1000] [--quality compat|product] [--incremental] [--edge-aa]`
//!
//! Writes a P6 PPM snapshot (RGB; the opaque alpha channel is dropped) and
//! prints `digest=<16 hex>` (FNV-1a 64 over RGBA bytes) plus `dirty=<count>`.
//! `--a11y` additionally writes the deterministic accessibility tree,
//! `--svg` a deterministic SVG 1.1 vector snapshot, and `--commands` the exact
//! raster display list plus optional effect commands. `--state` remains the
//! state-variable override; `--interactive-state` applies one visual state to
//! every interactive node and leaves stdout unchanged. `--transition` composes
//! the frame at tick 0 with the
//! settled frame; the stdout shape is unchanged. `--transition` is optional: it
//! takes precedence over the optional top-level `transition` key of the IR, which
//! selects the same slide/fade/scale tier when the flag is absent. The optional
//! `--frame-budget` applies the caller-supplied quality policy (fewer particles,
//! effect passes off) and prints one extra `quality=` line; without it the
//! frame is unchanged. Relative `Image.src` paths resolve against the process
//! CWD, not the IR file's directory; missing or invalid PNGs draw a placeholder.
//! `--auto-degrade <60|120>` wires the measured frame budget into the render:
//! since the CLI renders one frame (there is no live loop), `--frame-ms a,b,c`
//! supplies the measured durations and `--frame-samples <n>` repeats that
//! pattern to fill `n` injected frames (a tier decision needs `WINDOW_FRAMES`
//! samples). The resulting tier caps are applied to the render config; stdout
//! stays exactly `digest=...` then `dirty=...`.

use opurom_render::{
    a11y_json_at, apply_policy, choose_quality, digest_hex, is_interactive, parse_ir, svg_at,
    Frame, FrameBudget, FrameRate, Ir, IrError, Node, NodeState, QualityProfile, QualityTier,
    RenderConfig, Renderer, StateValue, StateView, Transition, TransitionKind, ValueType,
    SETTLED_MS, WINDOW_FRAMES,
};
use std::{env, fs, process};

/// Measured frame budgets above this are not meaningful; the policy saturates
/// at 16 ms anyway.
const MAX_FRAME_BUDGET_MS: i64 = 1000;
/// Bounds for an explicit `--frame-ms` sample and the simulated frame count.
const MAX_FRAME_MS: i64 = 1000;
const MAX_FRAME_SAMPLES: usize = 100_000;

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

/// Feed explicit measured frame durations into a `FrameBudget` and apply the
/// resulting tier caps to a `RenderConfig`, returning the effective IR. The CLI
/// renders a single frame (there is no live loop), so `--frame-ms` supplies the
/// measured durations; cycling them over `n` injected frames simulates the loop
/// a host would otherwise drive through `Renderer::render_auto_at`.
fn auto_degrade_ir(ir: &Ir, rate: FrameRate, samples: &[i64], n: usize) -> Ir {
    let mut budget = FrameBudget::new(rate);
    for i in 0..n {
        budget.push(samples[i % samples.len()]);
    }
    let mut config = RenderConfig::new(ir.clone());
    apply_policy(budget.tier(), &mut config);
    config.ir().clone()
}

/// Deterministic one-line tier verdict for `--auto-degrade`, mirroring the
/// same `FrameBudget` sequence the render applies. The decision rules live in
/// `perf.rs` alone; this line exposes the numbers (p50/p95, target, promote
/// threshold) so an external mirror can re-derive the same verdict.
fn auto_degrade_tier_line(rate: FrameRate, samples: &[i64], n: usize) -> String {
    let mut budget = FrameBudget::new(rate);
    for i in 0..n {
        budget.push(samples[i % samples.len()]);
    }
    let report = budget.report();
    let tier = match budget.tier() {
        QualityTier::Full => "full",
        QualityTier::Reduced => "reduced",
        QualityTier::Minimal => "minimal",
    };
    format!(
        "tier={tier} rate={} frames={n} p50={}ms p95={}ms target={}ms promote={}ms",
        rate.name(),
        report.p50_ms,
        report.p95_ms,
        budget.target_ms(),
        budget.promote_ms()
    )
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

/// Override the authored state of every interactive node for a state-matrix
/// screenshot. An authored `disabled` prop still wins in layout.
fn apply_interactive_state(node: &mut Node, caller: NodeState) {
    if is_interactive(node.kind) {
        node.props.state = NodeState::resolve(Some(caller), node.props.state, node.props.disabled);
    }
    for child in &mut node.children {
        apply_interactive_state(child, caller);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opurom_render::{perf_report, QualityTier};

    const IR: &str = r##"{"ir":"0.1.0","app":"Q","state":[],"tree":{"id":0,"kind":"Column","props":{"padding":4},"children":[{"id":1,"kind":"Container","props":{"width":48,"height":32,"color":"#223344","radius":4,"particles":{"count":64,"seed":7},"effects":[{"kind":"blur","radius":2}]},"children":[],"events":{}}],"events":{}},"theme":{"mode":"dark","accent":"#097FEB","radius":4}}"##;

    /// Values above every tier cap, so each tier visibly clamps them.
    const CAP_IR: &str = r##"{"ir":"0.1.0","app":"C","state":[],"tree":{"id":0,"kind":"Column","props":{"padding":4},"children":[{"id":1,"kind":"Container","props":{"width":48,"height":32,"color":"#223344","radius":4,"particles":{"count":512,"seed":7},"effects":[{"kind":"blur","radius":32}]},"children":[],"events":{}}],"events":{}},"theme":{"mode":"dark","accent":"#097FEB","radius":4}}"##;

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

    /// One window of injected samples; a tier decision needs `WINDOW_FRAMES`.
    fn window(budget: &mut FrameBudget, ms: i64) -> QualityTier {
        let mut tier = budget.tier();
        for _ in 0..WINDOW_FRAMES {
            tier = budget.push(ms);
        }
        tier
    }

    #[test]
    fn fast_measured_samples_keep_the_full_tier() {
        for (rate, fast) in [(FrameRate::Hz60, 16i64), (FrameRate::Hz120, 8)] {
            let mut budget = FrameBudget::new(rate);
            assert_eq!(window(&mut budget, fast), QualityTier::Full);
            let report = perf_report(&budget.window());
            assert_eq!(report.tier, QualityTier::Full);
            assert_eq!(report.p95_ms, fast);
        }
    }

    #[test]
    fn slow_measured_samples_degrade_deterministically() {
        let mut budget = FrameBudget::new(FrameRate::Hz60);
        assert_eq!(window(&mut budget, 30), QualityTier::Reduced);
        assert_eq!(window(&mut budget, 30), QualityTier::Minimal);
        // The 120 fps budget degrades on the same samples: its target is 8 ms.
        let mut tight = FrameBudget::new(FrameRate::Hz120);
        assert_eq!(window(&mut tight, 10), QualityTier::Reduced);
    }

    #[test]
    fn auto_degrade_ir_applies_tier_caps_to_the_render_config() {
        // Values above every tier cap, so each tier visibly clamps them.
        let base = parse_ir(CAP_IR).unwrap();
        let count = |ir: &Ir| ir.tree.children[0].props.particles.unwrap().count;
        let radius = |ir: &Ir| ir.tree.children[0].props.effects[0].radius;
        // Fast samples keep the Full tier, which is a no-op for valid IR.
        let full = auto_degrade_ir(&base, FrameRate::Hz60, &[16], WINDOW_FRAMES);
        assert_eq!(count(&full), 512);
        assert_eq!(radius(&full), 32);
        // One slow window degrades one tier and halves the particle field.
        let reduced = auto_degrade_ir(&base, FrameRate::Hz60, &[30], WINDOW_FRAMES);
        assert_eq!(count(&reduced), 256);
        assert_eq!(radius(&reduced), 8);
        // A second slow window saturates at Minimal.
        let minimal = auto_degrade_ir(&base, FrameRate::Hz60, &[30], 2 * WINDOW_FRAMES);
        assert_eq!(count(&minimal), 64);
        assert_eq!(radius(&minimal), 2);
    }

    #[test]
    fn tier_line_matches_the_budget_verdict() {
        // One slow window over the 16 ms target: p95=30 > 16 -> reduced.
        let slow = auto_degrade_tier_line(FrameRate::Hz60, &[30], WINDOW_FRAMES);
        assert_eq!(
            slow,
            "tier=reduced rate=60 frames=20 p50=30ms p95=30ms target=16ms promote=12ms"
        );
        // Fast samples stay at Full: p95=16 is neither above target nor below
        // the promote threshold.
        let fast = auto_degrade_tier_line(FrameRate::Hz60, &[16], WINDOW_FRAMES);
        assert!(fast.starts_with("tier=full rate=60 frames=20 p50=16ms p95=16ms"));
        // The 120 fps budget degrades on samples its 8 ms target rejects.
        let tight = auto_degrade_tier_line(FrameRate::Hz120, &[10], WINDOW_FRAMES);
        assert!(tight.starts_with("tier=reduced rate=120 frames=20 p50=10ms p95=10ms target=8ms"));
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
        // A list is compile-time data and cannot be set by a single CLI value.
        ValueType::List => Err(format!("state value {raw:?} cannot set a list state")),
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() < 4 {
        return Err(
            "usage: opurom-render <ir.json> <width> <height> <out.ppm> [--state name=value ...] [--interactive-state default|hover|active|focus|disabled|loading|success|error] [--tick 0..4000] [--transition slide|fade|scale, defaults to the IR] [--a11y out.json] [--svg out.svg] [--commands out.json] [--frame-budget 0..1000] [--quality compat|product] [--incremental] [--edge-aa] [--auto-degrade 60|120 --frame-ms a,b,c [--frame-samples n]]"
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
    let mut matrix_state: Option<NodeState> = None;
    // Opt-in incremental raster: reuse the previous frame and redraw only the
    // changed region. Without it the full reference raster is used.
    let mut incremental = false;
    // Explicit compatibility downgrade; the renderer default stays Product.
    let mut quality: QualityProfile = QualityProfile::default();
    let mut edge_aa = false;
    // Opt-in measured-frame budget: inject explicit samples (the CLI has no
    // live render loop) and apply the resulting tier caps to the render config.
    let mut auto_rate: Option<FrameRate> = None;
    let mut frame_ms: Vec<i64> = Vec::new();
    let mut frame_samples: Option<usize> = None;

    let mut i = 4usize;
    while i < args.len() {
        match args[i].as_str() {
            "--state" => {
                let contract = args
                    .get(i + 1)
                    .ok_or_else(|| "--state requires a name=value argument".to_string())?;
                let (name, raw) = contract
                    .split_once('=')
                    .ok_or_else(|| format!("--state expects name=value, got {contract:?}"))?;
                let decl = ir
                    .state
                    .iter()
                    .find(|d| d.name == name)
                    .ok_or_else(|| format!("unknown state name {name:?}"))?;
                state.set(name.to_string(), parse_value(decl.typ, raw)?);
                i += 2;
            }
            "--interactive-state" => {
                let contract = args.get(i + 1).ok_or_else(|| {
                    "--interactive-state requires default, hover, active, focus, disabled, loading, success or error"
                        .to_string()
                })?;
                matrix_state = Some(NodeState::parse(contract).ok_or_else(|| {
                    format!(
                        "unknown interactive state {contract:?}: expected default, hover, active, focus, disabled, loading, success or error"
                    )
                })?);
                i += 2;
            }
            "--tick" => {
                let contract = args
                    .get(i + 1)
                    .ok_or_else(|| "--tick requires a millisecond value".to_string())?;
                let value: i64 = contract.parse().map_err(|_| {
                    format!("invalid tick {contract:?}: expected 0..={SETTLED_MS} milliseconds")
                })?;
                if !(0..=SETTLED_MS).contains(&value) {
                    return Err(format!(
                        "invalid tick {contract:?}: expected 0..={SETTLED_MS} milliseconds"
                    ));
                }
                elapsed_ms = value;
                i += 2;
            }
            "--transition" => {
                let contract = args
                    .get(i + 1)
                    .ok_or_else(|| "--transition requires slide, fade or scale".to_string())?;
                transition_kind = Some(TransitionKind::parse(contract).ok_or_else(|| {
                    format!("unknown transition {contract:?}: expected slide, fade or scale")
                })?);
                i += 2;
            }
            "--a11y" => {
                let contract = args
                    .get(i + 1)
                    .ok_or_else(|| "--a11y requires an output JSON path".to_string())?;
                a11y_path = Some(contract.clone());
                i += 2;
            }
            "--svg" => {
                let contract = args
                    .get(i + 1)
                    .ok_or_else(|| "--svg requires an output SVG path".to_string())?;
                svg_path = Some(contract.clone());
                i += 2;
            }
            "--commands" => {
                let contract = args
                    .get(i + 1)
                    .ok_or_else(|| "--commands requires an output JSON path".to_string())?;
                commands_path = Some(contract.clone());
                i += 2;
            }
            "--frame-budget" => {
                let contract = args
                    .get(i + 1)
                    .ok_or_else(|| "--frame-budget requires a millisecond value".to_string())?;
                let value: i64 = contract.parse().map_err(|_| {
                    format!("invalid frame budget {contract:?}: expected 0..={MAX_FRAME_BUDGET_MS} milliseconds")
                })?;
                if !(0..=MAX_FRAME_BUDGET_MS).contains(&value) {
                    return Err(format!(
                        "invalid frame budget {contract:?}: expected 0..={MAX_FRAME_BUDGET_MS} milliseconds"
                    ));
                }
                frame_budget_ms = Some(value);
                i += 2;
            }
            "--incremental" => {
                incremental = true;
                i += 1;
            }
            "--quality" => {
                let value = args
                    .get(i + 1)
                    .ok_or("--quality requires compat or product")?;
                quality = QualityProfile::parse(value).ok_or_else(|| {
                    format!("unknown quality {value:?}: expected compat or product")
                })?;
                i += 2;
            }
            "--edge-aa" => {
                edge_aa = true;
                i += 1;
            }
            "--auto-degrade" => {
                let contract = args
                    .get(i + 1)
                    .ok_or_else(|| "--auto-degrade requires 60 or 120".to_string())?;
                auto_rate = Some(FrameRate::parse(contract).ok_or_else(|| {
                    format!("invalid auto-degrade {contract:?}: expected 60 or 120")
                })?);
                i += 2;
            }
            "--frame-ms" => {
                let contract = args.get(i + 1).ok_or_else(|| {
                    "--frame-ms requires comma-separated millisecond samples".to_string()
                })?;
                let parsed: Vec<i64> = contract
                    .split(',')
                    .map(|p| {
                        p.trim().parse::<i64>().map_err(|_| {
                            format!("invalid frame sample {p:?}: expected 0..={MAX_FRAME_MS}")
                        })
                    })
                    .collect::<Result<_, _>>()?;
                if parsed.is_empty() || parsed.iter().any(|&v| !(0..=MAX_FRAME_MS).contains(&v)) {
                    return Err(format!(
                        "invalid --frame-ms {contract:?}: expected comma-separated 0..={MAX_FRAME_MS} values"
                    ));
                }
                frame_ms = parsed;
                i += 2;
            }
            "--frame-samples" => {
                let contract = args
                    .get(i + 1)
                    .ok_or_else(|| "--frame-samples requires a positive integer".to_string())?;
                let value: usize = contract.parse().map_err(|_| {
                    format!("invalid frame samples {contract:?}: expected 1..={MAX_FRAME_SAMPLES}")
                })?;
                if !(1..=MAX_FRAME_SAMPLES).contains(&value) {
                    return Err(format!(
                        "invalid frame samples {contract:?}: expected 1..={MAX_FRAME_SAMPLES}"
                    ));
                }
                frame_samples = Some(value);
                i += 2;
            }
            other => {
                return Err(format!(
                    "unexpected argument {other:?}: expected --state name=value, --interactive-state default|hover|active|focus|disabled|loading|success|error, --tick 0..={SETTLED_MS}, --transition slide|fade|scale (defaults to the IR), --a11y out.json, --svg out.svg, --commands out.json, --frame-budget 0..={MAX_FRAME_BUDGET_MS}, --quality compat|product, --incremental, --edge-aa, --auto-degrade 60|120, --frame-ms a,b,c or --frame-samples n"
                ));
            }
        }
    }

    // Cross-validate the measured-frame flags so no sample is silently ignored.
    if auto_rate.is_some() && frame_ms.is_empty() {
        return Err(
            "--auto-degrade requires --frame-ms a,b,c samples (the CLI has no render loop)"
                .to_string(),
        );
    }
    if auto_rate.is_none() && (!frame_ms.is_empty() || frame_samples.is_some()) {
        return Err("--frame-ms and --frame-samples require --auto-degrade 60|120".to_string());
    }

    // The flag wins when present; otherwise the optional IR `transition` key
    // selects the kind.
    transition_kind = transition_kind.or(ir.transition);
    if let Some(caller) = matrix_state {
        apply_interactive_state(&mut ir.tree, caller);
    }

    if commands_path.is_some() && transition_kind.is_some() {
        return Err(
            "--commands is unavailable with --transition: transitions composite whole CPU frames"
                .to_string(),
        );
    }
    if incremental && transition_kind.is_some() {
        return Err(
            "--incremental is unavailable with --transition: transitions composite whole CPU frames"
                .to_string(),
        );
    }
    if edge_aa && commands_path.is_some() {
        return Err("--edge-aa is unavailable with --commands: the display-list stream has no coverage field"
            .to_string());
    }
    // Opt-in only: without the flag the IR tree is untouched, so the default
    // path is byte-for-byte identical to previous releases.
    let quality_line = frame_budget_ms.map(|ms| apply_quality(&mut ir.tree, ms));
    let mut tier_line: Option<String> = None;
    // Opt-in measured-frame budget: apply the decided tier caps to the render
    // config. Without it the IR is untouched, so the default path is identical.
    if let Some(rate) = auto_rate {
        let n = frame_samples.unwrap_or(WINDOW_FRAMES);
        ir = auto_degrade_ir(&ir, rate, &frame_ms, n);
        tier_line = Some(auto_degrade_tier_line(rate, &frame_ms, n));
    }
    let mut renderer = Renderer::try_new(width, height).map_err(|e| e.to_string())?;
    let quality = QualityProfile::resolve(Some(quality), edge_aa, false);
    renderer.set_quality(quality);
    renderer.set_edge_aa(edge_aa);
    let (mut frame, incremental_line) = if incremental {
        let frame = renderer.render_incremental_at(&ir, &state, elapsed_ms);
        let (nodes, pixels) = renderer.redraw_stats();
        (
            frame,
            Some(format!("incremental=nodes={nodes} pixels={pixels}")),
        )
    } else {
        (renderer.render_at(&ir, &state, elapsed_ms), None)
    };
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
    if let Some(line) = incremental_line {
        println!("{line}");
    }
    if let Some(line) = quality_line {
        println!("{line}");
    }
    if let Some(line) = tier_line {
        println!("{line}");
    }
    Ok(())
}
