#!/usr/bin/env node
// Machine-readable trace of the Flutter source anchors used by the gesture
// test suite. Additive only: it never edits shared CI, IR, ABI or renderer
// sources.
//
// Usage:
//   node gesture-gate/gesture-trace-gate.mjs --flutter <dir>
//   node gesture-gate/gesture-trace-gate.mjs --self-test pass|drift
//   node gesture-gate/gesture-trace-gate.mjs --coverage <json>
//
// Exit codes: 0 PASS, 1 FAIL, 2 usage/IO error.
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';

const ROOT = resolve(dirname(new URL(import.meta.url).pathname), '..');
const SCHEMA = 'opurom.gesture.trace/1';
const COVERAGE_SCHEMA = 'opurom.gesture.trace.coverage/1';
const SOURCE = 'flutter-3.50.0:ab2912035e160d9cad6306df307576da6128ebdb';
const SOURCES = [
  'src/gestures.rs',
  'src/scroll_physics.rs',
];
const TESTS = [
  'advanced_gesture_parity.rs',
  'advanced_gesture_thresholds.rs',
  'bounce_scroll_physics.rs',
  'drag_start_behavior.rs',
  'drag_start_parity.rs',
  'gesture_conflict_team.rs',
  'gesture_extreme_sequences.rs',
  'host_conflict_projection.rs',
  'force_press_parity.rs',
  'gesture_focus_semantics.rs',
  'gesture_semantics_report.rs',
  'gesture_hit_path.rs',
  'gesture_jank_tolerance.rs',
  'gesture_nested_report.rs',
  'gesture_perf_baseline.rs',
  'gesture_perf_report.rs',
  'gesture_parallel_report.rs',
  'gesture_regression_parity.rs',
  'gesture_robustness.rs',
  'multi_pointer_fairness.rs',
  'multi_pointer_fields.rs',
  'multi_pointer_parity.rs',
  'multidrag_strategy.rs',
  'pointer_signal_resolver.rs',
  'converter_parity.rs',
  'eager_recognizer.rs',
  'gesture_details_parity.rs',
  'hit_target_geometry.rs',
  'drag_end_details.rs',
  'recognizer_registration.rs',
  'hit_test_behavior.rs',
  'detector_combination.rs',
  'long_press_move_update.rs',
  'multidrag_pointer_state.rs',
  'scrollable_config.rs',
  'dismissible_thresholds.rs',
  'drag_target_parity.rs',
  'nested_scroll_coordinator.rs',
  'nested_scroll_coordination.rs',
  'parallel_pointer_arenas.rs',
  'pointer_resampling.rs',
  'rotation_callback.rs',
  'scale_gesture.rs',
];
const PATTERN = /([a-z_]+\.dart):(\d+)(?:-(\d+))?/g;

function parseArgs(argv) {
  const args = { flutter: null, selfTest: null, coverage: null };
  for (let i = 2; i < argv.length; i += 1) {
    if (argv[i] === '--flutter') args.flutter = argv[++i];
    else if (argv[i] === '--self-test') args.selfTest = argv[++i];
    else if (argv[i] === '--coverage') args.coverage = argv[++i];
    else throw new Error(`unknown argument: ${argv[i]}`);
  }
  return args;
}

function collectAnchors() {
  const anchors = [];
  for (const name of TESTS) {
    const path = join(ROOT, 'tests', name);
    if (!existsSync(path)) continue;
    const text = readFileSync(path, 'utf8');
    for (const match of text.matchAll(PATTERN)) {
      anchors.push({
        test: name,
        file: match[1],
        start: Number(match[2]),
        end: match[3] ? Number(match[3]) : Number(match[2]),
      });
    }
  }
  for (const name of SOURCES) {
    const path = join(ROOT, name);
    if (!existsSync(path)) continue;
    const text = readFileSync(path, 'utf8');
    for (const match of text.matchAll(PATTERN)) {
      anchors.push({
        test: name,
        file: match[1],
        start: Number(match[2]),
        end: match[3] ? Number(match[3]) : Number(match[2]),
      });
    }
  }
  return anchors;
}

