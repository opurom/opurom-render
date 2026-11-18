# Gesture archive freshness gate

This is an additive gate over the gesture evidence archive. It does not edit
shared CI files, IR, the native ABI, or renderer sources.

The archiver is deterministic: two clean builds of identical evidence may have
the same manifest hash. Freshness therefore cannot be proved from a different
hash. This gate checks the properties that a relabelled stale archive cannot
satisfy:

- the source pin and every gate document schema agree across two builds;
- every gate evidence source has a `<name>_gate_status` check in the joint
  verdict and a `<name>_status` field in the archived documents;
- the joint verdict is terminal (`PASS` or `N/A`);
- the manifest self-hash is a 64-hex SHA-256;
- a stale document copied from the previous archive is rejected by the
  archiver's `--verify` path;
- a shortened verdict chain is rejected by the consistency comparison.

Flutter 3.50.0 source pin:
`flutter-3.50.0:ab2912035e160d9cad6306df307576da6128ebdb`.

## Usage

```sh
# Compare two independently built archives.
node gesture-gate/gesture-archive-freshness.mjs \
  --current artifacts/gesture-current --previous artifacts/gesture-previous

# Self tests: PASS, stale document relabel and shortened verdict chain.
node gesture-gate/gesture-archive-freshness.mjs --self-test pass
node gesture-gate/gesture-archive-freshness.mjs --self-test relabel
node gesture-gate/gesture-archive-freshness.mjs --self-test stale
```

Output schema: `opurom.gesture.archive.freshness/1`. Exit codes: `0` PASS,
`1` FAIL, `2` usage or IO error.
