# Joint gesture fairness + performance gate

This is an additive gate. It does not edit shared CI files, IR, the native
ABI, renderer sources, or any file owned by another lane.

It combines two independent evidence sources into one machine-readable
verdict:

1. `multi_pointer_fairness` Rust tests (seven cases):
   - `simultaneous_downs_open_independent_arenas`
   - `same_frame_race_resolves_by_join_order`
   - `hold_delays_sweep_until_release`
   - `cancel_rolls_back_every_member_and_timer_state`
   - `single_winner_terminal_state_excludes_other_members`
   - `fairness_boundary_first_join_order_is_stable`
   - `multi_pointer_contest_is_deterministic_on_replay`
2. The `opurom.gesture.gate/1` report from `gesture-perf-gate.mjs`.
3. The `opurom.gesture.bounce.gate/1` report from `gesture-bounce-gate.mjs`
   (the iOS bounce scroll physics).

Flutter 3.50.0 source anchors (`ab2912035e160d9cad6306df307576da6128ebdb`):

- `packages/flutter/lib/src/gestures/arena.dart:117-129`: one arena per pointer.
- `packages/flutter/lib/src/gestures/arena.dart:157-221`: first-member sweep,
  hold and delayed release.
- `packages/flutter/lib/src/gestures/arena.dart:226-292`: accept/reject and
  one-callback-per-member guarantees.
- `packages/flutter/lib/src/gestures/team.dart:41-93`: team captain and combine
  semantics.

## Verdict and exit codes

Output schema: `opurom.gesture.joint/1`.

- `PASS` — all absolute checks pass and, when supplied, the baseline is
  comparable. Exit `0`.
- `N/A` — no baseline was supplied, so regression comparison is unavailable;
  absolute checks still passed. Exit `0`.
- `FAIL` — at least one absolute or baseline check failed. Exit `1`.
- `ERROR` — usage, missing file, invalid JSON, or a subprocess failure. Exit `2`.

The perf gate may itself report `N/A` (no perf baseline); the joint gate treats
that as valid perf evidence, not a failure.

## Usage

The bounce evidence is optional input: when it is not supplied the gate
generates it, and `bounce_gate_status` plus `bounce_machine_report` report
its verdict.

```sh
# Fresh evidence, absolute checks only (N/A when no baseline exists).
node gesture-gate/gesture-fairness-perf-gate.mjs

# Freeze a joint baseline.
node gesture-gate/gesture-fairness-perf-gate.mjs --write-baseline baseline.json

# Compare against a frozen baseline.
node gesture-gate/gesture-fairness-perf-gate.mjs --baseline baseline.json

# Reuse existing reports.
node gesture-gate/gesture-fairness-perf-gate.mjs \
  --perf perf.json --fairness fairness.json --bounce bounce.json \
  --baseline baseline.json

# Self tests: PASS, N/A and FAIL.
node gesture-gate/gesture-fairness-perf-gate.mjs --self-test pass
node gesture-gate/gesture-fairness-perf-gate.mjs --self-test na
node gesture-gate/gesture-fairness-perf-gate.mjs --self-test fail
```

CI wiring is intentionally outside this additive patch; an existing check can
invoke this script when the lanes agree to adopt it.
