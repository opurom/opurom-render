//! Opt-in proportional text metrics.
//!
//! `theme.fontMetrics: "proportional"` (or a per-node `fontMetrics` prop)
//! switches every text consumer — natural sizing, wrap, `maxLines` ellipsis,
//! `align`, selection geometry such as the Tabs underline, and the SVG
//! `textLength` — to advances derived from each glyph's coverage bitmap. The
//! default `Fixed` path keeps the historical monospaced advance, so committed
//! frames stay byte-identical.

use opurom_render::{
    digest_hex, font, layout, parse_ir, svg_at, FontMetrics, Kind, LaidOut, Renderer, StateValue,
    StateView, SETTLED_MS,
};
use serde_json::json;

/// One `Text` node; `theme_metrics` / `node_metrics` are the optional
/// `fontMetrics` values on the theme and on the node itself.
fn text_ir(
    theme_metrics: Option<&str>,
    node_metrics: Option<&str>,
    value: &str,
    width: Option<i64>,
) -> String {
    let mut theme = json!({ "mode": "light", "accent": "#097FEB", "radius": 4 });
    if let Some(m) = theme_metrics {
        theme["fontMetrics"] = json!(m);
    }
    let mut props = json!({ "value": value, "size": 24 });
    if let Some(m) = node_metrics {
        props["fontMetrics"] = json!(m);
    }
    if let Some(w) = width {
        props["width"] = json!(w);
    }
    json!({
        "ir": "0.1.0", "app": "T", "state": [],
        "tree": { "id": 0, "kind": "Column", "props": { "padding": 4 },
            "children": [ { "id": 1, "kind": "Text", "props": props,
                "children": [], "events": {} } ], "events": {} },
        "theme": theme
    })
    .to_string()
}

fn laid(ir: &str) -> LaidOut {
    let doc = parse_ir(ir).unwrap();
    let state = StateView::from_ir(&doc);
    layout(&doc, &state, 320, 120)
        .into_iter()
        .find(|n| n.kind == Kind::Text)
        .expect("the text node is laid out")
}

const LINE: &str = "Width Minimum i l W m";

#[test]
fn fixed_metrics_are_the_default_and_keep_the_monospaced_advance() {
    let scale = 3;
    assert_eq!(font::advance(scale), (font::GLYPH_W + 1) * scale);
    for text in [LINE, "a", "WWWW", ""] {
        assert_eq!(
            font::text_width(text, scale, FontMetrics::Fixed),
            (text.chars().count() as i64) * font::advance(scale),
            "fixed width must stay the historical monospaced advance"
        );
    }
    // No `fontMetrics` key at all resolves to Fixed, for the node and the theme.
    let doc = parse_ir(&text_ir(None, None, LINE, None)).unwrap();
    assert_eq!(doc.theme.font_metrics, FontMetrics::Fixed);
    assert_eq!(
        laid(&text_ir(None, None, LINE, None)).font_metrics,
        FontMetrics::Fixed
    );
}

#[test]
fn proportional_advance_is_derived_from_the_coverage_bitmap() {
    let scale = 3;
    // Narrow glyphs are compact, wide glyphs keep the full cell.
    let narrow = font::glyph_advance('i', scale, FontMetrics::Proportional);
    let wide = font::glyph_advance('W', scale, FontMetrics::Proportional);
    assert!(
        narrow < wide,
        "advance('i')={narrow} must be < advance('W')={wide}"
    );
    assert!(narrow >= 1, "a glyph must never collapse to a zero advance");
    for code in 0x20u8..=0x7e {
        let a = font::glyph_advance(code as char, scale, FontMetrics::Proportional);
        assert!(a >= 1, "zero advance for {code}");
        assert!(
            a <= font::advance(scale),
            "proportional advance exceeds the fixed cell for {code}"
        );
    }
    // The measured run width is the sum of the per-glyph advances.
    let run = font::text_width(LINE, scale, FontMetrics::Proportional);
    let sum: i64 = LINE
        .chars()
        .map(|c| font::glyph_advance(c, scale, FontMetrics::Proportional))
        .sum();
    assert_eq!(run, sum);
}

