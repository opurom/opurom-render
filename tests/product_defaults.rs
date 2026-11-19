//! Product-quality default and explicit compat downgrade for the CLI.
//!
//! Contract: the CLI renders the highest product specification by default;
//! compat pixels are reached only through an explicit `--quality compat` or
//! `--legacy-golden`. Stdout stays exactly `digest=` then `dirty=` on every
//! path (auto-degrade still appends its own `tier=` line when requested), and
//! the three canonical golden digests are byte-identical under compat.

use opurom_render::{CliMotionSpec, FontMetrics, RenderPolicy, RenderProfile};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const COUNTER: &str = "../opurom-examples/golden/counter.opuir.json";
const GOLDEN_320: &str = "8d45f412996a99c6";
const GOLDEN_800: &str = "318beb1dd337bc4e";
const GOLDEN_800_7: &str = "40073cb95c912d80";

const TEXT_IR: &str = r##"{"ir":"0.1.0","app":"P","state":[],"tree":{"id":0,"kind":"Column","props":{"padding":4,"gap":4},"children":[{"id":1,"kind":"Text","props":{"value":"Width","size":24},"children":[],"events":{}},{"id":2,"kind":"Container","props":{"width":60,"height":40,"color":"#223344","radius":8,"gradient":"#223344,#88CCFF","effects":[{"kind":"blur","radius":4}]},"children":[],"events":{}}],"events":{}},"theme":{"mode":"light","accent":"#097FEB","radius":8}}"##;

static NEXT: AtomicU64 = AtomicU64::new(0);

fn run_ir(ir: &str, wh: &[&str], args: &[&str]) -> (String, Vec<u8>) {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let tag = format!(
        "render-profile-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let ir_path = dir.join(format!("{tag}.opuir.json"));
    let ppm = dir.join(format!("{tag}.ppm"));
    fs::write(&ir_path, ir).unwrap();
    let output: Output = Command::new(env!("CARGO_BIN_EXE_opurom-render"))
        .arg(&ir_path)
        .args(wh)
        .arg(&ppm)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        fs::read(&ppm).unwrap(),
    )
}

fn run_counter(wh: &[&str], args: &[&str]) -> (String, Vec<u8>) {
    run_ir(&fs::read_to_string(COUNTER).unwrap(), wh, args)
}

fn digest(stdout: &str) -> &str {
    let first = stdout.lines().next().unwrap_or_default();
    assert!(first.starts_with("digest="), "{stdout}");
    &first["digest=".len()..]
}

#[test]
fn product_is_the_default_profile_and_compat_restores_all_three_goldens() {
    for (wh, state, pin) in [
        (vec!["320", "240"], vec![], GOLDEN_320),
        (vec!["800", "600"], vec![], GOLDEN_800),
        (vec!["800", "600"], vec!["--state", "count=7"], GOLDEN_800_7),
    ] {
        let wh: Vec<&str> = wh;
        let state: Vec<&str> = state;
        // Explicit compat keeps the historical bytes and stdout shape.
        let mut compat_args = vec!["--quality", "compat"];
        compat_args.extend(state.iter().copied());
        let (compat_out, compat_ppm) = run_counter(&wh, &compat_args);
        assert_eq!(compat_out, format!("digest={pin}\ndirty=1\n"));
        assert_eq!(digest(&compat_out), pin);
        // `--legacy-golden` is exactly the same profile.
        let mut legacy_args = vec!["--legacy-golden"];
        legacy_args.extend(state.iter().copied());
        let (legacy_out, legacy_ppm) = run_counter(&wh, &legacy_args);
        assert_eq!(legacy_out, compat_out);
        assert_eq!(legacy_ppm, compat_ppm);
        // The default product path changes the frame but keeps stdout at two
        // lines; it never silently emits compat pixels.
        let (product_out, product_ppm) = run_counter(&wh, &state);
        assert_eq!(product_out.lines().count(), 2, "{product_out}");
        assert!(product_out.starts_with("digest="));
        assert_ne!(digest(&product_out), pin, "default must not be compat");
        assert_ne!(product_ppm, compat_ppm);
    }
}

