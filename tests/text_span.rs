//! System-font display layer (SYSTEM_FONT_DISPLAY.md §2): opt-in text
//! suppression and deterministic text-span export. Default render unchanged.

use opurom_render::{
    digest, digest_hex, layout, parse_ir, text_spans, text_spans_for, text_spans_json, Renderer,
    StateView,
};

/// Mixed fixture: three text nodes with start/center/end alignment, a filled
/// rounded container (non-text graphic), an icon, and a plain shape.
const MIXED: &str = r##"{
  "ir": "0.1.0", "app": "Spans", "state": [],
  "tree": { "id": 0, "kind": "Column", "props": { "gap": 2, "padding": 4 },
    "children": [
      { "id": 1, "kind": "Text", "props": { "value": "left", "align": "start", "size": 20 }, "children": [], "events": {} },
      { "id": 2, "kind": "Text", "props": { "value": "mid", "align": "center", "size": 24 }, "children": [], "events": {} },
      { "id": 3, "kind": "Text", "props": { "value": "right", "align": "end", "size": 16 }, "children": [], "events": {} },
      { "id": 4, "kind": "Container", "props": { "width": 40, "height": 20, "color": "#112233", "radius": 6 }, "children": [], "events": {} },
      { "id": 5, "kind": "Icon", "props": { "name": "home", "color": "#445566" }, "children": [], "events": {} }
    ], "events": {} },
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 }
}"##;

const GRAPHICS_ONLY: &str = r##"{
  "ir": "0.1.0", "app": "Gfx", "state": [],
  "tree": { "id": 0, "kind": "Column", "props": { "padding": 4 },
    "children": [
      { "id": 1, "kind": "Container", "props": { "width": 40, "height": 20, "color": "#112233", "radius": 6 }, "children": [], "events": {} },
      { "id": 2, "kind": "Icon", "props": { "name": "home", "color": "#445566" }, "children": [], "events": {} }
    ], "events": {} },
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 }
}"##;

fn render(ir_text: &str, suppress: bool) -> Vec<u8> {
    let ir = parse_ir(ir_text).unwrap();
    let state = StateView::from_ir(&ir);
    let mut r = Renderer::new(240, 200);
    r.set_suppress_text(suppress);
    r.render(&ir, &state).pixels
}

#[test]
fn default_path_keeps_all_pinned_goldens() {
    let counter = std::fs::read_to_string("../opurom-examples/golden/counter.opuir.json").unwrap();
    let ir = parse_ir(&counter).unwrap();
    let state = StateView::from_ir(&ir);
    let a = Renderer::new(320, 240).render(&ir, &state).pixels;
    let b = Renderer::new(800, 600).render(&ir, &state).pixels;
    assert_eq!(format!("{:016x}", digest(&a)), "8d45f412996a99c6");
    assert_eq!(format!("{:016x}", digest(&b)), "318beb1dd337bc4e");
    // The setter exists but defaults to off, keeping bytes identical.
    let c = Renderer::new(320, 240).render(&ir, &state).pixels;
    assert_eq!(a, c);

    // Third pinned golden: the effects fixture frame digest, via the CLI path
    // used by the committed fixture (suppress_text is not involved here).
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ppm = dir.join("text-span-effects.ppm");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_opurom-render"))
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
        format!("digest={}", digest_hex(&std::fs::read(&ppm).unwrap())),
        std::fs::read_to_string("tests/fixtures/effects.digest.txt")
            .unwrap()
            .trim()
    );
}

#[test]
fn suppress_text_changes_only_text_pixels() {
    // Graphics-only frame is identical with the flag on and off.
    assert_eq!(render(GRAPHICS_ONLY, false), render(GRAPHICS_ONLY, true));

    let on = render(MIXED, false);
    let off = render(MIXED, true);
    assert_ne!(on, off, "suppressing text must change the frame");
    assert_eq!(on.len(), off.len());

    // Every differing pixel lies inside a text node's rect (text band); the
    // container/icon/shape bytes remain untouched.
    let ir = parse_ir(MIXED).unwrap();
    let state = StateView::from_ir(&ir);
    let laid = layout(&ir, &state, 240, 200);
    let text_rects: Vec<_> = laid
        .iter()
        .filter(|n| n.text.as_ref().is_some_and(|t| !t.is_empty()))
        .map(|n| n.rect)
        .collect();
    let w = 240usize;
    let mut outside = 0usize;
    let mut inside = 0usize;
    for i in 0..on.len() / 4 {
        if on[i * 4..i * 4 + 4] != off[i * 4..i * 4 + 4] {
            let x = (i % w) as i64;
            let y = (i / w) as i64;
            if text_rects
                .iter()
                .any(|r| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h)
            {
                inside += 1;
            } else {
                outside += 1;
            }
        }
    }
    assert!(inside > 0, "text pixels must change");
    assert_eq!(outside, 0, "non-text pixels must stay byte-identical");
}

#[test]
fn span_json_is_ordered_complete_and_byte_stable() {
    let ir = parse_ir(MIXED).unwrap();
    let state = StateView::from_ir(&ir);
    let a = text_spans_for(&ir, &state, 240, 200);
    let b = text_spans_for(&ir, &state, 240, 200);
    assert_eq!(a, b, "span JSON must be byte-identical across runs");

    let laid = layout(&ir, &state, 240, 200);
    let spans = text_spans(&laid);
    assert_eq!(spans.len(), 3, "three text nodes, graphics skipped");
    assert_eq!(spans[0].align as u8, 0); // Start
    assert_eq!(spans[1].align as u8, 1); // Center
    assert_eq!(spans[2].align as u8, 2); // End
    assert_eq!(spans[0].text, "left");
    assert_eq!(spans[1].text, "mid");
    assert_eq!(spans[2].text, "right");
    // Color is the resolved text color from layout (not a hardcoded default).
    assert_eq!(spans[0].color.a, 255);
    let resolved = laid
        .iter()
        .find(|n| n.text.as_deref() == Some("left"))
        .unwrap()
        .text_color
        .unwrap();
    assert_eq!(spans[0].color, resolved);

    // Every required field present, no floats anywhere in the JSON.
    for key in [
        "\"x\":",
        "\"y\":",
        "\"w\":",
        "\"h\":",
        "\"text\":",
        "\"color\":",
        "\"size\":",
        "\"align\":",
        "\"origin\":",
    ] {
        assert!(a.contains(key), "missing field {key} in {a}");
    }
    assert!(!a.contains('.'), "span JSON must contain no floats: {a}");

    // Span-export golden: fixed IR -> fixed JSON, guarded byte-for-byte.
    let golden = format!("{:016x}", digest(a.as_bytes()));
    assert_eq!(
        golden, "b760520c2156de9d",
        "pin actual span JSON digest: {a}"
    );
    let _ = text_spans_json(&layout(&ir, &state, 240, 200));
}
