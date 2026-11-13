//! Extended primitive tests: 68 additional DISTINCT drawn widget kinds.
//!
//! Every kind is checked against the documented fallback, against the other
//! new kinds, and across repeated runs; a single test enumerates all new kinds
//! and asserts pairwise-distinct digests. Prop schemas, a11y roles, the SVG
//! data-role backend and the three 0.1.0 golden digests are covered too.

use opurom_render::a11y::{a11y_tree, role_of};
use opurom_render::primitives::{self, COUNT, CONTRACTS};
use opurom_render::{digest_hex, parse_ir, svg, Kind, Renderer, Role, StateView};
use std::collections::HashSet;
use std::{fs, path::PathBuf, process::Command};

/// The 32 pre-existing primitives, unchanged by the extended library.
const EXISTING: &[&str] = &[
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

/// One self-contained primitive as the root, with a fixed box. `label` gives the
/// unknown-kind fallback text to draw, so a fallback frame is never empty.
fn prim_ir(kind: &str, extra: &str) -> String {
    let mut props = String::from("\"width\":160,\"height\":100,\"label\":\"sample\"");
    if !extra.is_empty() {
        props.push(',');
        props.push_str(extra);
    }
    format!(
        "{{\"ir\":\"0.1.0\",\"app\":\"P\",\"state\":[],\"tree\":{{\"id\":0,\"kind\":\"{}\",\"props\":{{{}}},\"children\":[],\"events\":{{}}}},\"theme\":{{\"mode\":\"light\",\"accent\":\"#097FEB\",\"radius\":4}}}}",
        kind, props
    )
}

fn digest_of_text(text: &str) -> String {
    let ir = parse_ir(text).unwrap();
    let state = StateView::from_ir(&ir);
    digest_hex(&Renderer::new(200, 140).pixels(&ir, &state))
}

fn digest_of_kind(kind: &str) -> String {
    digest_of_text(&prim_ir(kind, "\"values\":[10,90,30,70,50,20,80,40]"))
}

#[test]
fn capability_has_68_new_kinds_on_top_of_32() {
    assert_eq!(COUNT, 68);
    assert_eq!(CONTRACTS.len(), COUNT);
    assert_eq!(EXISTING.len(), 32);
    assert_eq!(32 + COUNT, 100);
    let unique: HashSet<&str> = CONTRACTS.iter().map(|s| s.name).collect();
    assert_eq!(unique.len(), COUNT, "extended kind names must be unique");
    for name in EXISTING {
        assert!(!unique.contains(name), "{name} must not be redefined");
    }
    for contract in CONTRACTS.iter() {
        assert!(
            !contract.props.is_empty(),
            "{} needs a documented prop schema",
            contract.name
        );
        assert!(!contract.family.is_empty(), "{} needs a family", contract.name);
    }
}

#[test]
fn capability_kinds_parse_to_extended_and_round_trip_their_names() {
    for (code, contract) in CONTRACTS.iter().enumerate() {
        let kind = Kind::parse(contract.name);
        assert_eq!(
            kind,
            Kind::Extended(code as u16),
            "{} must parse to its code",
            contract.name
        );
        assert_eq!(kind.name(), contract.name, "name() must round-trip");
        assert!(kind.is_leaf(), "{} is a self-contained drawing", contract.name);
        assert_eq!(primitives::name_of(code as u16), contract.name);
        assert_eq!(primitives::role_of(code as u16), contract.role);
        assert_eq!(primitives::prop_schema(code as u16), contract.props);
        assert_eq!(primitives::family_of(code as u16), contract.family);
    }
    // The documented fallback is still reachable for an unknown kind.
    assert_eq!(Kind::parse("FutureWidget"), Kind::Other);
}

#[test]
fn capability_primitives_differ_from_fallback_and_are_deterministic() {
    let fallback = digest_of_kind("FutureWidget");
    for contract in CONTRACTS.iter() {
        let a = digest_of_kind(contract.name);
        let b = digest_of_kind(contract.name);
        assert_eq!(a, b, "{} must be pixel-deterministic", contract.name);
        assert_ne!(a, fallback, "{} must not equal the fallback", contract.name);
    }
}

#[test]
fn capability_all_new_kinds_have_pairwise_distinct_digests() {
    // One test enumerating every new kind and asserting pairwise-distinct digests.
    let digests: Vec<(&str, String)> = CONTRACTS
        .iter()
        .map(|s| (s.name, digest_of_kind(s.name)))
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
    let unique: HashSet<&String> = digests.iter().map(|(_, d)| d).collect();
    assert_eq!(unique.len(), COUNT);
}

#[test]
fn capability_all_new_kinds_are_distinct_even_with_identical_props() {
    // A 64-entry constant series makes `series()` identical for every kind, so
    // distinctness must come from each kind's own geometry and identity band,
    // not from the props. This rules out parameter-only variations.
    let series: Vec<String> = (0..64).map(|_| "50".to_string()).collect();
    let extra = format!("\"values\":[{}]", series.join(","));
    let digests: Vec<(&str, String)> = CONTRACTS
        .iter()
        .map(|s| (s.name, digest_of_text(&prim_ir(s.name, &extra))))
        .collect();
    for i in 0..digests.len() {
        for j in (i + 1)..digests.len() {
            assert_ne!(
                digests[i].1, digests[j].1,
                "{} and {} drew identical pixels with identical props",
                digests[i].0, digests[j].0
            );
        }
    }
}

#[test]
fn capability_new_kinds_differ_from_all_existing_kinds() {
    let existing: HashSet<String> = EXISTING.iter().map(|k| digest_of_kind(k)).collect();
    for contract in CONTRACTS.iter() {
        assert!(
            !existing.contains(&digest_of_kind(contract.name)),
            "{} collided with an existing primitive",
            contract.name
        );
    }
}

#[test]
fn capability_prop_values_change_pixels_and_are_parsed() {
    let a = digest_of_text(&prim_ir("Histogram", "\"values\":[10,90,30,70,50]"));
    let b = digest_of_text(&prim_ir("Histogram", "\"values\":[50,50,50,50,50]"));
    assert_ne!(a, b, "the documented values prop must drive the drawing");
    let ir = parse_ir(&prim_ir("Histogram", "\"values\":[10,90,30,70,50]")).unwrap();
    assert_eq!(ir.tree.props.values, vec![10, 90, 30, 70, 50]);
    // A comma-separated string is the alternate documented form.
    let csv = digest_of_text(&prim_ir("Histogram", "\"values\":\"10,90,30,70,50\""));
    assert_eq!(a, csv);
    // Every kind accepts the same documented prop schema without a parse error.
    for contract in CONTRACTS.iter() {
        assert!(parse_ir(&prim_ir(
            contract.name,
            "\"value\":42,\"columns\":3,\"rows\":2,\"labels\":[\"a\",\"b\"],\"segments\":4"
        ))
        .is_ok());
    }
}

#[test]
fn capability_kinds_have_defined_a11y_roles_in_tree_and_svg() {
    for contract in CONTRACTS.iter() {
        let ir = parse_ir(&prim_ir(contract.name, "")).unwrap();
        let state = StateView::from_ir(&ir);
        assert_eq!(
            role_of(&ir.tree),
            contract.role,
            "default role of {}",
            contract.name
        );
        let tree = a11y_tree(&ir, &state, 200, 140);
        assert_eq!(tree[0].role, contract.role, "a11y tree role of {}", contract.name);
        let doc = svg(&ir, &state, 200, 140);
        assert!(
            doc.contains(&format!("data-role=\"{}\"", contract.role.name())),
            "SVG data-role of {}",
            contract.name
        );
        assert!(doc.contains("<rect"), "{} needs an SVG box", contract.name);
    }
    // An explicit role still wins on an extended kind.
    let ir = parse_ir(&prim_ir("Gauge", "\"role\":\"button\"")).unwrap();
    assert_eq!(role_of(&ir.tree), Role::Button);
}

#[test]
fn capability_control_carries_code_and_series() {
    use opurom_render::{layout, Control};
    let ir = parse_ir(&prim_ir("Histogram", "\"values\":[3,8,5]")).unwrap();
    let laid = layout(&ir, &StateView::from_ir(&ir), 200, 140);
    assert_eq!(
        laid[0].control,
        Some(Control::Extended {
            code: 7,
            values: vec![3, 8, 5]
        })
    );
}

#[test]
fn capability_cli_is_deterministic_across_processes() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let exe = env!("CARGO_BIN_EXE_opurom-render");
    for contract in [&CONTRACTS[0], &CONTRACTS[34], &CONTRACTS[67]] {
        let ir_path = dir.join(format!("capability-{}.opuir.json", contract.name));
        fs::write(&ir_path, prim_ir(contract.name, "")).unwrap();
        let run = |out: &str| -> (String, Vec<u8>) {
            let ppm = dir.join(out);
            let o = Command::new(exe)
                .arg(&ir_path)
                .args(["200", "140"])
                .arg(&ppm)
                .output()
                .unwrap();
            assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
            (
                String::from_utf8_lossy(&o.stdout).to_string(),
                fs::read(&ppm).unwrap(),
            )
        };
        let (s1, p1) = run(&format!("capability-{}-1.ppm", contract.name));
        let (s2, p2) = run(&format!("capability-{}-2.ppm", contract.name));
        assert_eq!(s1, s2, "{} stdout must be deterministic", contract.name);
        assert_eq!(p1, p2, "{} PPM must be byte-identical", contract.name);
    }
}

