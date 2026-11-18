//! Display-list serialization, clipping, draw order and CLI regression tests.

use opurom_render::{
    parse_ir, raster::Canvas, Command as RasterCommand, DisplayList, Ir, Rect, Renderer, Rgba,
    StateView,
};
use serde_json::Value;
use std::{fs, path::PathBuf, process::Command as ProcessCommand};

fn compat_renderer(width: u32, height: u32) -> opurom_render::Renderer {
    let mut renderer = opurom_render::Renderer::new(width, height);
    renderer.set_quality(opurom_render::QualityProfile::Compat);
    renderer
}

const RECT_GRADIENT_IR: &str = include_str!("fixtures/rect-gradient.opuir.json");

fn fixture() -> (Ir, StateView) {
    let ir = parse_ir(RECT_GRADIENT_IR).unwrap();
    let state = StateView::from_ir(&ir);
    (ir, state)
}

#[test]
fn command_json_is_deterministic_and_records_all_command_kinds() {
    let (ir, state) = fixture();
    let first = compat_renderer(96, 64)
        .display_list(&ir, &state)
        .to_json()
        .unwrap();
    let second = compat_renderer(96, 64)
        .display_list(&ir, &state)
        .to_json()
        .unwrap();
    assert_eq!(first, second, "identical frames must serialize identically");

    let doc: Value = serde_json::from_str(&first).unwrap();
    assert_eq!(doc["version"].as_i64(), Some(1));
    assert_eq!(doc["width"].as_i64(), Some(96));
    assert_eq!(doc["height"].as_i64(), Some(64));
    assert_eq!(doc["background"][0].as_i64(), Some(247));
    let commands = doc["commands"].as_array().unwrap();
    let kinds: Vec<&str> = commands.iter().filter_map(|c| c["kind"].as_str()).collect();
    for expected in ["solid", "rounded", "gradient"] {
        assert!(kinds.contains(&expected), "missing command kind {expected}");
    }

    let gradient = commands
        .iter()
        .find(|c| c["kind"].as_str() == Some("gradient"))
        .unwrap();
    assert_eq!(gradient["top"][0].as_i64(), Some(16));
    assert_eq!(gradient["bottom"][0].as_i64(), Some(240));
    assert_eq!(gradient["alpha"].as_i64(), Some(255));

    let has_negative_bound = commands.iter().any(|c| {
        let r = &c["rect"];
        r[0].as_i64().unwrap_or(0) < 0 || r[1].as_i64().unwrap_or(0) < 0
    });
    let has_overflowing_bound = commands.iter().any(|c| {
        let r = &c["rect"];
        r[1].as_i64().unwrap_or(0) + r[3].as_i64().unwrap_or(0) > 64
    });
    assert!(has_negative_bound, "fixture must exercise negative bounds");
    assert!(
        has_overflowing_bound,
        "fixture must exercise viewport clipping"
    );
}

#[test]
fn gradient_commands_keep_stop_and_draw_order() {
    let rect = Rect {
        x: 0,
        y: 0,
        w: 2,
        h: 2,
    };
    let list = DisplayList {
        unsupported: None,
        width: 4,
        height: 2,
        background: Rgba::rgb(0, 0, 0),
        quality: opurom_render::QualityProfile::Compat,
        commands: vec![
            RasterCommand::Gradient {
                rect,
                clip: rect,
                radius: 0,
                top: Rgba::rgb(1, 2, 3),
                bottom: Rgba::rgb(4, 5, 6),
                alpha: 255,
            },
            RasterCommand::Gradient {
                rect,
                clip: rect,
                radius: 0,
                top: Rgba::rgb(7, 8, 9),
                bottom: Rgba::rgb(10, 11, 12),
                alpha: 128,
            },
            RasterCommand::Solid {
                rect,
                clip: rect,
                color: Rgba::rgb(20, 30, 40),
            },
        ],
    };
    let doc: Value = serde_json::from_str(&list.to_json().unwrap()).unwrap();
    let commands = doc["commands"].as_array().unwrap();
    assert_eq!(commands.len(), 3);
    assert_eq!(commands[0]["kind"].as_str(), Some("gradient"));
    assert_eq!(commands[0]["top"][0].as_i64(), Some(1));
    assert_eq!(commands[0]["bottom"][2].as_i64(), Some(6));
    assert_eq!(commands[0]["alpha"].as_i64(), Some(255));
    assert_eq!(commands[1]["kind"].as_str(), Some("gradient"));
    assert_eq!(commands[1]["top"][0].as_i64(), Some(7));
    assert_eq!(commands[1]["bottom"][2].as_i64(), Some(12));
    assert_eq!(commands[1]["top"][3].as_i64(), Some(128));
    assert_eq!(commands[2]["kind"].as_str(), Some("solid"));
}

