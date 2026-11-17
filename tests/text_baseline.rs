//! Text baseline alignment (TEXT_BASELINE_PROPOSAL.md §3.1, signed 11-17):
//! opt-in `theme.verticalMetrics`, `Text.props.lineHeight` and `baselineKind`.
//! Absent keys keep byte-identical geometry, span JSON and goldens.

use opurom_render::{
    digest, digest_hex, layout, parse_ir, text_spans, text_spans_for, Renderer, StateView,
};
use serde_json::json;

const COUNTER: &str = "../opurom-examples/golden/counter.opuir.json";

fn doc(props: serde_json::Value, with_metrics: bool) -> String {
    let mut theme = json!({"mode":"light","accent":"#097FEB","radius":0});
    if with_metrics {
        theme["fontFamily"] = json!("Roboto");
        theme["verticalMetrics"] = json!({
            "Roboto": {"ascent": 928, "descent": 244, "leading": 0}
        });
    }
    json!({
        "ir":"0.1.0","app":"BL","state":[],
        "tree":{"id":0,"kind":"Column","props":{},"children":[
            {"id":1,"kind":"Text","props":props,"children":[],"events":{}}
        ],"events":{}},
        "theme":theme
    })
    .to_string()
}

fn span(props: serde_json::Value, with_metrics: bool) -> String {
    let text = doc(props, with_metrics);
    let ir = parse_ir(&text).unwrap();
    text_spans_for(&ir, &StateView::from_ir(&ir), 400, 200)
}

#[test]
fn baseline_activates_only_with_fontpx_and_matching_family_metrics() {
    // With metrics + fontPx: ascent round(928*24/1024)=22, descent round(244*24/1024)=6,
    // box=28, baseline=rect.y+22=22.
    let on = span(json!({"value":"Hello世界","fontPx":24}), true);
    assert!(on.contains("\"baseline\":22"), "baseline present: {on}");
    assert!(on.contains("\"h\":28"), "metrics box height: {on}");
    // No metrics in theme -> no baseline key, legacy behaviour.
    let off = span(json!({"value":"Hello世界","fontPx":24}), false);
    assert!(
        !off.contains("baseline"),
        "no metrics -> no baseline key: {off}"
    );
    // Metrics but no fontPx -> no baseline.
    let nopx = span(json!({"value":"Hello世界"}), true);
    assert!(
        !nopx.contains("baseline"),
        "no fontPx -> no baseline: {nopx}"
    );
}

#[test]
fn mixed_script_run_has_one_baseline() {
    let text = doc(json!({"value":"Hello世界gyjp","fontPx":24}), true);
    let ir = parse_ir(&text).unwrap();
    let state = StateView::from_ir(&ir);
    let spans = text_spans(&layout(&ir, &state, 400, 200));
    assert_eq!(spans.len(), 1, "one span for the whole run");
    assert_eq!(
        spans[0].baseline,
        Some(22),
        "one baseline, no per-script split"
    );
}

#[test]
fn line_height_multiplier_changes_box_baseline_stays() {
    let two = span(json!({"value":"Hi","fontPx":24,"lineHeight":2.0}), true);
    assert!(two.contains("\"h\":48"), "2.0 * 24 = 48: {two}");
    assert!(
        two.contains("\"baseline\":22"),
        "baseline independent of lineHeight: {two}"
    );
    // kTextHeightNone (0) uses the metrics box (ascent+descent+leading).
    let none = span(json!({"value":"Hi","fontPx":24,"lineHeight":0}), true);
    assert!(
        none.contains("\"h\":28"),
        "lineHeight 0 -> metrics box: {none}"
    );
}

#[test]
fn opt_in_span_json_is_byte_pinned() {
    let text = doc(json!({"value":"Hi","fontPx":24}), true);
    let ir = parse_ir(&text).unwrap();
    let json = text_spans_for(&ir, &StateView::from_ir(&ir), 400, 200);
    assert_eq!(
        format!("{:016x}", digest(json.as_bytes())),
        "90eb179457e5a2c8"
    );
}

#[test]
fn invalid_new_values_are_rejected() {
    assert!(parse_ir(&doc(
        json!({"value":"x","fontPx":24,"lineHeight":3.1}),
        true
    ))
    .is_err());
    assert!(parse_ir(&doc(json!({"value":"x","fontPx":24,"lineHeight":-1}), true)).is_err());
    assert!(parse_ir(&doc(
        json!({"value":"x","fontPx":24,"baselineKind":"grid"}),
        true
    ))
    .is_err());
    // Out-of-range metric.
    let mut t: serde_json::Value = serde_json::from_str(&doc(json!({"value":"x"}), true)).unwrap();
    t["theme"]["verticalMetrics"]["Roboto"]["ascent"] = json!(2049);
    assert!(parse_ir(&t.to_string()).is_err(), "ascent > 2048 rejected");
}

#[test]
fn baseline_kind_ideographic_is_wire_parity() {
    let text = doc(
        json!({"value":"世界","fontPx":24,"baselineKind":"ideographic"}),
        true,
    );
    let ir = parse_ir(&text).unwrap();
    let state = StateView::from_ir(&ir);
    let s = &text_spans(&layout(&ir, &state, 400, 200))[0];
    // Accepted and resolves through the same alphabetic baseline (documented limit).
    assert_eq!(s.baseline, Some(22));
}

#[test]
fn default_path_keeps_all_pinned_goldens() {
    let text = std::fs::read_to_string(COUNTER).unwrap();
    let ir = parse_ir(&text).unwrap();
    let state = StateView::from_ir(&ir);
    assert_eq!(
        digest_hex(&Renderer::new(320, 240).render(&ir, &state).pixels),
        "8d45f412996a99c6"
    );
    assert_eq!(
        digest_hex(&Renderer::new(800, 600).render(&ir, &state).pixels),
        "318beb1dd337bc4e"
    );
    // Default span (no metrics) emits no baseline key; the pinned span golden
    // itself is owned and guarded by tests/text_span.rs.
    let plain = span(json!({"value":"plain","size":20}), false);
    assert!(
        !plain.contains("baseline"),
        "default span has no baseline key: {plain}"
    );
}
