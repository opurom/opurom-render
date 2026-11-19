//! The `--auto-degrade <60|120>` flag wires the measured frame budget into the
//! production CLI. The CLI renders one frame, so `--frame-ms a,b,c` supplies
//! the measured durations and `--frame-samples <n>` repeats that pattern over
//! `n` injected frames (a tier decision needs `WINDOW_FRAMES` samples). The
//! resulting tier caps are applied to the render config; stdout stays exactly
//! `digest=...` then `dirty=...` for every invocation, including the default.

use opurom_render::{
    apply_policy, digest_hex, parse_ir, FrameBudget, FrameRate, RenderConfig, Renderer, StateView,
    WINDOW_FRAMES,
};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{fs, path::Path, path::PathBuf};

/// An IR whose particle field and effect passes visibly shrink per tier.
const PERF_IR: &str = r##"{
  "ir": "0.1.0", "app": "Perf", "state": [],
  "tree": {
    "id": 0, "kind": "Column", "props": { "padding": 4 },
    "children": [
      { "id": 1, "kind": "Container", "props": {
          "width": 80, "height": 50, "color": "#223344",
          "particles": { "count": 512, "seed": 7, "radius": 32, "alpha": 512 },
          "effects": [ { "kind": "blur", "radius": 32 } ]
        }, "children": [], "events": {} }
    ], "events": {}
  },
  "theme": { "mode": "dark", "accent": "#097FEB", "radius": 4 }
}"##;

const GOLDEN_IR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../opurom-examples/golden/counter.opuir.json"
);

fn run(ir: &Path, ppm: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_opurom-render"))
        .arg(ir)
        .args(["120", "90"])
        .arg(ppm)
        .args(["--quality", "compat"])
        .args(args)
        .output()
        .unwrap()
}

fn stdout(ir: &Path, ppm: &Path, args: &[&str]) -> String {
    let out = run(ir, ppm, args);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn digest(ir: &Path, ppm: &Path, args: &[&str]) -> String {
    let out = stdout(ir, ppm, args);
    let first = out.lines().next().unwrap_or_default();
    assert!(first.starts_with("digest="), "{out}");
    first.trim_start_matches("digest=").to_string()
}

fn fixtures() -> (PathBuf, PathBuf) {
    // Tests in one binary run in parallel and share CARGO_TARGET_TMPDIR. A
    // per-call counter makes every IR/PPM path unique, so a concurrent write
    // cannot truncate the file another test's CLI is reading.
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let tag = format!(
        "auto-degrade-{}-{}-",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let ir = dir.join(format!("{tag}fixture.opuir.json"));
    fs::write(&ir, PERF_IR).unwrap();
    (ir, dir.join(format!("{tag}frame.ppm")))
}

#[test]
fn no_flags_keep_stdout_shape_and_the_three_canonical_goldens() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let golden = Path::new(GOLDEN_IR);
    let ppm = dir.join(format!("auto-degrade-golden-{}.ppm", std::process::id()));
    for (wh, state, expected) in [
        (["320", "240"], &[] as &[&str], "8d45f412996a99c6"),
        (["800", "600"], &[], "318beb1dd337bc4e"),
        (["800", "600"], &["--state", "count=7"], "40073cb95c912d80"),
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_opurom-render"))
            .arg(golden)
            .args(wh)
            .arg(&ppm)
            .args(["--quality", "compat"])
            .args(state)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        assert_eq!(
            text,
            format!("digest={expected}\ndirty=1\n"),
            "stdout must stay exactly digest then dirty"
        );
    }
}

#[test]
fn the_measured_frame_flags_are_validated() {
    let (ir, ppm) = fixtures();
    for (args, expected) in [
        (
            vec!["--auto-degrade", "60"],
            "--auto-degrade requires --frame-ms",
        ),
        (
            vec!["--auto-degrade", "30", "--frame-ms", "30"],
            "invalid auto-degrade",
        ),
        (vec!["--frame-ms", "30"], "require --auto-degrade"),
        (vec!["--frame-samples", "20"], "require --auto-degrade"),
        (
            vec!["--auto-degrade", "60", "--frame-ms", "abc"],
            "invalid frame sample",
        ),
        (
            vec![
                "--auto-degrade",
                "60",
                "--frame-ms",
                "30",
                "--frame-samples",
                "0",
            ],
            "invalid frame samples",
        ),
        (
            vec!["--auto-degrade", "60", "--frame-ms", "1001"],
            "invalid --frame-ms",
        ),
    ] {
        let out = run(&ir, &ppm, &args);
        assert_eq!(out.status.code(), Some(1), "{args:?} must fail cleanly");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains(expected), "{args:?}: {stderr}");
    }
}

#[test]
fn fast_samples_keep_the_baseline_frame_and_stdout_shape() {
    let (ir, ppm) = fixtures();
    let baseline = stdout(&ir, &ppm, &[]);
    // At the 60 fps target every 16 ms sample is exactly on budget, so the
    // Full tier is a no-op and the frame is byte-identical to the default.
    let fast = stdout(&ir, &ppm, &["--auto-degrade", "60", "--frame-ms", "16"]);
    assert_eq!(
        fast.lines().take(2).collect::<Vec<_>>().join("\n") + "\n",
        baseline,
        "fast samples must not change the frame"
    );
    assert_eq!(fast.lines().count(), 3, "digest, dirty, then the tier line");
    assert!(
        fast.starts_with("digest=") && fast.contains("\ndirty="),
        "{fast}"
    );
    // The tier verdict is one stable machine-parsable line.
    let line = fast.lines().nth(2).unwrap();
    assert!(
        line.starts_with("tier=full rate=60 frames=20 p50=16ms p95=16ms target=16ms promote=12ms"),
        "{line}"
    );
    // Without --auto-degrade the stdout stays digest then dirty, byte-identical
    // to previous releases.
    assert_eq!(stdout(&ir, &ppm, &[]).lines().count(), 2);
}

