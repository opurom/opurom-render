//! Opt-in 4x geometric edge coverage. The default path is pinned by golden
//! frames, so the flag must leave it byte-identical while producing partial
//! coverage on rounded corners, discs and rings.

use opurom_render::{digest_hex, parse_ir, Renderer, StateView};
use std::{fs, path::PathBuf, process::Command};

/// One rounded box with a large radius on a known stage; corner pixels are the
/// only places where coverage can differ.
const IR: &str = r##"{
  "ir": "0.1.0", "app": "EdgeAA", "state": [],
  "tree": {
    "id": 0, "kind": "Column", "props": { "padding": 0 },
    "children": [
      { "id": 1, "kind": "Container", "props": {
          "width": 60, "height": 60, "color": "#112233", "radius": 20
        }, "children": [], "events": {} }
    ], "events": {}
  },
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 12 }
}"##;

fn fixture() -> (PathBuf, PathBuf) {
    // Concurrent test binaries share CARGO_TARGET_TMPDIR.
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ir = dir.join(format!("edgeaa-{}.opuir.json", std::process::id()));
    fs::write(&ir, IR).unwrap();
    (ir, dir.join(format!("edgeaa-{}.ppm", std::process::id())))
}

fn cli(args: &[&str]) -> std::process::Output {
    let (ir, ppm) = fixture();
    Command::new(env!("CARGO_BIN_EXE_opurom-render"))
        .arg(&ir)
        .args(["80", "80"])
        .arg(&ppm)
        .args(args)
        .output()
        .unwrap()
}

fn library(on: bool) -> Vec<u8> {
    let ir = parse_ir(IR).unwrap();
    let state = StateView::from_ir(&ir);
    let mut renderer = Renderer::new(80, 80);
    renderer.set_edge_aa(on);
    renderer.render(&ir, &state).pixels
}

#[test]
fn default_path_is_byte_identical_with_or_without_the_setter() {
    let plain = library(false);
    let ir = parse_ir(IR).unwrap();
    let state = StateView::from_ir(&ir);
    assert_eq!(
        digest_hex(&Renderer::new(80, 80).render(&ir, &state).pixels),
        digest_hex(&plain)
    );
    // The CLI with no flag emits no extra stdout line.
    let out = cli(&[]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!String::from_utf8_lossy(&out.stdout).contains("edge-aa"));
}

#[test]
fn edge_coverage_changes_only_corner_pixels() {
    let plain = library(false);
    let aa = library(true);
    assert_ne!(plain, aa, "the opt-in flag must change the frame");
    assert_eq!(plain.len(), aa.len());

    let mut differing = 0usize;
    let mut interior_different = 0usize;
    for (i, (a, b)) in plain.chunks_exact(4).zip(aa.chunks_exact(4)).enumerate() {
        let x = (i % 80) as i64;
        let y = (i / 80) as i64;
        assert_eq!(b[3], 255, "the frame stays opaque");
        if a != b {
            differing += 1;
            // The box spans 0..60; the interior 20..40 is far from any corner.
            if (20..40).contains(&x) && (20..40).contains(&y) {
                interior_different += 1;
            }
        }
    }
    assert!(differing > 0, "at least one corner pixel must blend");
    assert_eq!(interior_different, 0, "interior pixels must not change");
}

#[test]
fn edge_coverage_is_deterministic_across_processes_and_library_calls() {
    assert_eq!(library(true), library(true));
    let first = cli(&["--edge-aa"]);
    let second = cli(&["--edge-aa"]);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(first.stdout, second.stdout);
    let (_, ppm) = fixture();
    let bytes = fs::read(&ppm).unwrap();
    assert!(bytes.starts_with(b"P6\n80 80\n255\n"));
}

#[test]
fn edge_coverage_is_rejected_with_display_list_capture() {
    let (ir, ppm) = fixture();
    let out = Command::new(env!("CARGO_BIN_EXE_opurom-render"))
        .arg(&ir)
        .args(["80", "80"])
        .arg(&ppm)
        .args(["--edge-aa", "--commands", "/tmp/edgeaa-commands.json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("--edge-aa is unavailable with --commands")
    );
}

#[test]
fn capture_mode_ignores_the_flag() {
    // --commands alone keeps the display-list JSON byte-identical even when
    // the library renderer has coverage enabled: set_edge_aa is a no-op there.
    let ir = parse_ir(IR).unwrap();
    let state = StateView::from_ir(&ir);
    let mut renderer = Renderer::new(80, 80);
    renderer.set_edge_aa(true);
    let list = hex_of(renderer.display_list_at(&ir, &state, 0).to_json().unwrap());
    let plain = Renderer::new(80, 80);
    assert_eq!(
        list,
        hex_of(plain.display_list_at(&ir, &state, 0).to_json().unwrap())
    );
}

fn hex_of(s: String) -> String {
    // Cheap deterministic fingerprint of the JSON string.
    format!(
        "{}:{}",
        s.len(),
        s.bytes()
            .fold(0u64, |a, b| a.wrapping_mul(31).wrapping_add(b as u64))
    )
}
