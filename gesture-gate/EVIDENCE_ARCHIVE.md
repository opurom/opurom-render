# Joint gesture evidence archive

This is an additive archive fixture. It does not edit shared CI, IR, ABI or
renderer sources. It packages the joint gesture gate's raw evidence into fixed
JSON files and verifies them with SHA-256.

Flutter 3.50.0 source provenance (`ab2912035e160d9cad6306df307576da6128ebdb`):

- `packages/flutter/lib/src/gestures/arena.dart:117-129,157-221,226-292`:
  per-pointer arenas, sweep/hold/release and exactly-one callback guarantees.
- `packages/flutter/lib/src/gestures/team.dart:41-93`: team captain and
  combining-member semantics.
- `packages/flutter/lib/src/gestures/binding.dart:96-112,145-175,390-405`:
  per-device event batching and frame-sampled dispatch.
- `packages/flutter/lib/src/gestures/velocity_tracker.dart:142-145,179-220`:
  bounded 20-sample, 100 ms horizon.
- `packages/flutter/lib/src/gestures/constants.dart:65`: touch slop 18 px.

## Archived files

Given `--out <dir>`, the archiver writes:

- `joint.json` — `opurom.gesture.joint.evidence/1` summary.
- `perf.json` — canonical `opurom.gesture.gate/1` metrics.
- `fairness.json` — canonical seven-case fairness evidence.
- `bounce.json` — canonical iOS bounce physics metrics.
- `replay.json` — `opurom.gesture.replay/1` canonical case/order projection.
- `manifest.json` — `opurom.gesture.manifest/1` with sorted-key JSON, byte
  count and SHA-256 for every file.
- `manifest.json.sha256` — self-hash of the manifest.

JSON is serialized with sorted object keys and one trailing newline so the
same evidence always produces the same manifest hash.

## Verdict and exit codes

- `PASS` — every file exists, byte count and SHA-256 match, and the replay
  document re-hashes identically. Exit `0`.
- `FAIL` — hash mismatch, size mismatch, missing file, or replay drift. Exit `1`.
- `ERROR` — usage, missing input, invalid JSON or subprocess failure. Exit `2`.

```sh
# Create an archive from fresh joint evidence.
node gesture-gate/gesture-evidence-archive.mjs --out artifacts/gesture

# Verify an existing archive.
node gesture-gate/gesture-evidence-archive.mjs --verify artifacts/gesture

# Self tests: PASS, tamper and missing evidence.
node gesture-gate/gesture-evidence-archive.mjs --self-test pass
node gesture-gate/gesture-evidence-archive.mjs --self-test tamper
node gesture-gate/gesture-evidence-archive.mjs --self-test missing
```

CI wiring is deliberately outside this additive patch; an existing check can
invoke the archiver when the lanes agree to adopt it.
