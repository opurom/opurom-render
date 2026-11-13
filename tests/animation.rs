//! Animation renderer tests: deterministic page transitions, gesture progress,
//! fixed-seed particles, caller-supplied quality tiers and golden regressions.

use opurom_render::{
    choose_quality, digest_hex, parse_ir, Frame, GestureAnim, ParticleSystem, Quality, Renderer,
    StateValue, StateView, Transition, TransitionKind, ONE, PARTICLE_FIELD, PARTICLE_MAX_LIFE,
    TRANSITION_MS,
};
use std::{fs, path::PathBuf, process::Command};

fn solid_frame(width: u32, height: u32, pixel: [u8; 4]) -> Frame {
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    for _ in 0..width as usize * height as usize {
        pixels.extend_from_slice(&pixel);
    }
    Frame {
        width,
        height,
        pixels,
        dirty: Vec::new(),
    }
}

fn page_frames() -> (Frame, Frame) {
    (
        solid_frame(8, 4, [8, 16, 24, 255]),
        solid_frame(8, 4, [240, 230, 210, 255]),
    )
}

fn transition_sequence(kind: TransitionKind) -> (Vec<i64>, Vec<String>, Vec<u8>) {
    let (from, to) = page_frames();
    let mut transition = Transition::new(kind);
    let mut previous_tick = 0;
    let mut progress = Vec::new();
    let mut digests = Vec::new();
    for tick in [0, TRANSITION_MS / 2, TRANSITION_MS] {
        transition.tick(tick - previous_tick);
        previous_tick = tick;
        progress.push(transition.progress);
        let frame = transition.compose(&from, &to).unwrap();
        digests.push(digest_hex(&frame.pixels));
    }
    (progress, digests, to.pixels)
}

fn assert_transition_kind(kind: TransitionKind) {
    let (progress, digests, settled) = transition_sequence(kind);
    assert_eq!(progress, vec![0, ONE / 2, ONE]);
    assert_ne!(digests[0], digests[1], "{kind:?} must move");
    assert_ne!(digests[1], digests[2], "{kind:?} must keep moving");
    assert_ne!(digests[0], digests[2], "{kind:?} must change endpoints");

    let (repeat_progress, repeat_digests, _) = transition_sequence(kind);
    assert_eq!(progress, repeat_progress);
    assert_eq!(digests, repeat_digests);

    // An absolute tick from a fresh transition matches an accumulated sequence.
    let (from, to) = page_frames();
    let mut fresh = Transition::new(kind);
    assert_eq!(fresh.tick(TRANSITION_MS / 2), ONE / 2);
    assert_eq!(
        digest_hex(&fresh.compose(&from, &to).unwrap().pixels),
        digests[1]
    );
    assert_eq!(
        Transition {
            kind,
            progress: ONE,
        }
        .compose(&from, &to)
        .unwrap()
        .pixels,
        settled
    );
}

#[test]
fn slide_transition_is_deterministic_and_settles() {
    assert_transition_kind(TransitionKind::Slide);
}

#[test]
fn fade_transition_is_deterministic_and_settles() {
    assert_transition_kind(TransitionKind::Fade);
}

#[test]
fn scale_transition_is_deterministic_and_settles() {
    assert_transition_kind(TransitionKind::Scale);
}

#[test]
fn transition_ticks_are_monotonic_and_settle_at_1024() {
    for (name, kind) in [
        ("slide", TransitionKind::Slide),
        ("fade", TransitionKind::Fade),
        ("scale", TransitionKind::Scale),
    ] {
        assert_eq!(TransitionKind::parse(name), Some(kind));
        assert_eq!(kind.name(), name);
        let mut transition = Transition::new(kind);
        let mut previous = 0;
        for _ in 0..32 {
            let p = transition.tick(16);
            assert!((0..=ONE).contains(&p), "{name} left the fixed-point range");
            assert!(p >= previous, "{name} went backwards");
            previous = p;
        }
        assert_eq!(previous, ONE);
        assert_eq!(transition.tick(-16), ONE);
        assert_eq!(Transition::new(kind).tick(i64::MAX), ONE);
        assert_eq!(TransitionKind::parse("warp"), None);
    }
}

#[test]
fn transition_compose_rejects_mismatched_frames() {
    let from = solid_frame(4, 2, [0, 0, 0, 255]);
    let wrong_size = solid_frame(4, 3, [0, 0, 0, 255]);
    let err = Transition::new(TransitionKind::Fade)
        .compose(&from, &wrong_size)
        .unwrap_err();
    assert!(err.contains("same dimensions"), "unexpected error: {err}");

    let mut wrong_length = from.clone();
    wrong_length.pixels.pop();
    let err = Transition::new(TransitionKind::Fade)
        .compose(&from, &wrong_length)
        .unwrap_err();
    assert!(err.contains("wrong length"), "unexpected error: {err}");
}

