//! The optional top-level `transition` IR key configures the page-transition
//! engine. The CLI flag still wins; with no flag the IR key selects the kind.
//! The no-transition default path is byte-for-byte unchanged.

use opurom_render::{parse_ir, TransitionKind};
use std::{fs, path::PathBuf, process::Command};

const TREE: &str = r##"{
  "id": 0, "kind": "Column", "props": { "gap": 4 },
  "children": [
    { "id": 1, "kind": "Container", "props": { "width": 60, "height": 40, "animate": "spring" },
      "children": [], "events": {} }
  ], "events": {}
}"##;

fn ir_with_transition(transition: Option<&str>) -> String {
    let key = transition
        .map(|t| format!(r#", "transition": {t:?}"#))
        .unwrap_or_default();
    format!(
        r##"{{"ir":"0.1","app":"Trans","state":[],"tree":{TREE}{key},"theme":{{"mode":"light","accent":"#097FEB","radius":4}}}}"##
    )
}

fn run(ir: &PathBuf, ppm: &PathBuf, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_opurom-render"))
        .arg(ir)
        .args(["320", "240"])
        .arg(ppm)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn parse_ir_reads_each_transition_kind() {
    for (value, expected) in [
        ("slide", TransitionKind::Slide),
        ("fade", TransitionKind::Fade),
        ("scale", TransitionKind::Scale),
    ] {
        let ir = parse_ir(&ir_with_transition(Some(value))).unwrap();
        assert_eq!(ir.transition, Some(expected), "value {value}");
    }
}

#[test]
fn parse_ir_without_a_transition_key_is_none() {
    let ir = parse_ir(&ir_with_transition(None)).unwrap();
    assert_eq!(ir.transition, None);
}

#[test]
fn parse_ir_rejects_an_unknown_transition() {
    let error = parse_ir(&ir_with_transition(Some("warp"))).expect_err("warp is not a kind");
    let text = error.to_string();
    assert!(text.contains("transition"), "{text}");
    for kind in ["slide", "fade", "scale"] {
        assert!(text.contains(kind), "{text} must name {kind}");
    }
}

#[test]
fn parse_ir_rejects_a_non_string_transition() {
    let text = ir_with_transition(None).replacen(",\"theme\"", ",\"transition\": 5,\"theme\"", 1);
    let error = parse_ir(&text).expect_err("a number is not a transition");
    assert!(error.to_string().contains("must be a string"), "{error}");
}

#[test]
fn cli_ir_transition_is_honoured_when_the_flag_is_absent() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ir_path = dir.join("ir-transition.opuir.json");
    fs::write(&ir_path, ir_with_transition(Some("fade"))).unwrap();
    let ppm = dir.join("ir-transition.ppm");

    let from_ir = run(&ir_path, &ppm, &["--tick", "16"]);
    assert!(
        from_ir.status.success(),
        "{}",
        String::from_utf8_lossy(&from_ir.stderr)
    );
    let from_ir_stdout = String::from_utf8_lossy(&from_ir.stdout).to_string();

    let from_flag = run(&ir_path, &ppm, &["--tick", "16", "--transition", "fade"]);
    assert!(
        from_flag.status.success(),
        "{}",
        String::from_utf8_lossy(&from_flag.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&from_flag.stdout).to_string(),
        from_ir_stdout,
        "the IR key must select the same frame as the flag"
    );
    assert_eq!(fs::read(&ppm).unwrap().len(), 320 * 240 * 3 + 15);
}

#[test]
fn cli_flag_still_wins_over_the_ir_key() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ir_path = dir.join("ir-precedence.opuir.json");
    fs::write(&ir_path, ir_with_transition(Some("slide"))).unwrap();
    let ppm = dir.join("ir-precedence.ppm");

    let slide = run(&ir_path, &ppm, &["--tick", "16"]);
    let fade = run(&ir_path, &ppm, &["--tick", "16", "--transition", "fade"]);
    assert_ne!(
        String::from_utf8_lossy(&slide.stdout),
        String::from_utf8_lossy(&fade.stdout),
        "slide and fade must differ"
    );
    assert_eq!(
        String::from_utf8_lossy(&fade.stdout),
        String::from_utf8_lossy(
            &run(&ir_path, &ppm, &["--tick", "16", "--transition", "fade"]).stdout
        ),
        "the explicit flag must override the IR key"
    );
}

/// The sacred no-transition default: the settled counter frame, unchanged.
#[test]
fn no_transition_default_keeps_the_pinned_digest() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let counter = dir.join("counter.opuir.json");
    fs::write(
        &counter,
        fs::read_to_string("../opurom-spec/counter.opuir.json").unwrap(),
    )
    .unwrap();
    let ppm = dir.join("counter.ppm");
    let out = run(&counter, &ppm, &[]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "digest=16b69e96b1534324\ndirty=1\n",
        "the default path must stay byte-for-byte identical"
    );
}

#[test]
fn no_transition_ir_default_matches_the_library_settled_frame() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ir_path = dir.join("ir-settled.opuir.json");
    fs::write(&ir_path, ir_with_transition(None)).unwrap();
    let ppm = dir.join("ir-settled.ppm");
    let out = run(&ir_path, &ppm, &[]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "digest=272e9ffbc0672fe4\ndirty=1\n",
        "a no-transition IR renders exactly the settled frame"
    );
}