#[test]
fn empty_display_list_serializes_as_empty_commands() {
    let list = DisplayList {
        unsupported: None,
        width: 2,
        height: 1,
        background: Rgba::rgb(7, 8, 9),
        quality: opurom_render::QualityProfile::Compat,
        commands: Vec::new(),
    };
    let json = list.to_json().unwrap();
    let doc: Value = serde_json::from_str(&json).unwrap();
    assert!(doc["commands"].as_array().unwrap().is_empty());
    assert!(json.contains("\"commands\":[\n]}\n"));
}

#[test]
fn cli_commands_dump_keeps_golden_stdout_and_ppm() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let plain_ppm = dir.join("capability-counter-plain.ppm");
    let commands_ppm = dir.join("capability-counter-commands.ppm");
    let commands_json = dir.join("capability-counter.commands.json");
    let commands_arg = commands_json.to_str().unwrap();
    let exe = env!("CARGO_BIN_EXE_opurom-render");
    let run = |ppm: &PathBuf, extra: &[&str]| {
        ProcessCommand::new(exe)
            .arg("../opurom-examples/golden/counter.opuir.json")
            .args(["320", "240"])
            .arg(ppm)
            .args(extra)
            .output()
            .unwrap()
    };

    let plain = run(&plain_ppm, &["--quality", "compat"]);
    let with_commands = run(
        &commands_ppm,
        &["--commands", commands_arg, "--quality", "compat"],
    );
    assert!(
        plain.status.success(),
        "{}",
        String::from_utf8_lossy(&plain.stderr)
    );
    assert!(
        with_commands.status.success(),
        "{}",
        String::from_utf8_lossy(&with_commands.stderr)
    );
    assert_eq!(plain.stdout, with_commands.stdout);
    let stdout = String::from_utf8_lossy(&with_commands.stdout);
    assert_eq!(stdout.lines().count(), 2);
    assert!(stdout.contains("digest=8d45f412996a99c6"));
    assert!(stdout.contains("dirty=1"));
    assert_eq!(
        fs::read(&plain_ppm).unwrap(),
        fs::read(&commands_ppm).unwrap()
    );

    let doc: Value = serde_json::from_slice(&fs::read(&commands_json).unwrap()).unwrap();
    let commands = doc["commands"].as_array().unwrap();
    assert!(!commands.is_empty());
    assert!(commands.iter().any(|c| c["kind"].as_str() == Some("solid")));
    assert!(commands
        .iter()
        .any(|c| c["kind"].as_str() == Some("rounded")));
}

#[test]
fn cli_commands_argument_errors_are_readable() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ppm = dir.join("capability-error.ppm");
    let commands_json = dir.join("capability-error.commands.json");
    let commands_arg = commands_json.to_str().unwrap();
    let exe = env!("CARGO_BIN_EXE_opurom-render");
    let base = || {
        let mut command = ProcessCommand::new(exe);
        command
            .arg("../opurom-examples/golden/counter.opuir.json")
            .args(["320", "240"])
            .arg(&ppm);
        command
    };

    let missing = base().arg("--commands").output().unwrap();
    assert_eq!(missing.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&missing.stderr).contains("--commands requires"),
        "unexpected diagnostic: {}",
        String::from_utf8_lossy(&missing.stderr)
    );

    let transition = base()
        .args(["--commands", commands_arg, "--transition", "slide"])
        .args(["--quality", "compat"])
        .output()
        .unwrap();
    assert_eq!(transition.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&transition.stderr).contains("unavailable with --transition"),
        "unexpected diagnostic: {}",
        String::from_utf8_lossy(&transition.stderr)
    );
}