#[test]
fn gesture_drag_sequences_produce_the_same_digest_sequences() {
    let (from, to) = page_frames();
    const GESTURES: [(i64, i64); 5] = [(0, 0), (120, 30), (-20, 10), (900, -5), (40, 40)];
    let sequence = || {
        let mut gesture = GestureAnim::new();
        let mut out = Vec::new();
        for (dx, dy) in GESTURES {
            let p = gesture.drag(dx, dy);
            assert!((0..=ONE).contains(&p));
            let frame = gesture.compose(&from, &to).unwrap();
            out.push((p, digest_hex(&frame.pixels)));
        }
        out
    };
    let first = sequence();
    let second = sequence();
    assert_eq!(first, second, "the same gesture sequence must repeat");
    assert!(first.windows(2).any(|w| w[0].1 != w[1].1));
    assert_eq!(first[0].0, 0);
    assert_eq!(first.last().unwrap().0, ONE);
    assert_eq!(GestureAnim::new().drag(ONE, 0), ONE);
}

fn particle_digest_sequence(seed: u64, count: usize, steps: usize) -> Vec<String> {
    let mut system = ParticleSystem::new(seed, count);
    let mut out = Vec::new();
    for _ in 0..steps {
        system.step();
        let mut frame = solid_frame(64, 48, [16, 20, 28, 255]);
        system.render_into(&mut frame).unwrap();
        out.push(digest_hex(&frame.pixels));
    }
    out
}

#[test]
fn particle_system_is_integer_bounded_and_seed_deterministic() {
    let first = particle_digest_sequence(0x5eed_1234, 12, 24);
    let second = particle_digest_sequence(0x5eed_1234, 12, 24);
    let different_seed = particle_digest_sequence(0x5eed_5678, 12, 24);
    assert_eq!(first, second, "identical seeds must be byte-identical");
    assert_ne!(first, different_seed, "different seeds must change frames");
    assert!(
        first.windows(2).any(|w| w[0] != w[1]),
        "particles must move"
    );

    let mut system = ParticleSystem::new(99, 9);
    assert_eq!(system.particles.len(), 9);
    for _ in 0..200 {
        system.step();
        for p in &system.particles {
            assert!((0..PARTICLE_FIELD).contains(&p.x));
            assert!((0..PARTICLE_FIELD).contains(&p.y));
            assert!(p.vx.abs() <= 8 && p.vy.abs() <= 8);
            assert!((1..=PARTICLE_MAX_LIFE).contains(&p.life));
        }
    }
    let mut bad_frame = solid_frame(64, 48, [0, 0, 0, 255]);
    bad_frame.pixels.pop();
    assert!(system.render_into(&mut bad_frame).is_err());
}

#[test]
fn quality_degradation_matches_the_budget_table() {
    let cases = [
        (
            -1,
            8,
            true,
            Quality {
                particle_count: 2,
                effects_enabled: false,
            },
        ),
        (
            0,
            8,
            true,
            Quality {
                particle_count: 2,
                effects_enabled: false,
            },
        ),
        (
            0,
            8,
            false,
            Quality {
                particle_count: 2,
                effects_enabled: false,
            },
        ),
        (
            3,
            8,
            true,
            Quality {
                particle_count: 2,
                effects_enabled: false,
            },
        ),
        (
            4,
            8,
            true,
            Quality {
                particle_count: 4,
                effects_enabled: false,
            },
        ),
        (
            7,
            8,
            true,
            Quality {
                particle_count: 4,
                effects_enabled: false,
            },
        ),
        (
            8,
            8,
            true,
            Quality {
                particle_count: 6,
                effects_enabled: false,
            },
        ),
        (
            15,
            8,
            true,
            Quality {
                particle_count: 6,
                effects_enabled: false,
            },
        ),
        (
            16,
            8,
            true,
            Quality {
                particle_count: 8,
                effects_enabled: true,
            },
        ),
        (
            33,
            8,
            true,
            Quality {
                particle_count: 8,
                effects_enabled: true,
            },
        ),
        (
            16,
            8,
            false,
            Quality {
                particle_count: 8,
                effects_enabled: false,
            },
        ),
    ];
    for (budget, count, effects, expected) in cases {
        assert_eq!(
            choose_quality(budget, count, effects),
            expected,
            "budget {budget}"
        );
    }
}