#[test]
fn a_mixed_case_line_is_measurably_shorter_than_the_fixed_baseline() {
    let scale = 3;
    let fixed = font::text_width(LINE, scale, FontMetrics::Fixed);
    let proportional = font::text_width(LINE, scale, FontMetrics::Proportional);
    assert!(
        proportional < fixed,
        "proportional {proportional} must be shorter than fixed {fixed}"
    );
    for word in ["Width", "minimum", "Hello, World!", "Opurom Design System"] {
        assert!(
            font::text_width(word, scale, FontMetrics::Proportional)
                < font::text_width(word, scale, FontMetrics::Fixed),
            "{word} must shrink"
        );
    }
}

#[test]
fn theme_font_metrics_is_opt_in_and_node_level_overrides_the_theme() {
    assert_eq!(
        laid(&text_ir(Some("proportional"), Some("fixed"), LINE, None)).font_metrics,
        FontMetrics::Fixed,
        "an explicit node prop beats the theme"
    );
    assert_eq!(
        laid(&text_ir(Some("proportional"), None, LINE, None)).font_metrics,
        FontMetrics::Proportional
    );
    assert_eq!(
        laid(&text_ir(None, None, LINE, None)).font_metrics,
        FontMetrics::Fixed
    );
    let fixed = laid(&text_ir(None, None, LINE, None));
    let prop = laid(&text_ir(Some("proportional"), None, LINE, None));
    // Natural sizing shrinks with the new metrics: no explicit width, so the
    // node box is the measured run width.
    assert!(
        prop.rect.w < fixed.rect.w,
        "natural text width must shrink: {} vs {}",
        prop.rect.w,
        fixed.rect.w
    );
    assert_eq!(
        prop.rect.w,
        font::text_width(LINE, 3, FontMetrics::Proportional)
    );
}

#[test]
fn wrap_and_ellipsis_use_the_same_proportional_width() {
    let (scale, max_w) = (3, 90);
    let long = "alpha beta gamma delta epsilon zeta";
    let lines = font::wrap_width(long, max_w, scale);
    assert!(lines.len() > 2, "expected several wrapped lines: {lines:?}");
    for line in &lines {
        assert!(
            font::text_width(line, scale, FontMetrics::Proportional) <= max_w,
            "wrapped line {line:?} exceeds {max_w}"
        );
    }
    // `maxLines` caps the line count; the last line ends in ASCII dots and
    // still fits the same measured width budget.
    let capped = font::wrap_width_limited(long, max_w, scale, Some(2));
    assert_eq!(capped.len(), 2, "ellipsis stays inside maxLines");
    assert!(capped[1].ends_with("..."));
    assert!(
        font::text_width(&capped[1], scale, FontMetrics::Proportional) <= max_w,
        "the ellipsis line must fit its width budget"
    );
    // A zero cap draws nothing, matching the Fixed path.
    assert!(font::wrap_width_limited(long, max_w, scale, Some(0)).is_empty());
}

