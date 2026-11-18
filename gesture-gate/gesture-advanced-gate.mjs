#!/usr/bin/env node
// Standalone advanced gesture gate. It never edits shared CI files.
//
// Usage:
//   node gesture-gate/gesture-advanced-gate.mjs [--report <json>]
//     [--baseline <json>] [--write-baseline <json>]
//     [--self-test pass|na|fail]
//
// Exit codes: 0 PASS or N/A, 1 FAIL, 2 usage/IO error.
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';

const ROOT = resolve(dirname(new URL(import.meta.url).pathname), '..');
const REPORT_SCHEMA = 'opurom.gesture.advanced/1';
const GATE_SCHEMA = 'opurom.gesture.advanced.gate/1';
const METRICS = [
  'rotation_quarter_turn_milli',
  'rotation_horizontal_milli',
  'drag_down_origin_delta',
  'drag_start_origin_delta',
  'drag_threshold_px',
];
// The first four metrics are Flutter constants and must match exactly. The
// remaining ones are logical counts derived from the closed-form model and are
// only bounded, so a different libm rounding cannot fail the gate.
const LIMITS = {
  rotation_quarter_turn_milli: {
    exact: -1571,
    reason: 'the signed rotation is bounded to one full turn',
  },
  rotation_horizontal_milli: {
    exact: 0,
    reason: 'a horizontal move has zero rotation',
  },
  drag_down_origin_delta: {
    min: 1,
    max: 4096,
    reason: 'the down origin produces a non-zero first delta',
  },
  drag_start_origin_delta: {
    exact: 0,
    reason: 'the arena-win origin produces a zero first delta',
  },
  drag_threshold_px: {
    exact: 18,
    reason: 'Flutter kTouchSlop at constants.dart:65',
  },
};
// Regression budget for the derived counts: 2x + 8 logical steps.
const BASELINE_FACTOR = 2;
const BASELINE_SLACK = 8;

function parseArgs(argv) {
  const out = { report: null, baseline: null, writeBaseline: null, selfTest: null };
  for (let i = 2; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--report') out.report = argv[++i];
    else if (arg === '--baseline') out.baseline = argv[++i];
    else if (arg === '--write-baseline') out.writeBaseline = argv[++i];
    else if (arg === '--self-test') out.selfTest = argv[++i];
    else throw new Error(`unknown argument: ${arg}`);
  }
  return out;
}

function validateReport(text, label) {
  let value;
  try { value = JSON.parse(text); } catch (err) { throw new Error(`${label} is not JSON: ${err.message}`); }
  if (value?.schema !== REPORT_SCHEMA) throw new Error(`${label} has unsupported schema`);
  if (value?.clock !== 'logical') throw new Error(`${label} must use a logical clock`);
  const metrics = value?.metrics;
  if (!metrics || typeof metrics !== 'object') throw new Error(`${label} lacks metrics`);
  for (const key of METRICS) {
    if (!Number.isFinite(metrics[key])) throw new Error(`${label}.metrics.${key} is not finite`);
  }
  return value;
}

