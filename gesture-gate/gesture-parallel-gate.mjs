#!/usr/bin/env node
// Standalone parallel pointer arena gate. It never edits shared CI files.
//
// Usage:
//   node gesture-gate/gesture-parallel-gate.mjs [--report <json>]
//     [--baseline <json>] [--write-baseline <json>]
//     [--self-test pass|na|fail|no_independent_win|replay_drift|no_deadline]
//
// Exit codes: 0 PASS or N/A, 1 FAIL, 2 usage/IO error.
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';

const ROOT = resolve(dirname(new URL(import.meta.url).pathname), '..');
const REPORT_SCHEMA = 'opurom.gesture.parallel/1';
const GATE_SCHEMA = 'opurom.gesture.parallel.gate/1';
const METRICS = [
  'parallel_active_arenas',
  'parallel_resolved_arenas',
  'parallel_independent_wins',
  'parallel_replay_stable',
  'parallel_next_deadline_ms',
];
// The first four metrics are Flutter constants and must match exactly. The
// remaining ones are logical counts derived from the closed-form model and are
// only bounded, so a different libm rounding cannot fail the gate.
const LIMITS = {
  parallel_active_arenas: {
    min: 1,
    max: 64,
    reason: 'each pointer opens its own arena',
  },
  parallel_resolved_arenas: {
    min: 1,
    max: 64,
    reason: 'the primary contest must resolve',
  },
  parallel_independent_wins: {
    min: 1,
    max: 64,
    reason: 'a secondary pointer must resolve independently',
  },
  parallel_replay_stable: {
    exact: 1,
    reason: 'the replay must reproduce the same winner',
  },
  parallel_next_deadline_ms: {
    min: 1,
    max: 1000,
    reason: 'a pending pointer must report its long-press deadline',
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
    const child = spawnSync('cargo', ['test', '--test', 'gesture_parallel_report', '--quiet', '--', '--nocapture'], {
      cwd: ROOT,
      encoding: 'utf8',
      env: { ...process.env, GESTURE_PARALLEL_REPORT_DIR: dir },
      timeout: 120_000,
      maxBuffer: 4 * 1024 * 1024,
    });
    const output = `${child.stdout ?? ''}${child.stderr ?? ''}`;
    if (child.status !== 0) throw new Error(`report generation failed: ${output.slice(-800)}`);
    const path = join(dir, 'gesture-parallel-report.json');
    if (!existsSync(path)) throw new Error('report generation did not write gesture-parallel-report.json');
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
  if (state === 'no_independent_win') {
    const bad = JSON.parse(JSON.stringify(base));
    bad.metrics.parallel_independent_wins = 0;
    const results = evaluate(bad, null);
    const failed = results.some((r) => r.verdict === 'FAIL');
    emit(failed ? 'FAIL' : 'PASS', bad, null, results,
      'self-test no_independent_win: the secondary contest never resolved');
    return failed ? 1 : 0;
  }
  if (state === 'replay_drift') {
    const bad = JSON.parse(JSON.stringify(base));
    bad.metrics.parallel_replay_stable = 0;
    const results = evaluate(bad, null);
    const failed = results.some((r) => r.verdict === 'FAIL');
    emit(failed ? 'FAIL' : 'PASS', bad, null, results,
      'self-test replay_drift: the replay produced a different winner');
    return failed ? 1 : 0;
  }
  if (state === 'no_deadline') {
    const bad = JSON.parse(JSON.stringify(base));
    bad.metrics.parallel_next_deadline_ms = 0;
    const results = evaluate(bad, null);
    const failed = results.some((r) => r.verdict === 'FAIL');
    emit(failed ? 'FAIL' : 'PASS', bad, null, results,
      'self-test no_deadline: a pending pointer stopped reporting a deadline');
    return failed ? 1 : 0;
  }
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
    bad.metrics.parallel_independent_wins = 0;
    const results = evaluate(bad, null);
    const failed = results.some((r) => r.verdict === 'FAIL');
    emit(failed ? 'FAIL' : 'PASS', bad, null, results, 'self-test fail: a secondary pointer never wins');
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
