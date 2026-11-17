//! Signed text-baseline oracle fixtures and the pure gate comparator.
//!
//! The metrics gate compares render's integer baseline/box (produced by the
//! real parse_ir -> layout -> text_spans path) with the signed Flutter oracle
//! pixels. Both baseline and box height are compared on every signed case,
//! including lineHeight 1.0/1.5/2.0 after the leading-distribution fix.

use opurom_render::{layout, parse_ir, text_spans, StateView};
use serde_json::{json, Value};

pub const PINNED_SOURCE_COMMIT: &str = "ab2912035e";
pub const PINNED_FONT_SHA256: &str =
    "79e851404657dac2106b3d22ad256d47824a9a5765458edb72c9102a45816d95";
pub const PIXEL_NA_REASON: &str = "N/A - no font file / no CoreText / no Flutter oracle in CI";

const METRICS_JSON: &str = include_str!("../fixtures/text-baseline-metrics.json");
const PIXEL_JSON: &str = include_str!("../fixtures/text-baseline-pixel-ref.json");

#[derive(Clone, Debug)]
pub struct PerEm {
    pub ascent: i64,
    pub descent: i64,
    pub leading: i64,
}

#[derive(Clone, Debug)]
pub struct Case {
    pub font_px: i64,
    pub sample: String,
    pub script: String,
    pub line_height: Option<f64>,
    pub ascent_px: f64,
    pub descent_px: f64,
    pub baseline_px: f64,
    pub height_px: f64,
}

#[derive(Clone, Debug)]
pub struct Violation {
    pub font_px: i64,
    pub sample: String,
    pub field: &'static str,
    pub got: f64,
    pub want: f64,
}

#[derive(Clone, Debug)]
pub struct CheckResult {
    pub checked: usize,
    pub baseline_checked: usize,
    pub violations: Vec<Violation>,
}

pub fn metrics_fixture() -> Value {
    serde_json::from_str(METRICS_JSON).expect("signed metrics fixture must parse")
}

pub fn pixel_fixture() -> Value {
    serde_json::from_str(PIXEL_JSON).expect("signed pixel-ref fixture must parse")
}

fn value_f64(v: &Value, key: &str) -> f64 {
    v[key]
        .as_f64()
        .unwrap_or_else(|| panic!("missing numeric {key}"))
}

pub fn per_em(f: &Value) -> PerEm {
    let m = &f["perEm"]["Roboto"];
    PerEm {
        ascent: m["ascent"].as_i64().expect("signed ascent"),
        descent: m["descent"].as_i64().expect("signed descent"),
        leading: m["leading"].as_i64().expect("signed leading"),
    }
}

pub fn cases(f: &Value) -> Vec<Case> {
    f["cases"]
        .as_array()
        .expect("signed cases array")
        .iter()
        .map(|c| Case {
            font_px: c["fontPx"].as_i64().expect("signed fontPx"),
            sample: c["sample"].as_str().expect("signed sample").to_string(),
            script: c["script"].as_str().expect("signed script").to_string(),
            line_height: c["lineHeight"].as_f64(),
            ascent_px: value_f64(c, "ascent_px"),
            descent_px: value_f64(c, "descent_px"),
            baseline_px: value_f64(c, "baseline_px"),
            height_px: value_f64(c, "height_px"),
        })
        .collect()
}

pub fn tolerance_px(f: &Value) -> f64 {
    value_f64(&f["thresholds"], "metricsPx")
}

fn ir_doc(metrics: &PerEm, case: &Case) -> String {
    let theme = json!({
        "mode": "light", "accent": "#097FEB", "radius": 0,
        "fontFamily": "Roboto",
        "verticalMetrics": {"Roboto": {
            "ascent": metrics.ascent, "descent": metrics.descent, "leading": metrics.leading
        }}
    });
    let mut props = json!({"value": case.sample, "fontPx": case.font_px});
    if let Some(h) = case.line_height {
        props["lineHeight"] = json!(h);
    }
    json!({
        "ir": "0.1.0", "app": "TBG", "state": [],
        "tree": {"id": 0, "kind": "Column", "props": {}, "children": [
            {"id": 1, "kind": "Text", "props": props, "children": [], "events": {}}
        ], "events": {}},
        "theme": theme
    })
    .to_string()
}

/// Render's real (box height, absolute baseline) for one signed case.
pub fn computed(metrics: &PerEm, case: &Case) -> (f64, f64) {
    let text = ir_doc(metrics, case);
    let ir = parse_ir(&text).expect("signed IR must parse");
    let spans = text_spans(&layout(&ir, &StateView::from_ir(&ir), 400, 200));
    assert_eq!(spans.len(), 1, "one span for the whole run");
    let baseline = spans[0]
        .baseline
        .expect("signed metrics must activate the baseline");
    (spans[0].h as f64, baseline as f64)
}

/// Pure comparator over the signed cases. Every case is checked on box height;
/// lineHeight-unset cases are additionally checked on baseline.
pub fn check(metrics: &PerEm, case_list: &[Case], tolerance: f64) -> CheckResult {
    let mut violations = Vec::new();
    let mut baseline_checked = 0;
    for case in case_list {
        let (box_px, baseline_px) = computed(metrics, case);
        if (box_px - case.height_px).abs() > tolerance {
            violations.push(Violation {
                font_px: case.font_px,
                sample: case.sample.clone(),
                field: "height_px",
                got: box_px,
                want: case.height_px,
            });
        }
        baseline_checked += 1;
        if (baseline_px - case.baseline_px).abs() > tolerance {
            violations.push(Violation {
                font_px: case.font_px,
                sample: case.sample.clone(),
                field: "baseline_px",
                got: baseline_px,
                want: case.baseline_px,
            });
        }
    }
    CheckResult {
        checked: case_list.len(),
        baseline_checked,
        violations,
    }
}

pub fn find_case<'a>(
    case_list: &'a [Case],
    px: i64,
    sample: &str,
    line_height: Option<f64>,
) -> &'a Case {
    case_list
        .iter()
        .find(|c| c.font_px == px && c.sample == sample && c.line_height == line_height)
        .expect("signed case must exist")
}