#[test]
fn capability_spec_catalog_is_byte_identical_to_the_vendored_copy() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let contract = root.join("../opurom-examples/golden/components.json");
    let vendored = root.join("../opurom-lang/src/catalog.json");
    let a = fs::read(&contract).unwrap();
    let b = fs::read(&vendored).unwrap();
    assert_eq!(a, b, "contract and vendored catalog must be byte-identical");
    let doc: serde_json::Value = serde_json::from_slice(&a).unwrap();
    assert_eq!(doc["count"], 216);
    assert_eq!(doc["primitives"], 100);
    let list = doc["components"].as_array().unwrap();
    assert_eq!(list.len(), 216);
    let prims: HashSet<&str> = list.iter().map(|c| c[1].as_str().unwrap()).collect();
    assert_eq!(prims.len(), 100);
    for s in CONTRACTS.iter() {
        assert!(
            prims.contains(s.name),
            "{} missing from contract catalog",
            s.name
        );
    }
}

#[test]
fn capability_v0_1_golden_digests_are_unchanged() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let exe = env!("CARGO_BIN_EXE_opurom-render");
    let ir = "../opurom-examples/golden/counter.opuir.json";
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
    assert!(run("320", "240", "capability-golden-320.ppm", &[]).contains("digest=900abf4bd93d1e51"));
    assert!(run("800", "600", "capability-golden-800.ppm", &[]).contains("digest=c8403ba57da5ee61"));
    assert!(
        run("800", "600", "capability-golden-7.ppm", &["--state", "count=7"])
            .contains("digest=48a56056bd32a07e")
    );
}
