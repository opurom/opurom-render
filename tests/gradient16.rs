//! Opt-in 16-bit smooth gradient: 16-bit intermediate plus deterministic
//! ordered dithering must break the RGBA8 banding of the 10-bit default path.

use opurom_render::{digest_hex, parse_ir, raster::Canvas, Rect, Renderer, Rgba, StateView};
use serde_json::json;
use std::{fs, path::PathBuf, process::Command};

/// Documented no-banding limit: on the pinned 256-sample ramp, no run of
/// identical RGBA8 colours may exceed this many adjacent pixels.
const MAX_RAMP_BAND: usize = 12;

/// 256 samples sweeping 0..15; the narrow range makes the 10-bit default path
/// band at 17 adjacent pixels, so the assertion has something real to disprove.
fn ramp(bits: u8) -> Vec<u8> {
    let mut c = Canvas::new(256, 4, Rgba::rgb(0, 0, 0));
    c.set_gradient_bits(bits);
    c.fill_rounded_gradient(
        Rect {
            x: 0,
            y: 0,
            w: 256,
            h: 4,
        },
        Rgba::rgb(0, 0, 0),
        Rgba::rgb(15, 15, 15),
        0,
        1024,
        true,
    );
    c.pixels
}

fn max_band(pixels: &[u8]) -> usize {
    let mut longest = 0usize;
    let mut current = 0usize;
    let mut prev: Option<[u8; 3]> = None;
    for p in pixels.chunks_exact(4) {
        let rgb = [p[0], p[1], p[2]];
        if prev == Some(rgb) {
            current += 1;
        } else {
            current = 1;
            prev = Some(rgb);
        }
        longest = longest.max(current);
    }
    longest
}

fn gradient_ir(quality: Option<serde_json::Value>) -> String {
    let mut doc = json!({
        "ir": "0.1.0", "app": "GRAD", "state": [],
        "tree": { "id": 0, "kind": "Column", "props": { "padding": 0 },
            "children": [ { "id": 1, "kind": "Container", "props": {
                "width": 60, "height": 60, "radius": 0,
                "gradient": "#000000,#0F0F0F"
            }, "children": [], "events": {} } ], "events": {} },
        "theme": { "mode": "light", "accent": "#097FEB", "radius": 0 }
    });
    if let Some(q) = quality {
        doc["renderQuality"] = q;
    }
    doc.to_string()
}

#[test]
fn dithered_ramp_breaks_bands_and_matches_pinned_bytes() {
    let px = ramp(16);
    let band = max_band(&px);
    assert!(
        band <= MAX_RAMP_BAND,
        "16-bit ramp band {band} exceeds documented limit {MAX_RAMP_BAND}"
    );
    assert_eq!(digest_hex(&px), "d106eeb719f0235b");
    assert_eq!(px, ramp(16), "the dithered frame is byte-deterministic");
}

#[test]
fn ten_bit_default_bands_wider_and_flag_changes_pixels() {
    let plain = ramp(10);
    let dithered = ramp(16);
    let plain_band = max_band(&plain);
    let dithered_band = max_band(&dithered);
    assert!(
        plain_band > dithered_band,
        "flag must matter: default band {plain_band} vs dithered {dithered_band}"
    );
    assert_ne!(digest_hex(&plain), digest_hex(&dithered));

    let src = parse_ir(&gradient_ir(None)).unwrap();
    let qsrc = parse_ir(&gradient_ir(Some(json!({ "gradientBits": 16 })))).unwrap();
    let state = StateView::from_ir(&src);
    let qstate = StateView::from_ir(&qsrc);
    assert_eq!(src.render_quality, None);
    assert_eq!(
        qsrc.render_quality.as_ref().unwrap().gradient_bits,
        Some(16)
    );
    let a = Renderer::new(96, 96).pixels(&src, &state);
    let b = Renderer::new(96, 96).pixels(&qsrc, &qstate);
    assert_ne!(
        digest_hex(&a),
        digest_hex(&b),
        "renderQuality.gradientBits must change the raster frame"
    );
    assert_eq!(
        Renderer::new(96, 96)
            .display_list(&src, &state)
            .to_json()
            .unwrap(),
        Renderer::new(96, 96)
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

    let ppm = dir.join("gradient-effects.ppm");
    let out = Command::new(env!("CARGO_BIN_EXE_opurom-render"))
        .arg("tests/fixtures/effects.opuir.json")
        .args(["96", "160"])
        .arg(&ppm)
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