#[test]
fn align_centers_with_the_new_metrics() {
    let (box_w, scale) = (200, 3);
    let svg_text = |metrics: Option<&str>| -> (i64, i64) {
        let mut theme = json!({ "mode": "light", "accent": "#097FEB", "radius": 4 });
        if let Some(m) = metrics {
            theme["fontMetrics"] = json!(m);
        }
        let doc = json!({
            "ir": "0.1.0", "app": "C", "state": [],
            "tree": { "id": 0, "kind": "Column", "props": { "padding": 4 },
                "children": [ { "id": 1, "kind": "Text", "props": {
                    "value": "Width", "size": 24, "width": box_w, "align": "center" },
                    "children": [], "events": {} } ], "events": {} },
            "theme": theme
        })
        .to_string();
        let ir = parse_ir(&doc).unwrap();
        let state = StateView::from_ir(&ir);
        let svg = svg_at(&ir, &state, 320, 120, SETTLED_MS);
        let line = svg
            .lines()
            .find(|l| l.contains("<text"))
            .expect("the SVG emits a text element")
            .to_string();
        let attr = |name: &str| -> i64 {
            line.split(&format!(" {name}=\""))
                .nth(1)
                .and_then(|s| s.split('"').next())
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| panic!("missing {name} in {line}"))
        };
        (attr("x"), attr("textLength"))
    };
    let (fixed_x, fixed_len) = svg_text(None);
    let (prop_x, prop_len) = svg_text(Some("proportional"));
    let fixed_tw = font::text_width("Width", scale, FontMetrics::Fixed);
    let prop_tw = font::text_width("Width", scale, FontMetrics::Proportional);
    assert_ne!(
        fixed_tw, prop_tw,
        "the two metrics must measure differently"
    );
    assert_eq!(fixed_len, fixed_tw, "textLength uses the fixed metrics");
    assert_eq!(
        prop_len, prop_tw,
        "textLength uses the proportional metrics"
    );
    // Centre placement offsets by the measured width, so the narrower
    // proportional run sits further right by half the width difference.
    assert!(prop_x > fixed_x, "a narrower run is centred further right");
    assert_eq!(
        prop_x - fixed_x,
        (box_w - prop_tw) / 2 - (box_w - fixed_tw) / 2,
        "centre align offsets by the measured width"
    );
}

#[test]
fn proportional_rendering_is_deterministic_and_differs_from_fixed() {
    let render = |metrics: Option<&str>| {
        let ir = parse_ir(&text_ir(metrics, None, LINE, None)).unwrap();
        let state = StateView::from_ir(&ir);
        digest_hex(&Renderer::new(320, 120).pixels(&ir, &state))
    };
    let fixed = render(None);
    let prop = render(Some("proportional"));
    assert_ne!(fixed, prop, "opt-in metrics must change the frame");
    assert_eq!(
        prop,
        render(Some("proportional")),
        "repeated renders are identical"
    );
    assert_eq!(fixed, render(None), "the fixed path is unchanged");
}

/// Rightmost column holding dark text ink; the light-mode backdrop, the
/// container fill and its 1 px border are all lighter than this threshold.
fn rightmost_ink(pixels: &[u8], width: u32, height: u32) -> i64 {
    for x in (0..width as i64).rev() {
        for y in 0..height {
            let i = (y as usize * width as usize + x as usize) * 4;
            if pixels[i] < 128 {
                return x;
            }
        }
    }
    -1
}

#[test]
fn a_proportional_line_paints_less_ink_than_the_fixed_baseline() {
    // A 500 px viewport keeps the whole line on one wrapped line for both
    // metrics, so the rightmost inked column really is the line's end.
    let render = |metrics: Option<&str>| {
        let ir = parse_ir(&text_ir(metrics, None, LINE, None)).unwrap();
        let state = StateView::from_ir(&ir);
        Renderer::new(500, 120).pixels(&ir, &state)
    };
    let fixed = render(None);
    let prop = render(Some("proportional"));
    let fixed_right = rightmost_ink(&fixed, 500, 120);
    let prop_right = rightmost_ink(&prop, 500, 120);
    assert!(
        fixed_right >= 0 && prop_right >= 0,
        "both frames paint text"
    );
    assert!(
        prop_right < fixed_right,
        "the proportional line must end earlier: {prop_right} vs {fixed_right}"
    );
}

#[test]
fn the_three_canonical_goldens_stay_byte_identical_without_the_flag() {
    let doc = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../opurom-examples/golden/counter.opuir.json"
    ))
    .expect("the canonical IR is readable");
    let ir = parse_ir(&doc).unwrap();
    assert_eq!(ir.theme.font_metrics, FontMetrics::Fixed);
    let state = StateView::from_ir(&ir);
    let render = |w: u32, h: u32| digest_hex(&Renderer::new(w, h).pixels(&ir, &state));
    assert_eq!(render(320, 240), "8d45f412996a99c6");
    assert_eq!(render(800, 600), "318beb1dd337bc4e");
    let mut counted = StateView::from_ir(&ir);
    counted.set("count".to_string(), StateValue::Int(7));
    assert_eq!(
        digest_hex(&Renderer::new(800, 600).pixels(&ir, &counted)),
        "40073cb95c912d80"
    );
}