function evaluate(flutterDir) {
  const anchors = collectAnchors();
  const byFile = new Map();
  for (const anchor of anchors) {
    if (!byFile.has(anchor.file)) byFile.set(anchor.file, []);
    byFile.get(anchor.file).push(anchor);
  }
  const checks = [];
  const record = (check, verdict, detail) => {
    checks.push({ check, verdict, detail });
    return verdict === 'PASS';
  };
  let filesMissing = 0;
  let linesOutOfRange = 0;
  let linesChecked = 0;
  for (const [file, list] of byFile) {
    const path = join(flutterDir, 'packages', 'flutter', 'lib', 'src', 'gestures', file);
    const physicsPath = join(flutterDir, 'packages', 'flutter', 'lib', 'src', 'physics', file);
    const widgetsPath = join(flutterDir, 'packages', 'flutter', 'lib', 'src', 'widgets', file);
    const foundationPath = join(flutterDir, 'packages', 'flutter', 'lib', 'src', 'foundation', file);
    const renderingPath = join(flutterDir, 'packages', 'flutter', 'lib', 'src', 'rendering', file);
    const semanticsPath = join(flutterDir, 'packages', 'flutter', 'lib', 'src', 'semantics', file);
    const candidates = [path, physicsPath, widgetsPath, foundationPath, semanticsPath, renderingPath];
    const found = candidates.find((candidate) => existsSync(candidate));
    if (!found) {
      filesMissing += 1;
      record(`file:${file}`, 'FAIL', 'no candidate path exists');
      continue;
    }
    const lineCount = readFileSync(found, 'utf8').split('\n').length;
    for (const anchor of list) {
      linesChecked += 1;
      if (anchor.start < 1 || anchor.end > lineCount || anchor.start > anchor.end) {
        linesOutOfRange += 1;
      }
    }
    record(`file:${file}`, 'PASS', `lines=${lineCount} anchors=${list.length}`);
  }
  record('anchors_resolved', linesOutOfRange === 0 && filesMissing === 0 ? 'PASS' : 'FAIL',
    `checked=${linesOutOfRange === 0 ? linesChecked : 0} out_of_range=${linesOutOfRange} missing_files=${filesMissing}`);
  const failed = checks.some((item) => item.verdict === 'FAIL');
  return {
    status: failed ? 'FAIL' : 'PASS',
    checks,
    stats: {
      tests: TESTS.filter((name) => existsSync(join(ROOT, 'tests', name))).length
        + SOURCES.filter((name) => existsSync(join(ROOT, name))).length,
      anchors: anchors.length,
      files: byFile.size,
      lines_checked: linesOutOfRange === 0 ? linesChecked : 0,
      out_of_range: linesOutOfRange,
      missing_files: filesMissing,
    },
  };
}

function coverage(flutterDir, result) {
  const anchors = collectAnchors();
  const bySource = new Map();
  const byTest = new Map();
  for (const anchor of anchors) {
    bySource.set(anchor.file, (bySource.get(anchor.file) ?? 0) + 1);
    byTest.set(anchor.test, (byTest.get(anchor.test) ?? 0) + 1);
  }
  const sources = [...bySource.entries()]
    .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
    .map(([file, count]) => ({ file, anchors: count }));
  const tests = [...byTest.entries()]
    .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
    .map(([file, count]) => ({ file, anchors: count }));
  return {
    schema: COVERAGE_SCHEMA,
    status: result.status,
    source: SOURCE,
    stats: {
      ...result.stats,
      minimum_anchors_per_source: sources.length
        ? Math.min(...sources.map((item) => item.anchors))
        : 0,
      maximum_anchors_per_source: sources.length
        ? Math.max(...sources.map((item) => item.anchors))
        : 0,
    },
    sources,
    tests,
  };
}

function emit(result, detail) {
  console.log(JSON.stringify({
    schema: SCHEMA,
    status: result.status,
    detail,
    source: SOURCE,
    stats: result.stats,
    checks: result.checks,
  }, null, 2));
}

function main() {
  const args = parseArgs(process.argv);
  const flutterDir = args.flutter ?? '/Users/mac/work/opurom/flutter-golden';
  if (args.coverage) {
    const result = evaluate(flutterDir);
    const report = coverage(flutterDir, result);
    writeFileSync(resolve(args.coverage), `${JSON.stringify(report, null, 2)}\n`);
    console.log(`TRACE_COVERAGE=${resolve(args.coverage)}`);
    return result.status === 'PASS' ? 0 : 1;
  }
  if (args.selfTest) {
    const workspace = mkdtempSync(join(tmpdir(), 'opurom-gesture-trace-'));
    try {
      if (args.selfTest === 'pass') {
        const result = evaluate(flutterDir);
        emit(result, 'every anchor resolved against the pinned Flutter checkout');
        return result.status === 'PASS' ? 0 : 1;
      }
      if (args.selfTest === 'drift') {
        // The fault state points the gate at an empty checkout: every anchor
        // must be rejected.
        const empty = join(workspace, 'empty');
        mkdirSync(empty, { recursive: true });
        const result = evaluate(empty);
        const detected = result.status === 'FAIL';
        emit(detected ? result : { ...result, status: 'FAIL' },
          detected ? 'anchor drift was rejected' : 'anchor drift was not detected');
        return detected ? 1 : 0;
      }
      throw new Error(`unknown self-test state: ${args.selfTest}`);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  }
  const result = evaluate(flutterDir);
  emit(result, result.status === 'PASS'
    ? 'every gesture test anchor resolved against the pinned Flutter checkout'
    : 'some gesture test anchors are out of range');
  return result.status === 'PASS' ? 0 : 1;
}

try {
  process.exitCode = main();
} catch (err) {
  console.error(`ERROR ${err.message}`);
  process.exitCode = 2;
}
