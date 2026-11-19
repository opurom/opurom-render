//! Fill-available Grid height (opt-in additive): `fillHeight` spreads surplus
//! box height across rows. Default grids stay byte-identical.

use opurom_render::{digest, digest_hex, layout, parse_ir, Renderer, StateView};

/// Grid of twenty keys: four compact columns, five rows. `extra` is injected
/// into the grid props; `key_props` into every key (for centering checks).
fn grid_ir(extra: &str, key_props: &str) -> String {
    let mut children = String::new();
    for id in 1..=20 {
        if id > 1 {
            children.push(',');
        }
        children.push_str(&format!(
            r#"{{ "id": {id}, "kind": "Button", "props": {{ "label": "{id}"{key_props} }}, "children": [], "events": {{}} }}"#
        ));
    }
    format!(
        r##"{{
  "ir": "0.1.0", "app": "Calc", "state": [],
  "tree": {{ "id": 0, "kind": "Grid", "props": {{ "columns": 4, "compactColumns": 4{extra} }}, "children": [{children}], "events": {{}} }},
  "theme": {{ "mode": "light", "accent": "#097FEB", "radius": 4 }}
}}"##
    )
}

fn node<'a>(laid: &'a [opurom_render::LaidOut], id: u32) -> &'a opurom_render::LaidOut {
    laid.iter().find(|n| n.id == id).expect("node id missing")
}

#[test]
fn fill_height_spreads_surplus_evenly_across_all_rows() {
    let ir = parse_ir(&grid_ir(", \"fillHeight\": true", "")).unwrap();
    let laid = layout(&ir, &StateView::from_ir(&ir), 320, 640);

    // Four distinct column x's, five distinct row y's.
    let xs: std::collections::BTreeSet<i64> = (1..=20).map(|id| node(&laid, id).rect.x).collect();
    let ymap = |laid: &[opurom_render::LaidOut]| -> Vec<i64> {
        let mut v: Vec<i64> = (1..=20).map(|id| node(laid, id).rect.y).collect();
        v.dedup();
        v
    };
    let ys = ymap(&laid);
    assert_eq!(xs.len(), 4, "four compact columns");
    assert_eq!(ys.len(), 5, "five rows");

    // Rows fill the whole 640px box, and the split is even (<=1px drift).
    let bottom = node(&laid, 17).rect.y + node(&laid, 17).rect.h;
    assert_eq!(bottom, 640, "last row reaches the bottom of the box");
    let row_h: Vec<i64> = (0..5)
        .map(|r| {
            let id = (r * 4 + 1) as u32;
            node(&laid, id).rect.h
        })
        .collect();
    assert!(
        row_h.iter().max().unwrap() - row_h.iter().min().unwrap() <= 1,
        "even split: {:?}",
        row_h
    );
    // Keys with no explicit height fill their row and are trivially centred.
    for id in 1..=20u32 {
        assert_eq!(node(&laid, id).rect.h, row_h[(id as usize - 1) / 4]);
    }
}

#[test]
fn fill_height_centres_explicit_height_keys_in_their_row() {
    // Every key declares height 10: natural rows total 50, surplus 590 gives
    // 128px rows; a 10px key must sit at row_top + (128-10)/2.
    let ir = parse_ir(&grid_ir(", \"fillHeight\": true", ", \"height\": 10")).unwrap();
    let laid = layout(&ir, &StateView::from_ir(&ir), 320, 640);
    // Row pitch is the y delta between the first key of two adjacent rows.
    let row_h = node(&laid, 5).rect.y - node(&laid, 1).rect.y;
    assert_eq!(row_h, 128, "surplus 590 over 5 rows -> 128px pitch");
    for id in 1..=20u32 {
        let r = node(&laid, id).rect;
        let row_top = ((id as usize - 1) / 4) as i64 * row_h;
        assert_eq!(r.h, 10);
        assert_eq!(r.y, row_top + (row_h - 10) / 2, "key {id} centred");
    }
}

#[test]
fn fill_height_is_a_real_negative_probe() {
    let on = grid_ir(", \"fillHeight\": true", "");
    let off = grid_ir("", "");
    let a = parse_ir(&on).unwrap();
    let b = parse_ir(&off).unwrap();
    let la = layout(&a, &StateView::from_ir(&a), 320, 640);
    let lb = layout(&b, &StateView::from_ir(&b), 320, 640);
    assert_ne!(
        node(&la, 1).rect.h,
        node(&lb, 1).rect.h,
        "flag must actually grow rows"
    );
    let sa = StateView::from_ir(&a);
    let sb = StateView::from_ir(&b);
    let pa = Renderer::new(320, 640).render(&a, &sa).pixels;
    let pb = Renderer::new(320, 640).render(&b, &sb).pixels;
    assert_ne!(pa, pb, "flag must reach the raster output");
    // Determinism: identical inputs render byte-identically.
    assert_eq!(pa, Renderer::new(320, 640).render(&a, &sa).pixels);
    assert_eq!(pb, Renderer::new(320, 640).render(&b, &sb).pixels);
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
    assert_eq!(a, Renderer::new(320, 240).render(&ir, &state).pixels);

    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ppm = dir.join("fill-height-effects.ppm");
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
