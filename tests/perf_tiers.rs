//! Measured auto-degrade tiers, caps, reports, and golden-frame stability.

use opurom_render::{
    apply_policy, digest_hex, parse_ir, perf_report, Frame, FrameBudget, FrameRate, Ir, PerfReport,
    PerfWindow, QualityTier, RenderConfig, Renderer, StateValue, StateView, VisualEffectKind,
    EFFECT_MAX_RADIUS, PARTICLE_MAX_COUNT, PROMOTION_WINDOWS, SETTLED_MS, WINDOW_FRAMES,
};
use std::fs;

const POLICY_IR: &str = r##"{
  "ir": "0.1.0", "app": "Perf", "state": [],
  "tree": {
    "id": 0, "kind": "Column", "props": { "padding": 4 },
    "children": [
      { "id": 1, "kind": "Container", "props": {
          "width": 80, "height": 50, "color": "#223344", "glass": true,
          "particles": { "count": 512, "seed": 7, "radius": 32, "alpha": 512 },
          "effects": [
            { "kind": "blur", "radius": 32 },
            { "kind": "glass", "radius": 9 },
            { "kind": "liquid", "radius": 8 },
            { "kind": "glow", "radius": 16 }
          ]
        }, "children": [], "events": {} }
    ], "events": {}
  },
  "theme": { "mode": "dark", "accent": "#097FEB", "radius": 4 }
}"##;

const GOLDEN_IR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../opurom-examples/golden/counter.opuir.json"
);

fn policy_ir() -> Ir {
    parse_ir(POLICY_IR).expect("policy IR parses")
}

fn push_window(budget: &mut FrameBudget, duration_ms: i64) -> QualityTier {
    let mut tier = budget.tier();
    for _ in 0..WINDOW_FRAMES {
        tier = budget.push(duration_ms);
    }
    tier
}

fn render_window(
    renderer: &mut Renderer,
    config: &mut RenderConfig,
    state: &StateView,
    budget: &mut FrameBudget,
    duration_ms: i64,
) -> Frame {
    let mut last = None;
    for _ in 0..WINDOW_FRAMES {
        last = Some(renderer.render_auto_at(config, state, SETTLED_MS, duration_ms, budget));
    }
    last.expect("one window renders at least one frame")
}

#[test]
fn budget_at_target_keeps_full_for_60_and_120fps() {
    let mut at_60 = FrameBudget::new(FrameRate::Hz60);
    assert_eq!(at_60.rate(), FrameRate::Hz60);
    assert_eq!(at_60.target_ms(), 16);
    assert_eq!(at_60.promote_ms(), 12);
    for _ in 0..4 {
        assert_eq!(push_window(&mut at_60, 16), QualityTier::Full);
    }
    assert_eq!(at_60.report().p95_ms, 16);

    let mut at_120 = FrameBudget::new(FrameRate::Hz120);
    assert_eq!(at_120.target_ms(), 8);
    assert_eq!(at_120.promote_ms(), 6);
    for _ in 0..4 {
        assert_eq!(push_window(&mut at_120, 8), QualityTier::Full);
    }
    assert_eq!(at_120.report().p95_ms, 8);
}

#[test]
fn slow_bursts_drop_one_tier_per_window() {
    let mut budget = FrameBudget::new(FrameRate::Hz60);
    assert_eq!(push_window(&mut budget, 30), QualityTier::Reduced);
    assert_eq!(push_window(&mut budget, 30), QualityTier::Minimal);
    assert_eq!(push_window(&mut budget, 30), QualityTier::Minimal);

    // With 20 samples, nearest-rank p95 is the 19th value: one outlier stays
    // below the threshold, while a genuine slow burst crosses it.
    let mut isolated = FrameBudget::new(FrameRate::Hz60);
    for _ in 0..WINDOW_FRAMES - 1 {
        assert_eq!(isolated.push(10), QualityTier::Full);
    }
    assert_eq!(isolated.push(30), QualityTier::Full);

    let mut at_120 = FrameBudget::new(FrameRate::Hz120);
    assert_eq!(push_window(&mut at_120, 16), QualityTier::Reduced);
    assert_eq!(push_window(&mut at_120, 16), QualityTier::Minimal);
}

