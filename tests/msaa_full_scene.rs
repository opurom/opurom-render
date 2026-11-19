//! Full-scene opt-in 4x edge coverage: rounded boxes, discs, rings and straight
//! strokes all take the same fixed 4x sub-sample path; capture stays unchanged.

use opurom_render::{digest_hex, parse_ir, raster::Canvas, Rect, Renderer, Rgba, StateView};
use serde_json::json;
use std::{fs, path::PathBuf, process::Command};

fn scene(on: bool) -> Vec<u8> {
    let mut c = Canvas::new(32, 32, Rgba::rgb(0, 0, 0));
    if on {
        c.set_edge_aa(true);
    }
    c.fill_rounded(
        Rect {
            x: 1,
            y: 1,
            w: 12,
            h: 12,
        },
        Rgba::rgb(255, 0, 0),
        4,
    );
    c.fill_circle(
        Rect {
            x: 16,
            y: 1,
            w: 12,
            h: 12,
        },
        Rgba::rgb(0, 255, 0),
    );
    c.stroke_circle(
        Rect {
            x: 1,
            y: 16,
            w: 12,
            h: 12,
        },
        Rgba::rgb(0, 0, 255),
        2,
    );
    c.draw_check(
        Rect {
            x: 16,
            y: 16,
            w: 12,
            h: 12,
        },
        Rgba::rgb(255, 255, 255),
        1,
    );
    c.pixels
}

fn count_partial(pixels: &[u8]) -> usize {
    // The scene uses only opaque black/red/green/blue/white; any other colour
    // is a blended edge pixel produced by coverage.
    let pure: &[[u8; 3]] = &[
        [0, 0, 0],
        [255, 0, 0],
        [0, 255, 0],
        [0, 0, 255],
        [255, 255, 255],
    ];
    pixels
        .chunks_exact(4)
        .filter(|p| !pure.contains(&[p[0], p[1], p[2]]) && p[3] == 255)
        .count()
}

fn scene_ir(quality: Option<serde_json::Value>) -> String {
    let mut doc = json!({
        "ir": "0.1.0", "app": "MSAA", "state": [{ "name": "on", "type": "bool", "init": true }],
        "tree": { "id": 0, "kind": "Column", "props": { "padding": 2 },
            "children": [ { "id": 1, "kind": "Checkbox", "props": { "bind": "on" },
                "children": [], "events": {} } ], "events": {} },
        "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 }
    });
    if let Some(q) = quality {
        doc["renderQuality"] = q;
    }
    doc.to_string()
}

#[test]
fn full_scene_edge_pixels_match_the_fixed_coverage_oracle() {
    let aa = scene(true);
    // Byte-for-byte oracle for the whole four-shape scene. The sub-sample
    // coverage values themselves are checked against an independent analytic
    // point-to-segment reference in raster.rs unit tests.
    // All geometric edges now share the analytic convex-path coverage:
    // corners, straight/thin lines and large arcs use Skia AAA area masks.
    assert_eq!(digest_hex(&aa), "2ac2564791d504b6");
    assert!(
        count_partial(&aa) >= 20,
        "edge coverage must produce partial pixels"
    );
    assert_eq!(aa, scene(true), "the opt-in frame is byte-deterministic");
    assert!(
        aa.chunks_exact(4).all(|p| p[3] == 255),
        "frame stays opaque"
    );
}

#[test]
fn msaa_flag_changes_pixels_but_not_the_capture_stream() {
    let plain = scene(false);
    let aa = scene(true);
    assert_ne!(
        digest_hex(&plain),
        digest_hex(&aa),
        "the opt-in flag must matter"
    );
    assert_eq!(
        count_partial(&plain),
        0,
        "default edges are hard integer edges"
    );

    let src = parse_ir(&scene_ir(None)).unwrap();
    let qsrc = parse_ir(&scene_ir(Some(json!({ "msaa": 4 })))).unwrap();
    let state = StateView::from_ir(&src);
    let qstate = StateView::from_ir(&qsrc);
    assert_ne!(
        Renderer::new(48, 48).pixels(&src, &state),
        Renderer::new(48, 48).pixels(&qsrc, &qstate),
        "renderQuality.msaa must change the raster frame"
    );
    assert_eq!(
        Renderer::new(48, 48)
            .display_list(&src, &state)
            .to_json()
            .unwrap(),
        Renderer::new(48, 48)
            .display_list(&qsrc, &qstate)
            .to_json()
            .unwrap(),
        "capture mode records the same command stream"
    );
}

#[test]
fn default_path_bytes_and_pinned_digests_stay_identical() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let counter = fs::read_to_string("../opurom-examples/golden/counter.opuir.json").unwrap();
    let ir = parse_ir(&counter).unwrap();
    let state = StateView::from_ir(&ir);
    assert_eq!(
        digest_hex(&Renderer::new(320, 240).render(&ir, &state).pixels),
        "8d45f412996a99c6"
    );
    assert_eq!(
        digest_hex(&Renderer::new(800, 600).render(&ir, &state).pixels),
        "318beb1dd337bc4e"
    );

    let ppm = dir.join("msaa-effects.ppm");
    let out = Command::new(env!("CARGO_BIN_EXE_opurom-render"))
        .arg("tests/fixtures/effects.opuir.json")
        .args(["96", "160"])
        .arg(&ppm)
        .args(["--quality", "compat"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        format!("digest={}", digest_hex(&fs::read(&ppm).unwrap())),
        fs::read_to_string("tests/fixtures/effects.digest.txt")
            .unwrap()
            .trim()
    );
}