function generateReport() {
  const dir = mkdtempSync(join(tmpdir(), 'opurom-gesture-bounce-'));
  try {
    const child = spawnSync('cargo', ['test', '--test', 'gesture_advanced_report', '--quiet', '--', '--nocapture'], {
      cwd: ROOT,
      encoding: 'utf8',
      env: { ...process.env, GESTURE_ADVANCED_REPORT_DIR: dir },
      timeout: 120_000,
      maxBuffer: 4 * 1024 * 1024,
    });
    const output = `${child.stdout ?? ''}${child.stderr ?? ''}`;
    if (child.status !== 0) throw new Error(`report generation failed: ${output.slice(-800)}`);
    const path = join(dir, 'gesture-advanced-report.json');
    if (!existsSync(path)) throw new Error('report generation did not write gesture-advanced-report.json');
    return validateReport(readFileSync(path, 'utf8'), 'generated report');
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

function evaluate(report, baseline) {
  const results = [];
  for (const key of METRICS) {
    const actual = report.metrics[key];
    const limit = LIMITS[key];
    let verdict = 'PASS';
    let detail = `value=${actual}`;
    if (limit.exact !== undefined && actual !== limit.exact) {
      verdict = 'FAIL';
      detail += ` expected=${limit.exact}`;
    } else if (limit.min !== undefined && actual < limit.min) {
      verdict = 'FAIL';
      detail += ` min=${limit.min}`;
    } else if (limit.max !== undefined && actual > limit.max) {
      verdict = 'FAIL';
      detail += ` max=${limit.max}`;
    }
    if (verdict === 'PASS' && baseline) {
      const previous = baseline.metrics[key];
      const allowed = limit.exact !== undefined ? previous : previous * BASELINE_FACTOR + BASELINE_SLACK;
      const regressed = limit.exact !== undefined
        ? actual !== previous
        : actual > allowed;
      if (regressed) {
        verdict = 'FAIL';
        detail += ` baseline=${previous} allowed=${allowed}`;
      }
    }
    results.push({ metric: key, verdict, detail, reason: limit.reason });
  }
  return results;
}

function emit(status, report, baseline, results, note) {
  const payload = {
    schema: GATE_SCHEMA,
    status,
    note,
    source: report.source,
    baseline: baseline ? baseline.source : null,
    results,
  };
  console.log(JSON.stringify(payload, null, 2));
}

function selfTest(state) {
  const base = generateReport();
  if (state === 'pass') {
    const results = evaluate(base, null);
    const failed = results.some((r) => r.verdict === 'FAIL');
    emit(failed ? 'FAIL' : 'PASS', base, null, results,
      'self-test pass: generated report within absolute limits');
    return failed ? 1 : 0;
  }
  if (state === 'na') {
    emit('N/A', base, null, [], 'self-test N/A: no baseline supplied');
    return 0;
  }
  if (state === 'fail') {
    const bad = JSON.parse(JSON.stringify(base));
    bad.metrics.rotation_horizontal_milli = 1571;
    const results = evaluate(bad, null);
    const failed = results.some((r) => r.verdict === 'FAIL');
    emit(failed ? 'FAIL' : 'PASS', bad, null, results, 'self-test fail: a horizontal move reports rotation');
    return failed ? 1 : 0;
  }
  if (state === 'rotation_flat') {
    const bad = JSON.parse(JSON.stringify(base));
    bad.metrics.rotation_quarter_turn_milli = 0;
    const results = evaluate(bad, null);
    const failed = results.some((r) => r.verdict === 'FAIL');
    emit(failed ? 'FAIL' : 'PASS', bad, null, results,
      'self-test rotation_flat: the quarter turn stopped reporting');
    return failed ? 1 : 0;
  }
  if (state === 'start_origin_lost') {
    const bad = JSON.parse(JSON.stringify(base));
    bad.metrics.drag_start_origin_delta = 20;
    const results = evaluate(bad, null);
    const failed = results.some((r) => r.verdict === 'FAIL');
    emit(failed ? 'FAIL' : 'PASS', bad, null, results,
      'self-test start_origin_lost: the drag start fell back to the down origin');
    return failed ? 1 : 0;
  }
  throw new Error(`unknown self-test state: ${state}`);
}

try {
  const args = parseArgs(process.argv);
  if (args.selfTest) {
    process.exitCode = selfTest(args.selfTest);
  } else {
    const report = args.report
      ? validateReport(readFileSync(resolve(args.report), 'utf8'), 'report')
      : generateReport();
    if (args.writeBaseline) {
      const path = resolve(args.writeBaseline);
      mkdirSync(dirname(path), { recursive: true });
      writeFileSync(path, `${JSON.stringify(report, null, 2)}\n`);
    }
    const baseline = args.baseline
      ? validateReport(readFileSync(resolve(args.baseline), 'utf8'), 'baseline')
      : null;
    if (!baseline) {
      emit('N/A', report, null, evaluate(report, null),
        'no baseline supplied: absolute limits checked, comparison N/A');
      process.exitCode = 0;
    } else {
      const results = evaluate(report, baseline);
      const failed = results.some((r) => r.verdict === 'FAIL');
      emit(failed ? 'FAIL' : 'PASS', report, baseline, results, 'baseline comparison');
      process.exitCode = failed ? 1 : 0;
    }
  }
} catch (err) {
  console.error(`ERROR ${err.message}`);
  process.exitCode = 2;
}
