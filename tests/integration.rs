use opurom_render::{
    digest_hex, layout, parse_ir, Breakpoint, Direction, Ir, LaidOut, Renderer, Rgba, StateValue,
    StateView,
};
use std::{fs, process::Command};

/// Golden IR v0.1 from opurom-spec/counter.opuir.json (embedded so this crate is independent).
const COUNTER_IR: &str = r##"{
  "ir": "0.1",
  "app": "Counter",
  "state": [
    { "name": "count", "type": "int", "init": 0 }
  ],
  "tree": {
    "id": 0,
    "kind": "Column",
    "props": { "gap": 8, "padding": 16, "align": "center" },
    "children": [
      {
        "id": 1,
        "kind": "Text",
        "props": { "value": "Count: {count}", "size": 24 },
        "children": [],
        "events": {}
      },
      {
        "id": 2,
        "kind": "Row",
        "props": { "gap": 8 },
        "children": [
          {
            "id": 3,
            "kind": "Button",
            "props": { "label": "-" },
            "children": [],
            "events": { "tap": { "op": "add", "target": "count", "value": -1 } }
          },
          {
            "id": 4,
            "kind": "Button",
            "props": { "label": "+" },
            "children": [],
            "events": { "tap": { "op": "add", "target": "count", "value": 1 } }
          }
        ],
        "events": {}
      }
    ],
    "events": {}
  },
  "theme": { "mode": "system", "accent": "#097FEB", "radius": 12 }
}"##;

fn fixture() -> (Ir, StateView) {
    let ir = parse_ir(COUNTER_IR).expect("golden IR must parse");
    let state = StateView::from_ir(&ir);
    (ir, state)
}

fn node<'a>(laid: &'a [LaidOut], id: u32) -> &'a LaidOut {
    laid.iter().find(|n| n.id == id).expect("node id missing")
}

/// Exercises all eight v0.1 kinds plus every layout prop in the task.
const KIT_IR: &str = r##"{
  "ir": "0.1", "app": "Kit",
  "state": [ { "name": "n", "type": "string", "init": "hi" } ],
  "tree": { "id": 0, "kind": "Column",
    "props": { "gap": 4, "padding": 10, "align": "center" },
    "children": [
      { "id": 1, "kind": "Container", "props": { "width": 120, "height": 40, "radius": 6, "color": "#AABBCC" }, "children": [], "events": {} },
      { "id": 2, "kind": "Text", "props": { "value": "Hi {n}", "size": 14, "color": "#112233" }, "children": [], "events": {} },
      { "id": 3, "kind": "Button", "props": { "label": "Go" }, "children": [], "events": {} },
      { "id": 4, "kind": "Input", "props": { "placeholder": "name", "bind": "n" }, "children": [], "events": {} },
      { "id": 5, "kind": "Image", "props": { "src": "a.png", "width": 32, "height": 24 }, "children": [], "events": {} },
      { "id": 6, "kind": "Spacer", "props": { "width": 16, "height": 12 }, "children": [], "events": {} },
      { "id": 7, "kind": "Row", "props": { "gap": 2, "padding": 3 }, "children": [
        { "id": 8, "kind": "Spacer", "props": { "width": 5, "height": 5 }, "children": [], "events": {} }
      ], "events": {} }
    ], "events": {} },
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 12 }
}"##;

#[test]
fn all_kinds_honour_gap_padding_align_size_radius_color_and_dimensions() {
    let ir = parse_ir(KIT_IR).unwrap();
    let state = StateView::from_ir(&ir);
    let laid = layout(&ir, &state, 1000, 800);
    assert_eq!(laid.len(), 9);
    assert_eq!(node(&laid, 1).rect.w, 120);
    assert_eq!(node(&laid, 1).rect.h, 40);
    assert_eq!(node(&laid, 1).radius, 6);
    assert_eq!(node(&laid, 1).fill, Rgba::from_hex("#AABBCC"));
    assert_eq!(node(&laid, 2).text.as_deref(), Some("Hi hi"));
    assert_eq!(node(&laid, 2).text_color, Rgba::from_hex("#112233"));
    assert_eq!(node(&laid, 4).text.as_deref(), Some("hi")); // bind shows current state
    assert_eq!((node(&laid, 5).rect.w, node(&laid, 5).rect.h), (32, 24));
    assert_eq!((node(&laid, 6).rect.w, node(&laid, 6).rect.h), (16, 12));
    assert_eq!((node(&laid, 8).rect.w, node(&laid, 8).rect.h), (5, 5));
    let pixels = Renderer::new(1000, 800).pixels(&ir, &state);
    assert_eq!(pixels.len(), 1000 * 800 * 4);
}

