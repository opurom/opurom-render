# Gesture gate end-to-end regression

This script is an additive regression harness for the gesture gate stack. It
edits no shared CI file, IR field, native ABI entry or renderer source.

It drives the real entry points only through their public command line, so a
regression in any layer of the stack is observable as a non-zero exit code.

## Pipeline

1. **Freeze a baseline** — `gesture-fairness-perf-gate.mjs --write-baseline`
   stores the joint fairness + performance evidence.
2. **Produce raw evidence** — `gesture-fairness-perf-gate.mjs --baseline --raw`
   compares the fresh run against that baseline and emits the raw joint
   document.
3. **Archive** — `gesture-evidence-archive.mjs --out --joint` writes the
   canonical JSON set plus the SHA-256 manifest. The set now carries the
   performance, fairness, iOS bounce, semantics and nested scroll evidence.
4. **Verify the manifest** — `gesture-evidence-archive.mjs --verify` re-hashes
   every archived file and the manifest itself.
5. **Replay the verdict** — the joint gate runs again; the replayed status,
   check list and performance metrics must be byte-identical to the archived
   evidence.
6. **Archive determinism** — a second archive of the replayed evidence must
   produce the same manifest hash.
7. **Injected faults** — a tampered `fairness.json` and a removed `perf.json`
   must both be rejected with `FAIL`.

Flutter 3.50.0 source anchors (`ab2912035e160d9cad6306df307576da6128ebdb`):

- `packages/flutter/lib/src/gestures/arena.dart:117-129`: one arena per
  pointer id, so fairness evidence is per-pointer and independent.
- `packages/flutter/lib/src/gestures/arena.dart:157-221`: sweep, hold and
  release make the winner a function of join order, which is what the replay
  comparison pins down.
- `packages/flutter/lib/src/gestures/arena.dart:226-292`: accept/reject run
  exactly once per member, so the callback set is stable across replays.
- `packages/flutter/lib/src/gestures/velocity_tracker.dart:142-145,179-220`:
  bounded sample window keeps the performance metrics reproducible.

## Checks and exit codes

The report is `opurom.gesture.e2e/1` JSON on stdout.

| check | meaning |
| --- | --- |
| `joint_evidence_status` | the baseline-compared joint verdict is `PASS` |
| `manifest_verified` | the archiver verifies the SHA-256 manifest |
| `manifest_self_hash` | `manifest.json` matches `manifest.json.sha256` |
| `archive_files_present` | all seven archived documents exist |
| `bounce_evidence_stable` | the archived bounce metrics match the joint verdict |
| `semantics_evidence_stable` | the archived semantics metrics and digest match the joint verdict |
| `nested_evidence_stable` | the archived nested scroll metrics match the joint verdict |
| `replay_verdict_stable` | the replayed verdict equals the archived verdict |
| `archive_deterministic` | two archives share one manifest hash |
| `tamper_detected` | a modified archive is rejected |
| `missing_detected` | an incomplete archive is rejected |

- `PASS` — every check passed. Exit `0`.
- `FAIL` — at least one check failed. Exit `1`.
- `ERROR` — usage, missing tool or a subprocess failure. Exit `2`.

## Usage

```sh
# Run the full pipeline and keep the archive.
node gesture-gate/gesture-e2e-regression.mjs --out artifacts/gesture

# Self tests: PASS, tamper and missing evidence.
node gesture-gate/gesture-e2e-regression.mjs --self-test pass
node gesture-gate/gesture-e2e-regression.mjs --self-test tamper
node gesture-gate/gesture-e2e-regression.mjs --self-test missing
```

The `tamper` and `missing` self tests exit `1` on purpose: they report the
verdict of the injected fault, exactly like `gesture-evidence-archive.mjs`.

CI wiring stays outside this additive patch; an existing check can invoke this
script once the lanes agree to adopt it.