#[test]
fn software_bounds_clipping_matches_recorded_command() {
    let mut canvas = Canvas::capturing(4, 2, Rgba::rgb(0, 0, 0));
    let rect = Rect {
        x: -2,
        y: -1,
        w: 5,
        h: 3,
    };
    canvas.fill_rect(rect, Rgba::rgb(200, 100, 50));
    let pixels = canvas.pixels.clone();
    let commands = canvas.into_commands();
    assert_eq!(commands.len(), 1);
    match commands[0] {
        RasterCommand::Solid { rect: actual, .. } => assert_eq!(actual, rect),
        _ => panic!("fill_rect must record one solid command"),
    }

    let list = DisplayList {
        unsupported: None,
        width: 4,
        height: 2,
        background: Rgba::rgb(0, 0, 0),
        quality: opurom_render::QualityProfile::Compat,
        commands,
    };
    let doc: Value = serde_json::from_str(&list.to_json().unwrap()).unwrap();
    assert_eq!(doc["commands"][0]["rect"][0].as_i64(), Some(-2));
    assert_eq!(doc["commands"][0]["rect"][1].as_i64(), Some(-1));

    let mut expected: Vec<u8> = Vec::new();
    for _ in 0..2 {
        for x in 0..4 {
            expected.extend_from_slice(if x < 3 {
                &[200, 100, 50, 255]
            } else {
                &[0, 0, 0, 255]
            });
        }
    }
    assert_eq!(pixels, expected);
}

#[test]
fn zero_and_oversized_display_views_fail_closed() {
    let list = |w: u32, h: u32| DisplayList {
        unsupported: None,
        width: w,
        height: h,
        background: Rgba::rgb(0, 0, 0),
        quality: opurom_render::QualityProfile::Compat,
        commands: Vec::new(),
    };
    for (w, h) in [(0, 10), (10, 0), (0, 0), (4097, 10)] {
        assert!(
            list(w, h).to_json().is_err(),
            "invalid viewport {w}x{h} was serialized"
        );
    }
    assert!(list(4096, 4096).to_json().is_ok());
}

#[test]
fn command_count_above_the_wire_limit_is_rejected() {
    let rect = Rect {
        x: 0,
        y: 0,
        w: 1,
        h: 1,
    };
    let list = DisplayList {
        unsupported: None,
        width: 2,
        height: 2,
        background: Rgba::rgb(0, 0, 0),
        quality: opurom_render::QualityProfile::Compat,
        commands: vec![
            RasterCommand::Solid {
                rect,
                clip: rect,
                color: Rgba::rgb(1, 2, 3),
            };
            opurom_render::display::MAX_COMMANDS + 1
        ],
    };
    let err = list.to_json().unwrap_err().to_string();
    assert!(err.contains("too many commands"), "{err}");
}

#[test]
fn extreme_theme_tokens_are_rejected_with_typed_diagnostics() {
    let doc = |theme: serde_json::Value| {
        format!(
            r#"{{"ir":"0.1.0","app":"Stress","state":[],"tree":{{"id":0,"kind":"Column","props":{{"padding":0}},"children":[],"events":{{}}}},"theme":{theme}}}"#
        )
    };
    for theme in [
        serde_json::json!({"mode":"light","accent":"#097FEB","radius":-1}),
        serde_json::json!({"mode":"dark","accent":"not-a-color","radius":4}),
        serde_json::json!({"mode":"solarized","accent":"#097FEB","radius":4}),
    ] {
        assert!(
            parse_ir(&doc(theme.clone())).is_err(),
            "extreme theme was accepted: {theme}"
        );
    }
    // A very large but non-negative radius is a legal authored value; layout
    // clamps it to the viewport before the display-list wire boundary.
    let legal = parse_ir(&doc(
        serde_json::json!({"mode":"dark","accent":"#097FEB","radius":1_000_000}),
    ))
    .unwrap();
    Renderer::new(32, 32)
        .display_list(&legal, &StateView::from_ir(&legal))
        .to_json()
        .unwrap();
}

#[test]
fn quality_profile_is_frozen_in_the_renderer_state() {
    let (ir, state) = fixture();
    let compat = compat_renderer(96, 64)
        .display_list(&ir, &state)
        .to_json()
        .unwrap();
    assert!(!compat.contains("\"quality\""), "{compat}");

    let mut renderer = compat_renderer(96, 64);
    renderer.set_quality(opurom_render::QualityProfile::Product);
    let product = renderer.display_list(&ir, &state).to_json().unwrap();
    assert_eq!(
        compat, product,
        "product visual differences are raster-side"
    );
    assert_eq!(renderer.quality(), opurom_render::QualityProfile::Product);
    assert!(renderer.quality().edge_coverage());

    let mut probe = compat_renderer(96, 64);
    probe.set_quality(opurom_render::QualityProfile::Compat);
    assert_eq!(probe.quality(), opurom_render::QualityProfile::Compat);
}
