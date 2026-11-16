//! Reserved `set @locale` action (IR_ADDITIONS §15.4): the event spelling of a
//! locale patch bypasses the declared-state lookup; other targets do not.

use opurom_render::{parse_ir, Op, StateValue};
use serde_json::json;

fn ir_with_tap(action: serde_json::Value) -> String {
    json!({
        "ir": "0.1.0", "app": "Locale", "state": [],
        "directionality": "rtl",
        "tree": { "id": 0, "kind": "Text", "props": { "value": "hi", "align": "center" },
            "events": { "tap": action }, "children": [] },
        "theme": { "mode": "light", "accent": "#097FEB", "radius": 0 }
    })
    .to_string()
}

fn locale_action(tag: &str) -> serde_json::Value {
    json!({ "op": "set", "target": "@locale", "value": tag })
}

#[test]
fn set_locale_action_loads_and_carries_the_literal_tag() {
    for tag in ["en", "fr-CA", "zh-Hans-CN", "de-1996"] {
        let ir = parse_ir(&ir_with_tap(locale_action(tag))).unwrap();
        let a = &ir.tree.events[0].actions[0];
        assert_eq!(a.op, Op::Set);
        assert_eq!(a.target, "@locale");
        assert_eq!(a.value, Some(StateValue::Str(tag.to_string())));
        assert_eq!(a.value_from, None);
    }
}

#[test]
fn malformed_locale_tag_and_value_from_are_typed_errors() {
    for bad in ["EN", "e", "abcdefg", "en-x", "en-C", "en-"] {
        assert!(
            parse_ir(&ir_with_tap(locale_action(bad))).is_err(),
            "invalid BCP-47 tag must be rejected: {bad}"
        );
    }
    let with_from = json!({ "op": "set", "target": "@locale", "valueFrom": "x" });
    let err = parse_ir(&ir_with_tap(with_from)).unwrap_err().to_string();
    assert!(
        err.contains("@locale") && err.contains("valueFrom"),
        "{err}"
    );
    let no_value = json!({ "op": "set", "target": "@locale" });
    assert!(parse_ir(&ir_with_tap(no_value)).is_err());
}

#[test]
fn ordinary_undeclared_target_is_still_rejected() {
    // The reserved branch must not loosen validation for normal state targets.
    for action in [
        json!({ "op": "set", "target": "missing", "value": 1 }),
        json!({ "op": "add", "target": "@locale", "value": 1 }),
        json!({ "op": "toggle", "target": "@locale" }),
    ] {
        let err = parse_ir(&ir_with_tap(action.clone()))
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("undeclared state") || err.contains("needs"),
            "non-set @locale and ghost targets stay rejected: {action} -> {err}"
        );
    }
}