#[test]
fn quality_is_monotonic_as_the_budget_tightens() {
    let mut previous_count = 0;
    let mut previous_effects = false;
    for budget in 0..=24 {
        let quality = choose_quality(budget, 17, true);
        assert_eq!(quality, choose_quality(budget, 17, true));
        assert!(quality.particle_count <= 17);
        assert!(quality.particle_count >= previous_count);
        if previous_effects {
            assert!(quality.effects_enabled);
        }
        previous_count = quality.particle_count;
        previous_effects = quality.effects_enabled;
    }
    assert_eq!(choose_quality(24, 17, false).effects_enabled, false);
}

const CLI_IR: &str = r##"{
  "ir": "0.1", "app": "TransitionProof", "state": [],
  "tree": { "id": 0, "kind": "Column", "props": { "gap": 4 },
    "children": [
      { "id": 1, "kind": "Container", "props": { "width": 60, "height": 40, "animate": "ease" },
        "children": [], "events": {} },
      { "id": 2, "kind": "Container", "props": { "width": 60, "height": 40, "animate": "spring" },
        "children": [], "events": {} }
    ], "events": {} },
  "theme": { "mode": "light", "accent": "#4F46E5", "radius": 4 }
}"##;

fn cli_output(ir_path: &PathBuf, ppm: &PathBuf, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_opurom-render"))
        .arg(ir_path)
        .args(["320", "240"])
        .arg(ppm)
        .args(args)
        .output()
        .unwrap()
}

fn cli_stdout(ir_path: &PathBuf, ppm: &PathBuf, args: &[&str]) -> String {
    let output = cli_output(ir_path, ppm, args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn cli_transition_flags_are_deterministic_and_validated() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let ir_path = dir.join("r5-transition.opuir.json");
    fs::write(&ir_path, CLI_IR).unwrap();
    let ppm = dir.join("r5-transition.ppm");

    for kind in ["slide", "fade", "scale"] {
        let at_16 = cli_stdout(&ir_path, &ppm, &["--transition", kind, "--tick", "16"]);
        assert_eq!(
            at_16,
            cli_stdout(&ir_path, &ppm, &["--transition", kind, "--tick", "16"])
        );
        assert_eq!(at_16.lines().count(), 2, "stdout shape must not change");
        assert!(at_16.starts_with("digest="));
        assert!(at_16.contains("dirty=1"));
        let at_0 = cli_stdout(&ir_path, &ppm, &["--transition", kind, "--tick", "0"]);
        let at_320 = cli_stdout(&ir_path, &ppm, &["--transition", kind, "--tick", "320"]);
        assert_ne!(at_0, at_320, "{kind} must change frames across ticks");
        assert_eq!(
            at_320,
            cli_stdout(&ir_path, &ppm, &[]),
            "{kind} must settle on the settled frame"
        );
    }

    let unknown = cli_output(&ir_path, &ppm, &["--transition", "warp", "--tick", "16"]);
    assert_eq!(unknown.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("unknown transition"));

    let missing_kind = cli_output(&ir_path, &ppm, &["--transition"]);
    assert_eq!(missing_kind.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&missing_kind.stderr).contains("--transition requires"));

    for bad_tick in ["-1", "4001", "abc"] {
        let out = cli_output(
            &ir_path,
            &ppm,
            &["--transition", "slide", "--tick", bad_tick],
        );
        assert_eq!(out.status.code(), Some(1), "{bad_tick} must fail cleanly");
        assert!(String::from_utf8_lossy(&out.stderr).contains("invalid tick"));
    }

    let missing_tick = cli_output(&ir_path, &ppm, &["--tick"]);
    assert_eq!(missing_tick.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&missing_tick.stderr).contains("--tick requires"));
}

#[test]
fn v0_1_golden_digests_are_unchanged_by_r5() {
    let counter = fs::read_to_string("../opurom-spec/counter.opuir.json").unwrap();
    let ir = parse_ir(&counter).unwrap();
    let state = StateView::from_ir(&ir);
    assert_eq!(
        digest_hex(&Renderer::new(320, 240).render(&ir, &state).pixels),
        "c87bf023a6c01f9d"
    );
    assert_eq!(
        digest_hex(&Renderer::new(800, 600).render(&ir, &state).pixels),
        "8cc3021fa80749b5"
    );
    let mut count_7 = state.clone();
    count_7.set("count", StateValue::Int(7));
    assert_eq!(
        digest_hex(&Renderer::new(800, 600).render(&ir, &count_7).pixels),
        "e7aad51c98d9cf21"
    );
}
