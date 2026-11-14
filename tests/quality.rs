//! The `--frame-budget` flag wires `choose_quality` into the production CLI.
//! The no-flag path must stay byte-for-byte identical to the library render,
//! while an opt-in budget degrades particle fields and effect passes and prints
//! one extra `quality=` line.

use opurom_render::{digest_hex, parse_ir, Renderer, StateView};
use std::{fs, path::PathBuf, process::Command};

const QUALITY_IR: &str = r##"{
  "ir": "0.1", "app": "Quality", "state": [],
  "tree": {
    "id": 0, "kind": "Column", "props": { "padding": 4 },
    "children": [
      { "id": 1, "kind": "Container", "props": {
          "width": 120, "height": 80, "color": "#223344", "radius": 4,
          "particles": { "count": 64, "seed": 7, "radius": 3, "alpha": 512, "color": "#F2F6FF" },
          "effects": [ { "kind": "blur", "radius": 2 } ]
        }, "children": [], "events": {} }
    ], "events": {}
  },
  "theme": { "mode": "dark", "accent": "#097FEB", "radius": 4 }
}"##;

/// Pinned no-flag digest of `QUALITY_IR` at 320x240. The CLI default path must
/// never drift from the library renderer, which this change does not touch.
const DEFAULT_DIGEST: &str = "4ee5f628ec3699f6";

fn run(ir: &PathBuf, ppm: &PathBuf, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_opurom-render"))
        .arg(ir)
        .args(["320", "240"])
        .arg(ppm)
        .args(args)
        .output()
        .unwrap()
}

fn stdout(ir: &PathBuf, ppm: &PathBuf, args: &[&str]) -> String {
    let out = run(ir, ppm, args);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn fixtures() -> (PathBuf, PathBuf) {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ir = dir.join("quality.opuir.json");
    fs::write(&ir, QUALITY_IR).unwrap();
    (ir, dir.join("quality.ppm"))
}

#[test]
fn default_path_matches_library_render_and_has_no_quality_line() {
    let (ir_path, ppm) = fixtures();
    let out = stdout(&ir_path, &ppm, &[]);
    assert_eq!(out, format!("digest={DEFAULT_DIGEST}\ndirty=1\n"));

    // Independent oracle: the library renderer, which this flag never touches.
    let ir = parse_ir(QUALITY_IR).unwrap();
    let state = StateView::from_ir(&ir);
    assert_eq!(
        digest_hex(&Renderer::new(320, 240).render(&ir, &state).pixels),
        DEFAULT_DIGEST
    );
    assert!(fs::read(&ppm).unwrap().starts_with(b"P6\n320 240\n255\n"));
}

#[test]
fn frame_budget_degrades_the_frame_and_reports_quality() {
    let (ir_path, ppm) = fixtures();
    let default = stdout(&ir_path, &ppm, &[]);
    let default_bytes = fs::read(&ppm).unwrap();

    let degraded = stdout(&ir_path, &ppm, &["--frame-budget", "4"]);
    assert_eq!(degraded.lines().count(), 3, "only one extra stdout line");
    assert!(degraded.starts_with("digest="), "{degraded}");
    assert!(
        degraded.contains("quality=budget=4ms effects=off fields=1 particles=32"),
        "{degraded}"
    );
    assert!(
        !degraded.starts_with(&format!("digest={DEFAULT_DIGEST}")),
        "the frame must actually change"
    );
    assert_ne!(
        fs::read(&ppm).unwrap(),
        default_bytes,
        "PPM must be rewritten"
    );

    // The no-flag path is still untouched after the degraded run.
    assert_eq!(stdout(&ir_path, &ppm, &[]), default);
}

#[test]
fn frame_budget_tiers_are_monotonic_and_deterministic() {
    let (ir_path, ppm) = fixtures();
    let default_digest = DEFAULT_DIGEST.to_string();
    let mut previous_particles = 0usize;
    for (budget, expected) in [(0, 16), (4, 32), (8, 48), (16, 64)] {
        let args = ["--frame-budget", &budget.to_string()];
        let out = stdout(&ir_path, &ppm, &args);
        assert_eq!(out, stdout(&ir_path, &ppm, &args), "budget {budget}");
        let quality = out
            .lines()
            .find(|l| l.starts_with("quality="))
            .unwrap_or_else(|| panic!("no quality line: {out}"));
        let particles: usize = quality.split("particles=").nth(1).unwrap().parse().unwrap();
        assert_eq!(particles, expected, "budget {budget}: {quality}");
        assert!(particles >= previous_particles, "budget {budget} regressed");
        previous_particles = particles;
    }
    // The full tier (16 ms) leaves the frame identical to the no-flag render.
    let full = stdout(&ir_path, &ppm, &["--frame-budget", "16"]);
    assert!(
        full.starts_with(&format!("digest={default_digest}")),
        "{full}"
    );
}

#[test]
fn frame_budget_is_validated() {
    let (ir_path, ppm) = fixtures();
    for bad in ["-1", "1001", "abc"] {
        let out = run(&ir_path, &ppm, &["--frame-budget", bad]);
        assert_eq!(out.status.code(), Some(1), "{bad} must fail cleanly");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("invalid frame budget"),
            "{bad}"
        );
    }
    let out = run(&ir_path, &ppm, &["--frame-budget"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--frame-budget requires"));
}
