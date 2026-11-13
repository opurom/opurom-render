//! Global `theme.fontScale` is the default for nodes without a local prop.

use opurom_render::{a11y_tree, layout, parse_ir, Renderer, StateView};

const BASE: &str = r##"{
  "ir": "0.1",
  "app": "ThemeScale",
  "state": [{"name": "count", "type": "int", "init": 0}],
  "tree": {
    "id": 0, "kind": "Column", "props": {"padding": 16}, "events": {},
    "children": [
      {"id": 1, "kind": "Text", "props": {"value": "Hello", "size": 16}, "children": [], "events": {}}
    ]
  },
  "theme": {"mode": "system", "accent": "#097FEB", "radius": 12}
}"##;

fn with_theme(extra: &str) -> String {
    BASE.replace(
        "\"accent\": \"#097FEB\", \"radius\": 12",
        &format!("\"accent\": \"#097FEB\", \"radius\": 12, {extra}"),
    )
}

fn digest(text: &str) -> u64 {
    let ir = parse_ir(text).expect("parse theme IR");
    let state = StateView::from_ir(&ir);
    let mut renderer = Renderer::new(320, 240);
    let frame = renderer.render(&ir, &state);
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in &frame.pixels {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn text_scale(text: &str) -> i64 {
    let ir = parse_ir(text).expect("parse theme IR");
    let state = StateView::from_ir(&ir);
    layout::layout(&ir, &state, 320, 240)
        .iter()
        .find(|n| n.text.is_some())
        .expect("text node")
        .text_scale
}

#[test]
fn theme_font_scale_applies_without_a_local_prop() {
    let plain = digest(BASE);
    let scaled = digest(&with_theme("\"fontScale\": 1.5"));
    assert_ne!(plain, scaled, "a global scale must change rendered text");
}

#[test]
fn local_font_scale_overrides_the_global_value() {
    let scaled = digest(&with_theme("\"fontScale\": 1.5"));
    let only_local = BASE.replace(
        "\"value\": \"Hello\", \"size\": 16",
        "\"value\": \"Hello\", \"size\": 16, \"fontScale\": 1.5",
    );
    assert_eq!(
        digest(&only_local),
        scaled,
        "one node with a local scale must match the global default"
    );

    let overridden = with_theme("\"fontScale\": 2.0").replace(
        "\"value\": \"Hello\", \"size\": 16",
        "\"value\": \"Hello\", \"size\": 16, \"fontScale\": 1.5",
    );
    assert_eq!(
        digest(&overridden),
        digest(&only_local),
        "a local prop wins over the theme value"
    );
}

#[test]
fn theme_font_scale_rejects_out_of_range_values() {
    for value in ["0.49", "3.01", "-1", "\"1.5\""] {
        let text = with_theme(&format!("\"fontScale\": {value}"));
        let err = parse_ir(&text).unwrap_err().to_string();
        assert!(
            err.contains("theme.fontScale"),
            "expected a theme.fontScale diagnostic, got {err}"
        );
    }
}

#[test]
fn theme_has_no_default_font_scale() {
    let ir = parse_ir(BASE).expect("parse base IR");
    assert_eq!(ir.theme.font_scale, None);
}

#[test]
fn global_scale_reaches_layout_and_accessibility() {
    let ir = parse_ir(&with_theme("\"fontScale\": 2.0")).expect("parse scaled IR");
    let state = StateView::from_ir(&ir);
    assert!(text_scale(&with_theme("\"fontScale\": 2.0")) > text_scale(BASE));
    assert!(
        a11y_tree(&ir, &state, 320, 240)
            .iter()
            .any(|n| n.role == opurom_render::Role::Heading),
        "a 16px text at 2x becomes a heading in the accessibility tree"
    );
}