#[test]
fn recovery_requires_consecutive_good_windows() {
    let mut budget = FrameBudget::new(FrameRate::Hz60);
    assert_eq!(push_window(&mut budget, 30), QualityTier::Reduced);
    assert_eq!(push_window(&mut budget, 30), QualityTier::Minimal);

    for _ in 0..PROMOTION_WINDOWS - 1 {
        assert_eq!(push_window(&mut budget, 10), QualityTier::Minimal);
    }
    assert_eq!(budget.good_windows(), PROMOTION_WINDOWS - 1);
    // 14 ms is neither slow nor under the 12 ms promotion threshold.
    assert_eq!(push_window(&mut budget, 14), QualityTier::Minimal);
    assert_eq!(budget.good_windows(), 0);
    for _ in 0..PROMOTION_WINDOWS - 1 {
        assert_eq!(push_window(&mut budget, 10), QualityTier::Minimal);
    }
    assert_eq!(push_window(&mut budget, 10), QualityTier::Reduced);
    assert_eq!(budget.good_windows(), 0);

    for _ in 0..PROMOTION_WINDOWS - 1 {
        assert_eq!(push_window(&mut budget, 10), QualityTier::Reduced);
    }
    assert_eq!(push_window(&mut budget, 10), QualityTier::Full);
}

#[test]
fn caps_are_monotonic_bounded_and_applied() {
    let caps = QualityTier::ALL.map(QualityTier::caps);
    assert_eq!(caps[0].particle_count_cap, PARTICLE_MAX_COUNT);
    assert_eq!(caps[1].particle_count_cap, PARTICLE_MAX_COUNT / 2);
    assert_eq!(caps[2].particle_count_cap, PARTICLE_MAX_COUNT / 8);
    assert_eq!(caps[0].effect_radius_cap, EFFECT_MAX_RADIUS);
    assert_eq!(caps[1].effect_radius_cap, EFFECT_MAX_RADIUS / 4);
    assert_eq!(caps[2].effect_radius_cap, EFFECT_MAX_RADIUS / 16);
    assert!(caps[0].expensive_effects);
    assert!(!caps[1].expensive_effects);
    assert!(!caps[2].expensive_effects);
    for pair in caps.windows(2) {
        assert!(pair[0].particle_count_cap >= pair[1].particle_count_cap);
        assert!(pair[0].effect_radius_cap >= pair[1].effect_radius_cap);
    }
    for cap in caps {
        assert!((0..=PARTICLE_MAX_COUNT).contains(&cap.particle_count_cap));
        assert!((0..=EFFECT_MAX_RADIUS).contains(&cap.effect_radius_cap));
    }

    let ir = policy_ir();
    let mut config = RenderConfig::new(ir.clone());
    apply_policy(QualityTier::Full, &mut config);
    assert_eq!(config.ir(), &ir);
    {
        let node = &config.ir().tree.children[0];
        assert_eq!(node.props.particles.unwrap().count, PARTICLE_MAX_COUNT);
        assert_eq!(node.props.effects.len(), 4);
        assert!(node.props.glass);
    }

    apply_policy(QualityTier::Reduced, &mut config);
    {
        let node = &config.ir().tree.children[0];
        assert_eq!(node.props.particles.unwrap().count, PARTICLE_MAX_COUNT / 2);
        assert_eq!(
            node.props
                .effects
                .iter()
                .map(|e| (e.kind, e.radius))
                .collect::<Vec<_>>(),
            vec![
                (VisualEffectKind::Blur, EFFECT_MAX_RADIUS / 4),
                (VisualEffectKind::Glow, EFFECT_MAX_RADIUS / 4)
            ]
        );
        assert!(!node.props.glass);
    }

    apply_policy(QualityTier::Minimal, &mut config);
    {
        let node = &config.ir().tree.children[0];
        assert_eq!(node.props.particles.unwrap().count, PARTICLE_MAX_COUNT / 8);
        assert_eq!(
            node.props
                .effects
                .iter()
                .map(|e| (e.kind, e.radius))
                .collect::<Vec<_>>(),
            vec![
                (VisualEffectKind::Blur, EFFECT_MAX_RADIUS / 16),
                (VisualEffectKind::Glow, EFFECT_MAX_RADIUS / 16)
            ]
        );
        assert!(!node.props.glass);
    }

    // Applying Full again rebuilds from the immutable baseline.
    apply_policy(QualityTier::Full, &mut config);
    assert_eq!(config.ir(), &ir);
}