#[test]
fn compat_is_entered_only_explicitly() {
    // The bare spelling is product; an unknown name is a clean error.
    let (_, bare) = run_ir(TEXT_IR, &["320", "240"], &[]);
    let (_, named_product) = run_ir(TEXT_IR, &["320", "240"], &["--quality", "product"]);
    assert_eq!(bare, named_product, "product is already the default");
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ir_path = dir.join("profile-invalid.opuir.json");
    fs::write(&ir_path, TEXT_IR).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_opurom-render"))
        .arg(&ir_path)
        .args(["320", "240"])
        .arg(dir.join("profile-invalid.ppm"))
        .args(["--quality", "slow"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("invalid quality"));
}

#[test]
fn font_metrics_flag_explicitly_overrides_the_default() {
    let (product, _) = run_ir(TEXT_IR, &["320", "240"], &[]);
    let (fixed, _) = run_ir(
        TEXT_IR,
        &["320", "240"],
        &["--font-metrics", "fixed", "--no-edge-aa"],
    );
    let (proportional, _) = run_ir(
        TEXT_IR,
        &["320", "240"],
        &["--font-metrics", "proportional"],
    );
    assert_ne!(digest(&product), digest(&fixed), "fixed is a downgrade");
    assert_eq!(
        digest(&product),
        digest(&proportional),
        "product default is proportional"
    );
    // Compat pins fixed metrics with edge AA off, matching the explicit pair.
    let (compat, _) = run_ir(TEXT_IR, &["320", "240"], &["--quality", "compat"]);
    assert_eq!(digest(&compat), digest(&fixed));
}

#[test]
fn edge_aa_flag_and_no_edge_aa_override_each_other() {
    let (product, _) = run_ir(TEXT_IR, &["320", "240"], &[]);
    let (explicit_on, _) = run_ir(TEXT_IR, &["320", "240"], &["--edge-aa"]);
    assert_eq!(digest(&product), digest(&explicit_on));
    let (off, _) = run_ir(TEXT_IR, &["320", "240"], &["--no-edge-aa"]);
    assert_ne!(digest(&product), digest(&off), "edge AA is on by default");
    // The last explicit flag wins.
    let (back_on, _) = run_ir(TEXT_IR, &["320", "240"], &["--no-edge-aa", "--edge-aa"]);
    assert_eq!(digest(&product), digest(&back_on));
}

#[test]
fn no_motion_and_low_power_are_explicit_degradations() {
    let (product, _) = run_ir(TEXT_IR, &["320", "240"], &[]);
    let (no_motion, _) = run_ir(TEXT_IR, &["320", "240"], &["--no-motion"]);
    let (low_power, _) = run_ir(TEXT_IR, &["320", "240"], &["--low-power", "--no-edge-aa"]);
    // Settled frames keep their pixels with motion removed; a low-power frame
    // with edge AA off cannot equal the product frame.
    assert_eq!(
        digest(&product),
        digest(&no_motion),
        "settled frame keeps pixels"
    );
    assert_ne!(digest(&product), digest(&low_power));
}

#[test]
fn dpr_flag_renders_at_a_fixed_point_ratio() {
    let (one_x, _) = run_ir(TEXT_IR, &["320", "240"], &["--dpr", "1"]);
    let (product, _) = run_ir(TEXT_IR, &["320", "240"], &[]);
    assert_eq!(digest(&one_x), digest(&product), "dpr 1 is the default");
    let (two_x, _) = run_ir(TEXT_IR, &["320", "240"], &["--dpr", "2"]);
    assert_ne!(
        digest(&two_x),
        digest(&product),
        "dpr 2 refines edge coverage"
    );
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ir_path = dir.join("profile-bad-dpr.opuir.json");
    fs::write(&ir_path, TEXT_IR).unwrap();
    for bad in ["0", "5", "abc"] {
        let out = Command::new(env!("CARGO_BIN_EXE_opurom-render"))
            .arg(&ir_path)
            .args(["320", "240"])
            .arg(dir.join("profile-bad-dpr.ppm"))
            .args(["--dpr", bad])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(1), "{bad}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("invalid dpr"));
    }
}

#[test]
fn system_font_flag_suppresses_bitmap_text() {
    let (product, _) = run_ir(TEXT_IR, &["320", "240"], &[]);
    let (system, _) = run_ir(TEXT_IR, &["320", "240"], &["--system-font"]);
    assert_ne!(digest(&product), digest(&system));
}

#[test]
fn policy_data_defaults_match_the_cli_contract() {
    let p = RenderPolicy::product();
    assert_eq!(p.profile, RenderProfile::Product);
    assert!(p.edge_aa);
    assert_eq!(p.font_metrics, FontMetrics::Proportional);
    assert!(p.motion.enabled);
    assert_eq!(p.motion, CliMotionSpec::product());
    assert!(!p.low_power);
    let c = RenderPolicy::compat();
    assert_eq!(c.profile, RenderProfile::Compat);
    assert!(!c.edge_aa);
    assert_eq!(c.font_metrics, FontMetrics::Fixed);
}
