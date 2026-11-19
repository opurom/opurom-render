//! Root `directionality` contract: strict ltr/rtl parse, default ltr, and a
//! default path whose pinned digests stay byte-identical.

use opurom_render::{digest_hex, parse_ir, Directionality, Renderer, StateView};
use serde_json::json;
use std::{fs, path::PathBuf, process::Command};

fn ir_with(value: Option<serde_json::Value>) -> String {
    let mut doc = json!({
        "ir": "0.1.0", "app": "Rtl", "state": [],
        "tree": { "id": 0, "kind": "Column", "props": { "padding": 0 },
            "children": [], "events": {} },
        "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 }
    });
    if let Some(v) = value {
        doc["directionality"] = v;
    }
    doc.to_string()
}

#[test]
fn directionality_parses_ltr_rtl_and_defaults_to_ltr() {
    assert_eq!(
        parse_ir(&ir_with(None)).unwrap().directionality,
        Directionality::Ltr
    );
    assert_eq!(
        parse_ir(&ir_with(Some(json!("ltr"))))
            .unwrap()
            .directionality,
        Directionality::Ltr
    );
    assert_eq!(
        parse_ir(&ir_with(Some(json!("rtl"))))
            .unwrap()
            .directionality,
        Directionality::Rtl
    );
    assert_eq!(Directionality::parse("rtl"), Some(Directionality::Rtl));
    assert_eq!(Directionality::default(), Directionality::Ltr);
}

#[test]
fn unknown_or_mistyped_directionality_is_rejected() {
    for bad in [
        json!("RTL"),
        json!("auto"),
        json!(1),
        json!(null),
        json!([]),
    ] {
        assert!(
            parse_ir(&ir_with(Some(bad.clone()))).is_err(),
            "invalid directionality must be a typed error: {bad}"
        );
    }
    let err = parse_ir(&ir_with(Some(json!("auto"))))
        .unwrap_err()
        .to_string();
    assert!(err.contains("directionality"), "{err}");
}

#[test]
fn absent_directionality_keeps_all_pinned_goldens() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let counter = fs::read_to_string("../opurom-examples/golden/counter.opuir.json").unwrap();
    let ir = parse_ir(&counter).unwrap();
    assert_eq!(ir.directionality, Directionality::Ltr);
    let state = StateView::from_ir(&ir);
    assert_eq!(
        digest_hex(&Renderer::new(320, 240).render(&ir, &state).pixels),
        "8d45f412996a99c6"
    );
    assert_eq!(
        digest_hex(&Renderer::new(800, 600).render(&ir, &state).pixels),
        "318beb1dd337bc4e"
    );

    let ppm = dir.join("directionality-effects.ppm");
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
