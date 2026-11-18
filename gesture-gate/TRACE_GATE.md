# Gesture test anchor trace gate

This is an additive gate over the gesture test suite. It does not edit shared
CI files, IR, the native ABI, or renderer sources.

Every gesture test and the two gesture source files document their Flutter
source anchors as `file.dart:start-end` comments. This gate extracts those
anchors and checks that the referenced file exists in the pinned Flutter
checkout and that every line is within the file. It catches comment drift
after an upgrade, on both the test side and the implementation side.

The checkout is the local read-only mirror at
`/Users/mac/work/opurom/flutter-golden`, pinned at
`ab2912035e160d9cad6306df307576da6128ebdb` (the `3.50.0` tag). The gate never
reads a network.

Output schema: `opurom.gesture.trace/1`. `PASS` exits `0`, `FAIL` exits `1`,
usage or IO errors exit `2`.

```sh
# Validate every anchor in the pinned checkout.
node gesture-gate/gesture-trace-gate.mjs --flutter /path/to/flutter

# Self tests: PASS, and a drifted checkout must be rejected.
node gesture-gate/gesture-trace-gate.mjs --self-test pass
node gesture-gate/gesture-trace-gate.mjs --self-test drift
```
