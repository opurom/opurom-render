//! SVG renderer tests: the deterministic SVG 1.1 vector backend and the eight
//! new distinct primitives (distinct pixels, deterministic, prop schemas,
//! a11y roles, 32 primitives, and unchanged v0.1 golden digests).

use opurom_render::{
    a11y::role_of, a11y_tree, digest_hex, layout, parse_ir, svg, svg_at, Control, Kind, Renderer,
    Role, StateValue, StateView, SETTLED_MS,
};
use std::{fs, path::PathBuf, process::Command};

// ------------------------------------------------------------- SVG fixture

const SVG_IR: &str = r##"{
  "ir": "0.1", "app": "Svg",
  "state": [
    { "name": "n", "type": "int", "init": 5 },
    { "name": "show", "type": "bool", "init": false }
  ],
  "tree": { "id": 0, "kind": "Column", "props": { "gap": 4, "padding": 8 },
    "children": [
      { "id": 1, "kind": "Text", "props": { "value": "N={n}" }, "children": [], "events": {} },
      { "id": 2, "kind": "Container", "props": { "width": 120, "height": 60,
          "color": "#112233", "gradient": "#4F46E5,#101828" },
        "children": [], "events": {} },
      { "id": 3, "kind": "Button", "props": { "label": "Go" }, "when": "show",
        "children": [], "events": {} }
    ], "events": {} },
  "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 }
}"##;

#[test]
fn svg_is_byte_deterministic_and_has_a_viewbox_root() {
    let ir = parse_ir(SVG_IR).unwrap();
    let state = StateView::from_ir(&ir);
    let a = svg(&ir, &state, 320, 240);
    let b = svg(&ir, &state, 320, 240);
    assert_eq!(
        a, b,
        "same IR+viewport+state+theme must give identical bytes"
    );
    assert!(a.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"));
    assert!(a.contains("<svg xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\""));
    assert!(a.contains("viewBox=\"0 0 320 240\""));
    assert!(a.trim_end().ends_with("</svg>"));
}

#[test]
fn svg_emits_gradient_defs_text_and_data_roles() {
    let ir = parse_ir(SVG_IR).unwrap();
    let state = StateView::from_ir(&ir);
    let doc = svg(&ir, &state, 320, 240);

    // Gradients become linearGradient defs referenced by url(#id).
    assert!(doc.contains("<linearGradient id=\"g0\""));
    assert!(doc.contains("stop-color=\"#4f46e5\""));
    assert!(doc.contains("stop-color=\"#101828\""));
    assert!(doc.contains("fill=\"url(#g0)\""));
    assert_eq!(doc.matches("<stop ").count(), 2);
    assert!(doc.contains("<defs>"));

    // Text uses the built-in font metrics and is interpolated + escaped.
    assert!(doc.contains("<text "));
    assert!(doc.contains("font-family=\"monospace\""));
    assert!(doc.contains("font-size=\"21\"") || doc.contains("font-size=\"14\""));
    assert!(doc.contains(">N=5</text>"));

    // data-role comes from the a11y role (Text -> text).
    assert!(doc.contains("data-role=\"text\""));
    assert!(doc.contains("data-role=\"none\""));
}

#[test]
fn svg_hidden_when_nodes_are_marked_or_omitted() {
    let ir = parse_ir(SVG_IR).unwrap();
    let hidden = StateView::from_ir(&ir); // show = false
    let hidden_doc = svg(&ir, &hidden, 320, 240);
    assert!(hidden_doc.contains("data-hidden=\"true\""));
    assert!(hidden_doc.contains("data-id=\"3\""));
    // The hidden node's drawing is omitted (no visible rect for it).
    assert!(!hidden_doc.contains("<rect data-id=\"3\""));
    assert_eq!(hidden_doc, svg(&ir, &hidden, 320, 240));

    let mut shown = StateView::from_ir(&ir);
    shown.set("show", StateValue::Bool(true));
    let shown_doc = svg(&ir, &shown, 320, 240);
    assert!(!shown_doc.contains("data-hidden=\"true\""));
    assert!(shown_doc.contains("<rect data-id=\"3\""));
    assert_ne!(hidden_doc, shown_doc);
}

#[test]
fn svg_reflects_state_viewport_and_theme() {
    let ir = parse_ir(SVG_IR).unwrap();
    let s5 = StateView::from_ir(&ir);
    let mut s9 = StateView::from_ir(&ir);
    s9.set("n", StateValue::Int(9));

    assert_ne!(
        svg(&ir, &s5, 320, 240),
        svg(&ir, &s9, 320, 240),
        "state interpolation must change the SVG"
    );
    assert!(svg(&ir, &s9, 320, 240).contains(">N=9</text>"));
    assert_ne!(
        svg(&ir, &s5, 320, 240),
        svg(&ir, &s5, 800, 600),
        "viewport must change the SVG"
    );
    let dark = parse_ir(&SVG_IR.replace("\"mode\": \"light\"", "\"mode\": \"dark\"")).unwrap();
    assert_ne!(
        svg(&ir, &s5, 320, 240),
        svg(&dark, &s5, 320, 240),
        "theme must change the SVG"
    );
}

#[test]
fn svg_at_is_tick_deterministic_and_matches_settled_default() {
    let ir = parse_ir(SVG_IR).unwrap();
    let state = StateView::from_ir(&ir);
    assert_eq!(
        svg_at(&ir, &state, 320, 240, 0),
        svg_at(&ir, &state, 320, 240, 0)
    );
    assert_eq!(
        svg_at(&ir, &state, 320, 240, SETTLED_MS),
        svg(&ir, &state, 320, 240)
    );
}

#[test]
fn cli_writes_byte_identical_svg_and_leaves_stdout_unchanged() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ir_path = dir.join("r4-svg.opuir.json");
    fs::write(&ir_path, SVG_IR).unwrap();
    let exe = env!("CARGO_BIN_EXE_opurom-render");
    let ppm = dir.join("r4-svg.ppm");
    let s1 = dir.join("r4-svg-1.svg");
    let s2 = dir.join("r4-svg-2.svg");
    let run = |out: &PathBuf| -> String {
        let o = Command::new(exe)
            .arg(&ir_path)
            .args(["320", "240"])
            .arg(&ppm)
            .arg("--svg")
            .arg(out)
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).to_string()
    };
    let stdout = run(&s1);
    assert_eq!(stdout, run(&s2));
    assert_eq!(stdout.lines().count(), 2, "stdout stays digest + dirty");
    assert!(stdout.starts_with("digest="));
    assert!(stdout.contains("dirty=1"));

    let bytes = fs::read(&s1).unwrap();
    assert_eq!(bytes, fs::read(&s2).unwrap());
    let ir = parse_ir(SVG_IR).unwrap();
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        svg(&ir, &StateView::from_ir(&ir), 320, 240)
    );

    let missing = Command::new(exe)
        .arg(&ir_path)
        .args(["320", "240"])
        .arg(&ppm)
        .arg("--svg")
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("--svg requires"));
}