#[test]
fn golden_digest_is_stable() {
    let (ir, state) = fixture();
    let mut renderer = Renderer::new(800, 600);
    let frame = renderer.render(&ir, &state);
    assert_eq!((frame.width, frame.height), (800, 600));
    assert_eq!(frame.pixels.len(), 800 * 600 * 4);
    let view = frame.view();
    assert_eq!((view.width, view.height), (800, 600));
    assert_eq!(view.pixels, frame.pixels.as_slice());
    assert_eq!(digest_hex(&frame.pixels), "aaf572509f15d60c");
}

#[test]
fn two_fresh_renderers_at_two_viewports_are_deterministic() {
    let mut digests = Vec::new();
    for (w, h) in [(400u32, 700u32), (1000u32, 700u32)] {
        let (ir, state) = fixture();
        let mut a = Renderer::new(w, h);
        let mut b = Renderer::new(w, h);
        let fa = a.render(&ir, &state);
        let fb = b.render(&ir, &state);
        assert_eq!(fa.pixels, fb.pixels, "fresh renderers disagree at {w}x{h}");
        assert_eq!(fa.view().pixels, fb.view().pixels);
        assert_eq!(digest_hex(&fa.pixels), digest_hex(&a.pixels(&ir, &state)));
        digests.push(digest_hex(&fa.pixels));
    }
    assert_ne!(digests[0], digests[1], "viewport must affect fluid layout");
}

#[test]
fn breakpoints_match_contract() {
    assert_eq!(Breakpoint::from_width(599), Breakpoint::Compact);
    assert_eq!(Breakpoint::from_width(600), Breakpoint::Medium);
    assert_eq!(Breakpoint::from_width(899), Breakpoint::Medium);
    assert_eq!(Breakpoint::from_width(900), Breakpoint::Expanded);
}

#[test]
fn row_collapses_to_column_in_compact_width() {
    let (ir, state) = fixture();
    let compact = layout(&ir, &state, 400, 700);
    let expanded = layout(&ir, &state, 1000, 700);
    assert_eq!(node(&compact, 2).direction, Direction::Vertical);
    assert_eq!(node(&expanded, 2).direction, Direction::Horizontal);

    let (c3, c4) = (node(&compact, 3), node(&compact, 4));
    let (e3, e4) = (node(&expanded, 3), node(&expanded, 4));
    // Compact: buttons stack vertically (same x, different y).
    assert_eq!(c3.rect.x, c4.rect.x);
    assert_ne!(c3.rect.y, c4.rect.y);
    // Expanded: buttons sit side by side (same y, different x).
    assert_eq!(e3.rect.y, e4.rect.y);
    assert_ne!(e3.rect.x, e4.rect.x);
    assert_ne!(c3.rect, e3.rect, "layout must differ across breakpoints");
}

#[test]
fn interpolation_renders_different_pixels_after_state_change() {
    let (ir, mut state) = fixture();
    let mut renderer = Renderer::new(800, 600);
    let before = renderer.render(&ir, &state);
    assert_eq!(
        node(&renderer.compute_layout(&ir, &state), 1)
            .text
            .as_deref(),
        Some("Count: 0")
    );
    state.set("count", StateValue::Int(42));
    assert_eq!(
        node(&renderer.compute_layout(&ir, &state), 1)
            .text
            .as_deref(),
        Some("Count: 42")
    );
    let after = renderer.render(&ir, &state);
    assert_ne!(before.pixels, after.pixels);
    assert_ne!(digest_hex(&before.pixels), digest_hex(&after.pixels));
    assert!(!after.dirty.is_empty());
}

#[test]
fn unchanged_render_yields_zero_dirty_rects() {
    let (ir, state) = fixture();
    let mut renderer = Renderer::new(800, 600);
    let first = renderer.render(&ir, &state);
    assert_eq!(first.dirty.len(), 1, "first frame covers the whole surface");
    let second = renderer.render(&ir, &state);
    assert!(second.dirty.is_empty(), "unchanged state must be a no-op");
    assert_eq!(first.pixels, second.pixels);
    assert!(renderer.render(&ir, &state).dirty.is_empty());
}

#[test]
fn changed_state_yields_bounded_covering_dirty_rects() {
    let (ir, mut state) = fixture();
    let mut renderer = Renderer::new(800, 600);
    let before = renderer.render(&ir, &state);
    state.set("count", StateValue::Int(12345));
    let after = renderer.render(&ir, &state);
    assert!(!after.dirty.is_empty());
    assert!(after.dirty.len() <= 64, "dirty rect list must stay bounded");
    for r in &after.dirty {
        assert!(r.w > 0 && r.h > 0);
        assert!(r.x + r.w <= 800 && r.y + r.h <= 600);
    }
    for y in 0..600u32 {
        for x in 0..800u32 {
            let i = (y as usize * 800 + x as usize) * 4;
            if before.pixels[i..i + 4] != after.pixels[i..i + 4] {
                assert!(
                    after
                        .dirty
                        .iter()
                        .any(|r| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h),
                    "changed pixel ({x},{y}) not covered by {:?}",
                    after.dirty
                );
            }
        }
    }
}