#[test]
fn slow_samples_degrade_once_per_full_window() {
    let (ir, ppm) = fixtures();
    let baseline = digest(&ir, &ppm, &[]);
    // 19 injected frames never complete a rolling window, so no tier decision
    // is taken and the frame stays on the baseline. The tier line still prints
    // and names the not-yet-decided Full tier.
    let pre = stdout(
        &ir,
        &ppm,
        &[
            "--auto-degrade",
            "60",
            "--frame-ms",
            "30",
            "--frame-samples",
            "19",
        ],
    );
    assert!(
        pre.lines().nth(2).unwrap().starts_with("tier=full "),
        "{pre}"
    );
    assert_eq!(
        digest(
            &ir,
            &ppm,
            &[
                "--auto-degrade",
                "60",
                "--frame-ms",
                "30",
                "--frame-samples",
                "19"
            ]
        ),
        baseline,
        "an incomplete window must not degrade"
    );
    let reduced = digest(
        &ir,
        &ppm,
        &[
            "--auto-degrade",
            "60",
            "--frame-ms",
            "30",
            "--frame-samples",
            "20",
        ],
    );
    assert_ne!(reduced, baseline, "one full slow window must degrade");
    let reduced_out = stdout(
        &ir,
        &ppm,
        &[
            "--auto-degrade",
            "60",
            "--frame-ms",
            "30",
            "--frame-samples",
            "20",
        ],
    );
    assert!(
        reduced_out.lines().nth(2).unwrap().starts_with(
            "tier=reduced rate=60 frames=20 p50=30ms p95=30ms target=16ms promote=12ms"
        ),
        "{reduced_out}"
    );
    let minimal = digest(
        &ir,
        &ppm,
        &[
            "--auto-degrade",
            "60",
            "--frame-ms",
            "30",
            "--frame-samples",
            "40",
        ],
    );
    assert_ne!(minimal, reduced, "a second slow window degrades further");
    // The 120 fps target is 8 ms, so 10 ms samples degrade too.
    let tight = digest(
        &ir,
        &ppm,
        &[
            "--auto-degrade",
            "120",
            "--frame-ms",
            "10",
            "--frame-samples",
            "20",
        ],
    );
    assert_ne!(tight, baseline, "120 fps degrades on the same slow samples");
}

#[test]
fn the_cli_applies_exactly_the_library_tier_caps() {
    let (ir, ppm) = fixtures();
    let doc = parse_ir(PERF_IR).unwrap();
    let state = StateView::from_ir(&doc);
    let expected = |frames: usize, ms: i64, rate: FrameRate| {
        let mut budget = FrameBudget::new(rate);
        for _ in 0..frames {
            budget.push(ms);
        }
        let mut config = RenderConfig::new(doc.clone());
        apply_policy(budget.tier(), &mut config);
        digest_hex(&Renderer::new(120, 90).render(config.ir(), &state).pixels)
    };
    for (args, frames, ms, rate) in [
        (
            vec![
                "--auto-degrade",
                "60",
                "--frame-ms",
                "30",
                "--frame-samples",
                "20",
            ],
            WINDOW_FRAMES,
            30,
            FrameRate::Hz60,
        ),
        (
            vec![
                "--auto-degrade",
                "60",
                "--frame-ms",
                "30",
                "--frame-samples",
                "40",
            ],
            2 * WINDOW_FRAMES,
            30,
            FrameRate::Hz60,
        ),
        (
            vec![
                "--auto-degrade",
                "120",
                "--frame-ms",
                "10",
                "--frame-samples",
                "20",
            ],
            WINDOW_FRAMES,
            10,
            FrameRate::Hz120,
        ),
    ] {
        assert_eq!(
            digest(&ir, &ppm, &args),
            expected(frames, ms, rate),
            "{args:?} must match the library policy"
        );
    }
}

#[test]
fn frame_ms_samples_are_cycled_to_fill_the_window() {
    let (ir, ppm) = fixtures();
    let baseline = digest(&ir, &ppm, &[]);
    // Fast values cycle into a full window and stay on budget.
    assert_eq!(
        digest(&ir, &ppm, &["--auto-degrade", "60", "--frame-ms", "1,2,3"]),
        baseline,
        "cycled fast samples keep the Full tier"
    );
    // Slow values cycle into the same window and degrade.
    let slow = digest(
        &ir,
        &ppm,
        &["--auto-degrade", "60", "--frame-ms", "30,40,50"],
    );
    assert_ne!(slow, baseline, "cycled slow samples degrade");
    // The same list with an explicit count reproduces the same decision.
    assert_eq!(
        digest(
            &ir,
            &ppm,
            &[
                "--auto-degrade",
                "60",
                "--frame-ms",
                "30,40,50",
                "--frame-samples",
                "20"
            ]
        ),
        slow
    );
}
