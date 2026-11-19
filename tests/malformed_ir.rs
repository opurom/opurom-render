//! Fail-closed gates for malformed IR/JSON at the display-list entry point.
//! Every case must return a typed error, never a partial frame or panic.

use opurom_render::{parse_ir, Renderer, StateView};
use serde_json::json;

fn base_doc() -> serde_json::Value {
    json!({
        "ir": "0.1.0", "app": "Malformed", "state": [],
        "tree": { "id": 0, "kind": "Column", "props": { "padding": 0 },
            "children": [ { "id": 1, "kind": "Container", "props": {
                "width": 40, "height": 40, "color": "#112233", "radius": 4
            }, "children": [], "events": {} } ], "events": {} },
        "theme": { "mode": "light", "accent": "#097FEB", "radius": 4 }
    })
}

fn rejected(mut edit: impl FnMut(&mut serde_json::Value)) -> String {
    let mut doc = base_doc();
    edit(&mut doc);
    let text = doc.to_string();
    let err = match parse_ir(&text) {
        Ok(ir) => {
            // A value may parse but must never produce a display list with
            // invalid geometry; that is a second fail-closed boundary.
            let list = Renderer::new(64, 64).display_list(&ir, &StateView::from_ir(&ir));
            match list.to_json() {
                Ok(_) => panic!("malformed IR was accepted: {text}"),
                Err(e) => e.to_string(),
            }
        }
        Err(e) => e.to_string(),
    };
    assert!(!err.is_empty(), "empty diagnostic for {text}");
    err
}

#[test]
fn truncated_json_and_wrong_types_fail_closed() {
    assert!(parse_ir("{\"ir\":\"0.1.0\"").is_err());
    let err = rejected(|d| {
        d["app"] = json!(42);
    });
    assert!(err.contains("app") || err.contains("string"), "{err}");
}

#[test]
fn unknown_version_enum_and_root_kind_fail_closed() {
    let err = rejected(|d| {
        d["ir"] = json!("9.9.9");
    });
    assert!(err.contains("unsupported IR version"), "{err}");
    let err = rejected(|d| {
        d["directionality"] = json!("sideways");
    });
    assert!(err.contains("directionality"), "{err}");
    let err = rejected(|d| {
        d["tree"]["children"][0]["props"]["align"] = json!("sideways");
    });
    assert!(err.contains("unknown value"), "{err}");
}

#[test]
fn out_of_range_layout_geometry_is_clamped_inside_the_viewport() {
    // Layout clamps authored geometry to the viewport before display-list
    // validation, so the wire stays valid even for extreme authored values.
    for bad in [json!(i64::MAX), json!(-4), json!(i64::MIN)] {
        let mut doc = base_doc();
        doc["tree"]["children"][0]["props"]["width"] = bad.clone();
        match parse_ir(&doc.to_string()) {
            Ok(ir) => {
                let list = Renderer::new(64, 64).display_list(&ir, &StateView::from_ir(&ir));
                list.to_json()
                    .unwrap_or_else(|e| panic!("bad width {bad}: {e}"));
            }
            Err(e) => assert!(
                e.to_string().contains("non-negative"),
                "unexpected diagnostic for {bad}: {e}"
            ),
        }
    }
}

#[test]
fn diagnostics_are_deterministic_for_the_same_bad_input() {
    let mut doc = base_doc();
    doc["ir"] = json!("bogus");
    let text = doc.to_string();
    let a = parse_ir(&text).unwrap_err().to_string();
    let b = parse_ir(&text).unwrap_err().to_string();
    assert_eq!(a, b);
}
