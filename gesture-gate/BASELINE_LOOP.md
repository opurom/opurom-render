# Gesture gate baseline loop

This is an additive closed-loop baseline fixture for the six standalone
gesture gates. It does not edit shared CI files, IR, the native ABI, or
renderer sources.

For `perf`, `bounce`, `semantics`, `nested`, `parallel` and `advanced`, the
loop proves:

1. `--write-baseline` writes a usable baseline;
2. the same report compares cleanly against that baseline;
3. an enlarged metric triggers a regression `FAIL`;
4. a missing baseline is `ERROR` (exit `2`), not a silent PASS.

Each gate uses its own budget for the comparison: exact constants stay equal
and bounded counts may not exceed `baseline * 2 + 8`.

The signed rotation metric is an exact Flutter-derived constant of `-1571`
milli-radians, so a two-turn value cannot pass by exploiting the older
one-sided bound.

## Usage

```sh
# Pass mode: all six baseline writes and comparisons succeed.
node gesture-gate/gesture-baseline-loop.mjs --self-test pass

# Fault modes: the injected fault is detected, so the tool exits 1.
node gesture-gate/gesture-baseline-loop.mjs --self-test degrade
node gesture-gate/gesture-baseline-loop.mjs --self-test missing

# Fault matrix: all three parallel-gate faults must be rejected.
node gesture-gate/gesture-baseline-loop.mjs --self-test parallel_faults

# Full matrix: 13 targeted injections across all six gates must be rejected.
node gesture-gate/gesture-baseline-loop.mjs --self-test gate_faults

# Baseline regression matrix: each metric is pushed beyond `baseline*2+8`.
node gesture-gate/gesture-baseline-loop.mjs --self-test baseline_faults

# Default mode is pass.
node gesture-gate/gesture-baseline-loop.mjs
```

Output schema: `opurom.gesture.baseline.loop/1`. Exit codes: `0` PASS, `1`
FAIL, `2` usage or IO error. The source pin is
`flutter-3.50.0:ab2912035e160d9cad6306df307576da6128ebdb`.