#[test]
fn perf_report_uses_hand_computed_integer_math() {
    let mut budget = FrameBudget::new(FrameRate::Hz60);
    for _ in 0..WINDOW_FRAMES {
        budget.push(20);
    }
    assert_eq!(budget.tier(), QualityTier::Reduced);
    // 20 samples of 20 ms: p50=20, p95=20, 10_000*20/400=500 (50.0 fps).
    assert_eq!(
        budget.report(),
        PerfReport {
            p50_ms: 20,
            p95_ms: 20,
            fps_x10: 500,
            tier: QualityTier::Reduced,
        }
    );
    assert_eq!(perf_report(&budget.window()), budget.report());

    // Sorted [8..17]: nearest-rank p50 is the 5th value (12), p95 is the 10th
    // value (17), and 10_000*10/125=800 (80.0 fps).
    let hand = PerfWindow {
        samples: vec![8, 9, 10, 11, 12, 13, 14, 15, 16, 17],
        tier: QualityTier::Minimal,
    };
    assert_eq!(
        perf_report(&hand),
        PerfReport {
            p50_ms: 12,
            p95_ms: 17,
            fps_x10: 800,
            tier: QualityTier::Minimal,
        }
    );

    // Sorted [1..19,100]: p50=10, p95=19 (not the 100 ms outlier), and
    // trunc(200_000/290)=689 (68.9 fps).
    let mut samples: Vec<i64> = (1..=19).collect();
    samples.push(100);
    let outlier = PerfWindow {
        samples,
        tier: QualityTier::Full,
    };
    assert_eq!(
        perf_report(&outlier),
        PerfReport {
            p50_ms: 10,
            p95_ms: 19,
            fps_x10: 689,
            tier: QualityTier::Full,
        }
    );

    let zero = PerfWindow {
        samples: vec![0; WINDOW_FRAMES],
        tier: QualityTier::Full,
    };
    assert_eq!(
        perf_report(&zero),
        PerfReport {
            p50_ms: 0,
            p95_ms: 0,
            fps_x10: 0,
            tier: QualityTier::Full,
        }
    );
}

#[test]
fn state_machine_and_policy_are_deterministic() {
    let mut sequence = vec![16; WINDOW_FRAMES];
    sequence.extend(vec![30; WINDOW_FRAMES]);
    sequence.extend(vec![10; WINDOW_FRAMES * 2]);
    sequence.extend((0..7).map(|i| 8 + i as i64));

    let mut first = FrameBudget::new(FrameRate::Hz60);
    let mut second = FrameBudget::new(FrameRate::Hz60);
    for (index, duration_ms) in sequence.iter().copied().enumerate() {
        assert_eq!(
            first.push(duration_ms),
            second.push(duration_ms),
            "frame {index}"
        );
    }
    assert_eq!(first, second);
    assert_eq!(first.window(), second.window());
    assert_eq!(first.report(), second.report());

    let mut config_a = RenderConfig::new(policy_ir());
    let mut config_b = RenderConfig::new(policy_ir());
    for tier in QualityTier::ALL {
        let a = apply_policy(tier, &mut config_a);
        let b = apply_policy(tier, &mut config_b);
        assert_eq!(a, b, "tier {tier:?}");
    }
}

