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
    let on = span(json!({"value":"Hello\u{4e16}\u{754c}","fontPx":24}), true);
    assert!(on.contains("\"baseline\":22"), "baseline present: {on}");
    assert!(on.contains("\"h\":28"), "metrics box height: {on}");
    // No metrics in theme -> no baseline key, legacy behaviour.
    let off = span(json!({"value":"Hello\u{4e16}\u{754c}","fontPx":24}), false);
    assert!(
        !off.contains("baseline"),
        "no metrics -> no baseline key: {off}"
    );
    // Metrics but no fontPx -> no baseline.
    let nopx = span(json!({"value":"Hello\u{4e16}\u{754c}"}), true);
    assert!(
        !nopx.contains("baseline"),
        "no fontPx -> no baseline: {nopx}"
    );
}

#[test]
fn mixed_script_run_has_one_baseline() {
    let text = doc(
        json!({"value":"Hello\u{4e16}\u{754c}gyjp","fontPx":24}),
        true,
    );
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
fn line_height_multiplier_changes_box_and_baseline() {
    let two = span(json!({"value":"Hi","fontPx":24,"lineHeight":2.0}), true);
    assert!(two.contains("\"h\":48"), "2.0 * 24 = 48: {two}");
    // Proportional leading split: round(48 * 928/(928+244)) = 38.
    assert!(
        two.contains("\"baseline\":38"),
        "baseline scales with lineHeight: {two}"
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
        json!({"value":"\u{4e16}\u{754c}","fontPx":24,"baselineKind":"ideographic"}),
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

#[test]
fn leading_distribution_even_matches_half_leading() {
    let mut props = json!({"value":"Hi","fontPx":24,"lineHeight":2.0});
    let proportional = span(props.clone(), true);
    props["leadingDistribution"] = json!("even");
    let even = span(props, true);
    // Roboto metrics 928/244 per em at px 24: ascent 22, descent 6.
    // lineHeight 2.0 -> 48 px box, natural ascent+descent = 28, leading = 20.
    // proportional baseline = 22 + round(20 * 928/1172) = 38.
    // even baseline = 22 + 20/2 = 32 (Flutter TextLeadingDistribution.even,
    // engine 371f772, lib/ui/text.dart:1445-1467).
    assert!(proportional.contains("\"baseline\":38"), "{proportional}");
    assert!(even.contains("\"baseline\":32"), "{even}");
    assert_ne!(proportional, even);
}

#[test]
fn leading_distribution_is_opt_in_and_validated() {
    let bad = doc(
        json!({"value":"Hi","fontPx":24,"leadingDistribution":"center"}),
        true,
    );
    assert!(
        parse_ir(&bad).is_err(),
        "unknown leading distribution must fail"
    );
}
