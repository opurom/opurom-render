# Gesture module release report

This is an additive aggregate report. It does not edit shared CI files, IR,
the native ABI, or renderer sources.

`gesture-release-report.mjs` runs the final release-readiness checks and
writes an `opurom.gesture.release/1` JSON report. It records:

- the commit and file range against `origin/render/p0-parity`;
- trace gate and its drift fault;
- archive, consistency, freshness and e2e passes plus their fault states;
- pass/na/fail self-checks for all six standalone gates;
- all baseline loop modes, including its three fault matrices.

`PASS` exits `0`, `FAIL` exits `1`, usage or IO errors exit `2`.

```sh
# Write a machine-readable report.
node gesture-gate/gesture-release-report.mjs --out release.json

# Self tests: PASS, and a simulated fault must be reported as FAIL.
node gesture-gate/gesture-release-report.mjs --self-test pass
node gesture-gate/gesture-release-report.mjs --self-test fault
```

The source pin is `flutter-3.50.0:ab2912035e160d9cad6306df307576da6128ebdb`.