#[test]
fn auto_render_restores_full_complexity_after_recovery() {
    let ir = policy_ir();
    let state = StateView::from_ir(&ir);
    let full_digest = digest_hex(&Renderer::new(64, 48).pixels(&ir, &state));
    let mut config = RenderConfig::new(ir.clone());
    let mut budget = FrameBudget::new(FrameRate::Hz60);
    let mut renderer = Renderer::new(64, 48);

    let first_slow = render_window(&mut renderer, &mut config, &state, &mut budget, 30);
    assert_eq!(budget.tier(), QualityTier::Reduced);
    assert_ne!(digest_hex(&first_slow.pixels), full_digest);
    let minimal = render_window(&mut renderer, &mut config, &state, &mut budget, 30);
    assert_eq!(budget.tier(), QualityTier::Minimal);
    assert_ne!(digest_hex(&minimal.pixels), full_digest);
    assert_eq!(
        config.ir().tree.children[0].props.particles.unwrap().count,
        PARTICLE_MAX_COUNT / 8
    );

    for _ in 0..PROMOTION_WINDOWS {
        render_window(&mut renderer, &mut config, &state, &mut budget, 10);
    }
    assert_eq!(budget.tier(), QualityTier::Reduced);
    assert_eq!(
        config.ir().tree.children[0].props.particles.unwrap().count,
        PARTICLE_MAX_COUNT / 2
    );
    for _ in 0..PROMOTION_WINDOWS {
        render_window(&mut renderer, &mut config, &state, &mut budget, 10);
    }
    assert_eq!(budget.tier(), QualityTier::Full);
    assert_eq!(config.ir(), &ir);

    let recovered =
        Renderer::new(64, 48).render_auto_at(&mut config, &state, SETTLED_MS, 10, &mut budget);
    assert_eq!(digest_hex(&recovered.pixels), full_digest);
}

fn automatic_digest(ir: &Ir, state: &StateView, width: u32, height: u32) -> (String, String) {
    let mut config = RenderConfig::new(ir.clone());
    let mut budget = FrameBudget::new(FrameRate::Hz60);
    let mut renderer = Renderer::new(width, height);
    let mut frame = None;
    for _ in 0..WINDOW_FRAMES {
        frame = Some(renderer.render_auto_at(&mut config, state, SETTLED_MS, 10, &mut budget));
    }
    assert_eq!(budget.tier(), QualityTier::Full);
    assert_eq!(config.ir(), ir);
    assert_eq!(budget.report().p95_ms, 10);
    let baseline = digest_hex(&Renderer::new(width, height).pixels(ir, state));
    (baseline, digest_hex(&frame.unwrap().pixels))
}

#[test]
fn full_tier_without_slow_frames_keeps_three_golden_digests() {
    let source = fs::read_to_string(GOLDEN_IR).expect("canonical counter IR is present");
    let ir = parse_ir(&source).expect("canonical counter IR parses");
    let mut state = StateView::from_ir(&ir);

    state.set("count", StateValue::Int(0));
    let (small_baseline, small_auto) = automatic_digest(&ir, &state, 320, 240);
    assert_eq!(small_baseline, "8d45f412996a99c6");
    assert_eq!(small_auto, small_baseline);

    let (wide_baseline, wide_auto) = automatic_digest(&ir, &state, 800, 600);
    assert_eq!(wide_baseline, "318beb1dd337bc4e");
    assert_eq!(wide_auto, wide_baseline);

    state.set("count", StateValue::Int(7));
    let (seven_baseline, seven_auto) = automatic_digest(&ir, &state, 800, 600);
    assert_eq!(seven_baseline, "40073cb95c912d80");
    assert_eq!(seven_auto, seven_baseline);
}
