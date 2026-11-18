# Gesture performance gate (standalone)

This directory is an add-on gate. It does not edit shared CI files, IR, the
native ABI, or renderer sources.

## Gate stack

| script | purpose | doc |
| --- | --- | --- |
| `gesture-perf-gate.mjs` | machine-readable performance verdict | this file |
| `gesture-bounce-gate.mjs` | iOS bounce physics verdict | `BOUNCE_GATE.md` |
| `gesture-semantics-gate.mjs` | semantics and focus verdict | `SEMANTICS_GATE.md` |
| `gesture-nested-gate.mjs` | nested scroll coordination verdict | `NESTED_GATE.md` |
| `gesture-parallel-gate.mjs` | parallel pointer arena verdict | `PARALLEL_GATE.md` |
| `gesture-advanced-gate.mjs` | rotation and drag-start verdict | `ADVANCED_GATE.md` |
| `gesture-fairness-perf-gate.mjs` | joint fairness + performance + bounce + semantics + nested + parallel + advanced verdict | `JOINT_GATE.md` |
| `gesture-evidence-archive.mjs` | SHA-256 evidence archive | `EVIDENCE_ARCHIVE.md` |
| `gesture-e2e-regression.mjs` | end-to-end regression over the stack | `E2E_REGRESSION.md` |
| `gesture-schema-consistency.mjs` | archive schema/source parity | `SCHEMA_CONSISTENCY.md` |

## Why these metrics

Flutter 3.50.0 (`ab2912035e160d9cad6306df307576da6128ebdb`) pins the relevant
mechanisms:

- `packages/flutter/lib/src/gestures/binding.dart:96-112,145-175,390-405`:
  touch events are queued per device, sampled at frame boundaries, inactive
  resamplers are removed, and non-touch events are dispatched immediately.
- `packages/flutter/lib/src/gestures/arena.dart:117-129,254-292`: one arena
  per pointer; resolution walks the member list once, so the logical work is
  linear in member count.
- `packages/flutter/lib/src/gestures/velocity_tracker.dart:142-145,179-220`:
  tracking is bounded by 20 samples and a 100 ms horizon.
- `packages/flutter/lib/src/gestures/constants.dart:65`: `kTouchSlop` is 18 px.

Opurom values are logical-operation counts from a deterministic fixture, never
wall-clock timings. This keeps CI stable across machines while catching
algorithmic regressions.

## Output schema

`gesture-perf-report.json`:

```json
{
  "schema": "opurom.gesture.perf/1",
  "source": "flutter-3.50.0:ab2912035e160d9cad6306df307576da6128ebdb",
  "clock": "logical",
  "metrics": {
    "gesture_burst_small": 244,
    "gesture_burst_large": 484,
    "arena_slots_one": 62,
    "arena_slots_four": 248,
    "packet_loss_full": 92,
    "packet_loss_lossy": 83,
    "replay_bytes": 487,
    "slop_threshold": 18
  }
}
```

- `gesture_burst_small/large`: dispatch count for 120/240 frames with two
  active pointers.
- `arena_slots_one/four`: arena member visits for one/four pointers.
- `packet_loss_full/lossy`: dispatch count with and without dropped producer
  packets.
- `replay_bytes`: length of the deterministic event replay digest.
- `slop_threshold`: Flutter touch slop in logical pixels.

## Gate rules

1. Generate a report with the Rust fixture, or pass one with `--report`.
2. If no `--baseline` is supplied, emit `N/A` with exit code `0` after checking
   absolute limits. This is honest: no regression comparison is possible.
3. With a baseline, each metric must satisfy its absolute limit and the
   comparison bound. Scaling metrics allow `baseline * 2 + 8`; deterministic
   metrics (`replay_bytes`, `slop_threshold`) require exact equality.
4. `PASS` exits `0`; `FAIL` exits `1`; usage or IO errors exit `2`.
5. The machine-readable verdict is written to stdout as
   `opurom.gesture.gate/1` JSON.

```sh
# Generate a fresh report and compare against a frozen baseline.
node gesture-gate/gesture-perf-gate.mjs --baseline path/to/baseline.json

# Freeze the current report as a baseline.
node gesture-gate/gesture-perf-gate.mjs --write-baseline path/to/baseline.json

# Self tests: PASS, N/A and FAIL.
node gesture-gate/gesture-perf-gate.mjs --self-test pass
node gesture-gate/gesture-perf-gate.mjs --self-test na
node gesture-gate/gesture-perf-gate.mjs --self-test fail
```

CI integration can be added later by calling this script from an existing
check; that wiring is intentionally outside this additive patch.
