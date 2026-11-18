#!/usr/bin/env node
// Standalone gesture performance gate. It never edits shared CI files.
// Usage: node gesture-gate/gesture-perf-gate.mjs [--report <json>] [--baseline <json>] [--write-baseline <json>]
// Exit codes: 0 PASS or N/A, 1 FAIL, 2 usage/IO error.
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';

const ROOT = resolve(dirname(new URL(import.meta.url).pathname), '..');
const METRICS = [
  'gesture_burst_small',
  'gesture_burst_large',
  'arena_slots_one',
  'arena_slots_four',
  'packet_loss_full',
  'packet_loss_lossy',
  'replay_bytes',
  'slop_threshold',
];
// Regression budget: these are logical-operation counts, not wall-clock time.
// Exact equality is required for the deterministic replay and Flutter slop.
const LIMITS = {
  gesture_burst_small: { max: 300, reason: 'burst work must stay linear and bounded' },
  gesture_burst_large: { max: 600, reason: '2x frames must not exceed 2x work' },
  arena_slots_one: { max: 80, reason: 'one-member arena visit bound' },
  arena_slots_four: { max: 320, reason: 'four-member arena visit bound' },
  packet_loss_full: { max: 100, reason: 'packet dispatch bound' },
  packet_loss_lossy: { max: 100, reason: 'lossy packet dispatch bound' },
  replay_bytes: { exact: 487, reason: 'event replay digest is frozen' },
  slop_threshold: { exact: 18, reason: 'Flutter kTouchSlop at constants.dart:65' },
};
// Baseline comparison allows normal jitter of 2x + 8 logical ops for scaling
// metrics, and exact equality for deterministic digest/threshold fields.
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
  if (value?.schema !== 'opurom.gesture.perf/1') throw new Error(`${label} has unsupported schema`);
  if (value?.clock !== 'logical') throw new Error(`${label} must use a logical clock`);
  const metrics = value?.metrics;
  if (!metrics || typeof metrics !== 'object') throw new Error(`${label} lacks metrics`);
  for (const key of METRICS) {
    if (!Number.isFinite(metrics[key])) throw new Error(`${label}.metrics.${key} is not finite`);
  }
  return value;
}

function generateReport() {
  const dir = mkdtempSync(join(tmpdir(), 'opurom-gesture-perf-'));
  try {
    const child = spawnSync('cargo', ['test', '--test', 'gesture_perf_report', '--quiet', '--', '--nocapture'], {
      cwd: ROOT,
      encoding: 'utf8',
      env: { ...process.env, GESTURE_PERF_REPORT_DIR: dir },
      timeout: 120_000,
      maxBuffer: 4 * 1024 * 1024,
    });
    const output = `${child.stdout ?? ''}${child.stderr ?? ''}`;
    if (child.status !== 0) throw new Error(`report generation failed: ${output.slice(-800)}`);
    const path = join(dir, 'gesture-perf-report.json');
    if (!existsSync(path)) throw new Error('report generation did not write gesture-perf-report.json');
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
    } else if (limit.max !== undefined && actual > limit.max) {
      verdict = 'FAIL';
      detail += ` max=${limit.max}`;
    }
    if (verdict === 'PASS' && baseline) {
      const previous = baseline.metrics[key];
      const allowed = limit.exact !== undefined ? previous : previous * BASELINE_FACTOR + BASELINE_SLACK;
      if (actual > allowed) {
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
    schema: 'opurom.gesture.gate/1',
    status,
    note,
    source: report.source,
    baseline: baseline ? baseline.source : null,
    results,
  };
  console.log(JSON.stringify(payload, null, 2));
}

function selfTest() {
  const base = generateReport();
  if (selfTestArg === 'pass') {
    emit('PASS', base, null, evaluate(base, null), 'self-test pass: generated report within absolute limits');
    return 0;
  }
  if (selfTestArg === 'na') {
    emit('N/A', base, null, [], 'self-test N/A: no baseline supplied');
    return 0;
  }
  if (selfTestArg === 'fail') {
    const bad = JSON.parse(JSON.stringify(base));
    bad.metrics.replay_bytes += 1;
    const results = evaluate(bad, null);
    const failed = results.filter((r) => r.verdict === 'FAIL').length > 0;
    emit(failed ? 'FAIL' : 'PASS', bad, null, results, 'self-test fail: replay digest drift');
    return failed ? 1 : 0;
  }
  if (selfTestArg === 'burst_drift') {
    const bad = JSON.parse(JSON.stringify(base));
    bad.metrics.gesture_burst_large = 4096;
    const results = evaluate(bad, null);
    const failed = results.some((r) => r.verdict === 'FAIL');
    emit(failed ? 'FAIL' : 'PASS', bad, null, results,
      'self-test burst_drift: the large burst exceeded its visit bound');
    return failed ? 1 : 0;
  }
  if (selfTestArg === 'slop_drift') {
    const bad = JSON.parse(JSON.stringify(base));
    bad.metrics.slop_threshold = 20;
    const results = evaluate(bad, null);
    const failed = results.some((r) => r.verdict === 'FAIL');
    emit(failed ? 'FAIL' : 'PASS', bad, null, results,
      'self-test slop_drift: the touch slop left the Flutter constant');
    return failed ? 1 : 0;
  }
  throw new Error(`unknown self-test state: ${selfTestArg}`);
}

let selfTestArg = null;
try {
  const args = parseArgs(process.argv);
  selfTestArg = args.selfTest;
  if (args.selfTest) process.exitCode = selfTest();
  else {
    const report = args.report ? validateReport(readFileSync(resolve(args.report), 'utf8'), 'report') : generateReport();
    if (args.writeBaseline) {
      const path = resolve(args.writeBaseline);
      mkdirSync(dirname(path), { recursive: true });
      writeFileSync(path, `${JSON.stringify(report, null, 2)}\n`);
    }
    const baseline = args.baseline ? validateReport(readFileSync(resolve(args.baseline), 'utf8'), 'baseline') : null;
    if (!baseline) {
      emit('N/A', report, null, evaluate(report, null), 'no baseline supplied: absolute limits checked, comparison N/A');
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
