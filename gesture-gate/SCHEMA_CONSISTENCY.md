# Gesture evidence schema consistency gate

This is an additive gate over the gesture evidence archive. It does not edit
shared CI files, IR, the native ABI, or renderer sources.

The archiver writes nine evidence documents plus a manifest and its self-hash.
A later lane can add a new evidence source; this gate catches the case where
the new document is written with a stale schema, a different Flutter pin, or
is not wired into the joint verdict and the replay projection.

Flutter 3.50.0 source pin: `ab2912035e160d9cad6306df307576da6128ebdb`.

## Checks

- `manifest_file_set`: the manifest lists exactly the expected nine documents.
- `files_present`: every expected file and `manifest.json.sha256` exists.
- `schema_versions`: each document carries its expected `/1` schema.
- `source_pin`: every document uses the same Flutter source string.
- `joint_evidence_parity`: every gate evidence source has a `<name>_gate_status`
  check in the joint verdict.
- `replay_projection_complete`: every gate evidence source has a `<name>_order`
  projection in `replay.json`.

The verdict is `opurom.gesture.consistency/1` JSON. `PASS` exits `0`, `FAIL`
exits `1`, usage or IO errors exit `2`.

```sh
# Validate an existing archive.
node gesture-gate/gesture-schema-consistency.mjs --archive artifacts/gesture

# Self tests: PASS, schema drift, source drift and missing evidence.
node gesture-gate/gesture-schema-consistency.mjs --self-test pass
node gesture-gate/gesture-schema-consistency.mjs --self-test schema
node gesture-gate/gesture-schema-consistency.mjs --self-test source
node gesture-gate/gesture-schema-consistency.mjs --self-test missing
```