#[test]
fn theme_mode_and_accent_change_pixels() {
    let (ir, state) = fixture();
    let renderer = Renderer::new(320, 240);
    let base = renderer.pixels(&ir, &state);
    let dark = parse_ir(&COUNTER_IR.replace("\"mode\": \"system\"", "\"mode\": \"dark\"")).unwrap();
    let accent = parse_ir(&COUNTER_IR.replace("#097FEB", "#E5464F")).unwrap();
    assert_ne!(base, renderer.pixels(&dark, &state));
    assert_ne!(base, renderer.pixels(&accent, &state));
    assert_eq!(base, renderer.pixels(&ir, &state));
}

#[test]
fn malformed_ir_is_rejected_with_readable_diagnostics() {
    let cases: Vec<(String, &str)> = vec![
        ("not json at all".into(), "invalid IR JSON"),
        (
            COUNTER_IR.replace("\"0.1\"", "\"0.2\""),
            "unsupported IR version",
        ),
        (
            COUNTER_IR.replace("\"id\": 3", "\"id\": 9"),
            "pre-order",
        ),
        (COUNTER_IR.replace("#097FEB", "#GGGGGG"), "theme.accent"),
        (
            COUNTER_IR.replace("\"init\": 0", "\"init\": true"),
            "type does not match",
        ),
        (
            COUNTER_IR.replace("\"op\": \"add\"", "\"op\": \"multiply\""),
            "unknown value",
        ),
        (
            COUNTER_IR.replace("\"align\": \"center\"", "\"align\": \"sideways\""),
            "unknown value",
        ),
        (
            COUNTER_IR.replace("\"value\": \"Count: {count}\"", "\"value\": {\"a\": 1}"),
            "must be a string or a number",
        ),
        (
            r##"{"ir":"0.1","app":"X","state":[],"theme":{"mode":"light","accent":"#000000","radius":0}}"##.into(),
            "missing required field",
        ),
    ];
    for (i, (text, expected)) in cases.iter().enumerate() {
        let err = parse_ir(text).expect_err(&format!("case {i} was accepted"));
        let msg = err.to_string();
        assert!(!msg.is_empty(), "case {i} had an empty diagnostic");
        assert!(
            msg.contains(expected),
            "case {i} diagnostic {msg:?} does not mention {expected:?}"
        );
    }
}

#[test]
fn cli_writes_ppm_and_prints_stable_digest() {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ir_path = dir.join("counter.opuir.json");
    fs::write(&ir_path, COUNTER_IR).unwrap();
    let exe = env!("CARGO_BIN_EXE_opurom-render");
    let run = |name: &str| {
        Command::new(exe)
            .arg(&ir_path)
            .args(["800", "600"])
            .arg(dir.join(name))
            .args(["--state", "count=7"])
            .output()
            .unwrap()
    };
    let (o1, o2) = (run("cli-1.ppm"), run("cli-2.ppm"));
    assert!(
        o1.status.success(),
        "{}",
        String::from_utf8_lossy(&o1.stderr)
    );
    assert!(
        o2.status.success(),
        "{}",
        String::from_utf8_lossy(&o2.stderr)
    );
    let (s1, s2) = (
        String::from_utf8_lossy(&o1.stdout),
        String::from_utf8_lossy(&o2.stdout),
    );
    assert_eq!(s1, s2, "digest must be stable across processes");
    assert!(s1
        .lines()
        .any(|l| l.starts_with("digest=") && l.len() == 7 + 16));
    assert!(s1.lines().any(|l| l == "dirty=1"));
    let (p1, p2) = (
        fs::read(dir.join("cli-1.ppm")).unwrap(),
        fs::read(dir.join("cli-2.ppm")).unwrap(),
    );
    assert_eq!(p1, p2);
    assert!(p1.starts_with(b"P6\n800 600\n255\n"));
    assert_eq!(p1.len(), "P6\n800 600\n255\n".len() + 800 * 600 * 3);

    // Repeated --state is accepted and changes the digest deterministically.
    let o3 = Command::new(exe)
        .arg(&ir_path)
        .args(["800", "600"])
        .arg(dir.join("cli-3.ppm"))
        .args(["--state", "count=9", "--state", "count=7"])
        .output()
        .unwrap();
    assert!(o3.status.success());
    assert_eq!(String::from_utf8_lossy(&o3.stdout), s1);

    // Malformed IR is a diagnostic with exit code 1, never a panic.
    let bad_path = dir.join("bad.opuir.json");
    fs::write(&bad_path, "not json").unwrap();
    let bad = Command::new(exe)
        .arg(&bad_path)
        .args(["64", "48", dir.join("bad.ppm").to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(bad.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&bad.stderr).contains("error:"));
    assert!(!String::from_utf8_lossy(&bad.stderr).contains("panicked"));
}