// ------------------------------------------------------- particle fields

const PARTICLE_IR: &str = r##"{"ir":"0.1","app":"Dots","state":[],"tree":{"id":0,"kind":"Column","props":{"padding":8,"gap":4},"children":[{"id":1,"kind":"Container","props":{"width":96,"height":64,"color":"#223344","radius":4{FIELD}},"children":[],"events":{}}],"events":{}},"theme":{"mode":"light","accent":"#097FEB","radius":4}}"##;

fn particle_ir(field: &str) -> String {
    PARTICLE_IR.replace("{FIELD}", field)
}

/// `(cx, cy, r, fill)` of every emitted dot, in document order.
fn circles(doc: &str) -> Vec<(i64, i64, i64, String)> {
    doc.lines()
        .filter(|l| l.starts_with("  <circle "))
        .map(|l| {
            let num = |key: &str| -> i64 {
                l.split(&format!("{key}=\""))
                    .nth(1)
                    .unwrap_or_else(|| panic!("circle is missing {key}"))
                    .split('"')
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap()
            };
            let fill = l
                .split("fill=\"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap();
            (num("cx"), num("cy"), num("r"), fill.to_string())
        })
        .collect()
}

#[test]
fn svg_draws_one_circle_per_particle_dot_in_index_order() {
    let ir = parse_ir(&particle_ir(",\"particles\":{\"count\":12,\"seed\":7}")).unwrap();
    let state = StateView::from_ir(&ir);
    let doc = svg(&ir, &state, 320, 240);
    assert_eq!(
        doc,
        svg(&ir, &state, 320, 240),
        "circles must be byte-stable"
    );
    assert_eq!(circles(&doc).len(), 12, "one circle per dot");

    let laid = layout(&ir, &state, 320, 240);
    let node = laid.iter().find(|n| n.id == 1).expect("container node");
    let dots = circles(&doc);
    for (cx, cy, r, fill) in &dots {
        assert!(
            *cx >= node.rect.x && *cx < node.rect.x + node.rect.w,
            "dot centre {cx} outside the node rect"
        );
        assert!(
            *cy >= node.rect.y && *cy < node.rect.y + node.rect.h,
            "dot centre {cy} outside the node rect"
        );
        assert_eq!(*r, 2, "the default radius must reach the SVG");
        assert_eq!(fill, "#097feb", "the accent is the default dot colour");
    }
    // The seed selects positions, so a different seed must move the dots.
    let shifted = parse_ir(&particle_ir(",\"particles\":{\"count\":12,\"seed\":8}")).unwrap();
    let other = circles(&svg(&shifted, &state, 320, 240));
    assert_ne!(
        dots.iter().map(|d| d.0).collect::<Vec<_>>(),
        other.iter().map(|d| d.0).collect::<Vec<_>>()
    );
    assert!(
        doc.contains("<rect data-id=\"1\""),
        "the node box is still drawn"
    );
    assert!(
        doc.contains("data-role=\"none\""),
        "circles carry the node role"
    );
}

fn faded_dots(doc: &str) -> usize {
    doc.lines()
        .filter(|l| l.starts_with("  <circle ") && l.contains(" opacity="))
        .count()
}

#[test]
fn svg_particle_fields_skip_no_ops_and_carry_alpha() {
    for field in [
        "",
        ",\"particles\":{\"count\":0,\"seed\":7}",
        ",\"particles\":{\"count\":9,\"seed\":7,\"alpha\":0}",
    ] {
        let ir = parse_ir(&particle_ir(field)).unwrap();
        assert_eq!(
            circles(&svg(&ir, &StateView::from_ir(&ir), 320, 240)).len(),
            0,
            "field {field} must draw no dots"
        );
    }

    let opaque = parse_ir(&particle_ir(",\"particles\":{\"count\":3,\"seed\":1}")).unwrap();
    let doc = svg(&opaque, &StateView::from_ir(&opaque), 320, 240);
    assert_eq!(circles(&doc).len(), 3);
    assert_eq!(
        faded_dots(&doc),
        0,
        "a full alpha needs no opacity attribute"
    );

    let faded = parse_ir(&particle_ir(
        ",\"particles\":{\"count\":3,\"seed\":1,\"radius\":5,\"alpha\":512,\"color\":\"#FF0044\"}",
    ))
    .unwrap();
    let doc = svg(&faded, &StateView::from_ir(&faded), 320, 240);
    let dots = circles(&doc);
    assert_eq!(dots.len(), 3);
    assert!(dots.iter().all(|d| d.2 == 5 && d.3 == "#ff0044"));
    assert_eq!(faded_dots(&doc), 3, "half alpha per dot");
}

fn text_font_size(doc: &str) -> i64 {
    doc.lines()
        .find(|l| l.starts_with("  <text data-id=\"1\""))
        .and_then(|l| l.split("font-size=\"").nth(1))
        .and_then(|s| s.split('"').next())
        .and_then(|s| s.parse().ok())
        .expect("the text node must be drawn with a font-size")
}

#[test]
fn svg_text_font_size_follows_theme_font_scale() {
    let plain = parse_ir(&SVG_IR).unwrap();
    let scaled = parse_ir(&SVG_IR.replace("\"radius\": 4 }", "\"radius\": 4, \"fontScale\": 2 }"))
        .expect("theme fontScale must parse");
    let size = text_font_size(&svg(&plain, &StateView::from_ir(&plain), 320, 240));
    let grown = text_font_size(&svg(&scaled, &StateView::from_ir(&scaled), 320, 240));
    assert!(
        grown > size,
        "font-size must grow with theme.fontScale: {size} -> {grown}"
    );
}

// ------------------------------------------------ 8 new distinct primitives

/// One self-contained primitive as the root, with a fixed box and a label so
/// the fallback (an unknown kind) always has text to draw.
fn prim_ir(kind: &str, extra: &str) -> String {
    let mut props = String::from("\"width\":160,\"height\":100,\"label\":\"sample\"");
    if !extra.is_empty() {
        props.push(',');
        props.push_str(extra);
    }
    let mut s =
        String::from("{\"ir\":\"0.1\",\"app\":\"P\",\"state\":[],\"tree\":{\"id\":0,\"kind\":\"");
    s.push_str(kind);
    s.push_str("\",\"props\":{");
    s.push_str(&props);
    s.push_str(
        "},\"children\":[],\"events\":{}},\"theme\":{\"mode\":\"light\",\"accent\":\"#097FEB\",\"radius\":4}}",
    );
    s
}

fn digest_of_text(text: &str) -> String {
    let ir = parse_ir(text).unwrap();
    let state = StateView::from_ir(&ir);
    digest_hex(&Renderer::new(200, 140).pixels(&ir, &state))
}

/// kind -> (props exercising its documented prop schema).
const PRIMS: &[(&str, &str)] = &[
    (
        "Table",
        "\"columns\":3,\"rows\":3,\"labels\":[\"A\",\"B\",\"C\"],\"values\":[1,2,3,4,5,6,7,8,9]",
    ),
    ("Chart", "\"values\":[3,8,5,10,6]"),
    ("Calendar", "\"columns\":7,\"rows\":5"),
    (
        "RichText",
        "\"value\":\"the quick brown fox jumps over the lazy dog again and again\"",
    ),
    ("Player", ""),
    ("Map", ""),
    ("Menu", "\"labels\":[\"File\",\"Edit\",\"View\",\"Help\"]"),
    ("Tooltip", "\"value\":\"tip\""),
];

#[test]
fn eight_new_primitives_differ_from_fallback_and_are_deterministic() {
    for (kind, extra) in PRIMS {
        let d = digest_of_text(&prim_ir(kind, extra));
        assert_eq!(
            d,
            digest_of_text(&prim_ir(kind, extra)),
            "{kind} must be pixel-deterministic"
        );
        // An unknown kind degrades to the documented Text/Container fallback.
        let fallback = digest_of_text(&prim_ir("FutureWidget", extra));
        assert_ne!(d, fallback, "{kind} must not equal the fallback drawing");
    }
}

#[test]
fn eight_new_primitives_are_pairwise_distinct() {
    let digests: Vec<(&str, String)> = PRIMS
        .iter()
        .map(|(k, e)| (*k, digest_of_text(&prim_ir(k, e))))
        .collect();
    for i in 0..digests.len() {
        for j in (i + 1)..digests.len() {
            assert_ne!(
                digests[i].1, digests[j].1,
                "{} and {} drew identical pixels",
                digests[i].0, digests[j].0
            );
        }
    }
}

#[test]
fn new_primitive_props_are_parsed_and_change_pixels() {
    // `values` accepts an integer array or a comma-separated string.
    let arr = prim_ir("Chart", "\"values\":[3,8,5,10,6]");
    let csv = prim_ir("Chart", "\"values\":\"3,8,5,10,6\"");
    assert_eq!(digest_of_text(&arr), digest_of_text(&csv));
    assert_ne!(
        digest_of_text(&arr),
        digest_of_text(&prim_ir("Chart", "\"values\":[1,1,1,1,1]"))
    );
    // No `values` -> deterministic stand-in bars, distinct from explicit ones.
    let def = prim_ir("Chart", "");
    assert_eq!(digest_of_text(&def), digest_of_text(&def));
    assert_ne!(digest_of_text(&def), digest_of_text(&arr));

    // Typed Props fields: values, rows, columns, labels.
    let ir = parse_ir(&prim_ir(
        "Table",
        "\"columns\":4,\"rows\":2,\"values\":[1,2,3],\"labels\":[\"H\"]",
    ))
    .unwrap();
    assert_eq!(ir.tree.props.values, vec![1, 2, 3]);
    assert_eq!(ir.tree.props.rows, Some(2));
    assert_eq!(ir.tree.props.columns, Some(4));
    assert_eq!(ir.tree.props.labels, vec!["H".to_string()]);
}

#[test]
fn new_primitives_carry_their_control_data() {
    let chart = parse_ir(&prim_ir("Chart", "\"values\":[3,8,5]")).unwrap();
    let laid = layout(&chart, &StateView::from_ir(&chart), 200, 140);
    assert_eq!(
        laid[0].control,
        Some(Control::Chart {
            values: vec![3, 8, 5]
        })
    );

    let menu = parse_ir(&prim_ir("Menu", "\"labels\":[\"A\",\"B\"]")).unwrap();
    let laid = layout(&menu, &StateView::from_ir(&menu), 200, 140);
    assert_eq!(
        laid[0].control,
        Some(Control::Menu {
            items: vec!["A".into(), "B".into()]
        })
    );

    let table = parse_ir(&prim_ir(
        "Table",
        "\"columns\":4,\"rows\":2,\"labels\":[\"H\"]",
    ))
    .unwrap();
    let laid = layout(&table, &StateView::from_ir(&table), 200, 140);
    assert_eq!(
        laid[0].control,
        Some(Control::Table {
            columns: 4,
            rows: 2,
            headers: vec!["H".into()]
        })
    );
}

// ------------------------------------------------ 32 primitives + a11y roles

#[test]
fn there_are_32_distinct_primitives() {
    const NAMES: &[&str] = &[
        "Column",
        "Row",
        "Container",
        "Text",
        "Button",
        "Input",
        "Image",
        "Spacer",
        "Card",
        "Divider",
        "Icon",
        "Badge",
        "Progress",
        "Switch",
        "Slider",
        "Checkbox",
        "Radio",
        "Dialog",
        "Tabs",
        "List",
        "Grid",
        "Scroll",
        "Avatar",
        "Spinner",
        "Table",
        "Chart",
        "Calendar",
        "RichText",
        "Player",
        "Map",
        "Menu",
        "Tooltip",
    ];
    assert_eq!(NAMES.len(), 32);
    for &name in NAMES {
        let k = Kind::parse(name);
        assert_ne!(k, Kind::Other, "{name} must be a known primitive");
        assert_eq!(k.name(), name);
    }
    assert_eq!(Kind::parse("FutureWidget"), Kind::Other);
}

#[test]
fn new_primitives_have_defined_a11y_roles() {
    let expected = [
        ("Table", Role::None),
        ("Chart", Role::Image),
        ("Calendar", Role::None),
        ("RichText", Role::Text),
        ("Player", Role::Button),
        ("Map", Role::Image),
        ("Menu", Role::None),
        ("Tooltip", Role::Text),
    ];
    for (kind, role) in expected {
        let ir = parse_ir(&prim_ir(kind, "")).unwrap();
        assert_eq!(role_of(&ir.tree), role, "default role of {kind}");
        // The data-only a11y tree and the SVG data-role use the same role.
        let tree = a11y_tree(&ir, &StateView::from_ir(&ir), 200, 140);
        assert_eq!(tree[0].role, role, "a11y tree role of {kind}");
        let doc = svg(&ir, &StateView::from_ir(&ir), 200, 140);
        assert!(
            doc.contains(&format!("data-role=\"{}\"", role.name())),
            "SVG data-role of {kind}"
        );
    }
    // An explicit role always wins.
    let ir = parse_ir(&prim_ir("Chart", "\"role\":\"slider\"")).unwrap();
    assert_eq!(role_of(&ir.tree), Role::Slider);
}

// ------------------------------------------------ v0.1 golden regressions

#[test]
fn v0_1_golden_digests_are_unchanged_by_r4() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let exe = env!("CARGO_BIN_EXE_opurom-render");
    let ir = "../opurom-spec/counter.opuir.json";
    let run = |w: &str, h: &str, out: &str, extra: &[&str]| -> String {
        let o = Command::new(exe)
            .arg(ir)
            .args([w, h])
            .arg(dir.join(out))
            .args(extra)
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).to_string()
    };
    assert!(run("320", "240", "r4-golden-320.ppm", &[]).contains("digest=900abf4bd93d1e51"));
    assert!(run("800", "600", "r4-golden-800.ppm", &[]).contains("digest=c8403ba57da5ee61"));
    assert!(
        run("800", "600", "r4-golden-7.ppm", &["--state", "count=7"])
            .contains("digest=48a56056bd32a07e")
    );
}
