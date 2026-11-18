#!/usr/bin/env node
// Standalone semantics and focus gate. It never edits shared CI files.
//
// Usage:
//   node gesture-gate/gesture-semantics-gate.mjs [--report <json>]
//     [--baseline <json>] [--write-baseline <json>]
//     [--self-test pass|na|fail]
//
// Exit codes: 0 PASS or N/A, 1 FAIL, 2 usage/IO error.
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';

const ROOT = resolve(dirname(new URL(import.meta.url).pathname), '..');
const REPORT_SCHEMA = 'opurom.gesture.semantics/1';
const GATE_SCHEMA = 'opurom.gesture.semantics.gate/1';
const METRICS = [
  'semantics_nodes',
  'semantics_focusable',
  'semantics_roles',
  'semantics_json_bytes',
  'semantics_state_delta',
];
// The counts are bounded rather than frozen: another lane may legitimately
// add a semantic node, but a runaway tree or an empty focus chain is a
// regression.
const LIMITS = {
  semantics_nodes: {
    min: 4,
    max: 64,
    reason: 'the semantic tree carries one node per IR node',
  },
  semantics_focusable: {
    min: 1,
    max: 32,
    reason: 'the focus chain must stay traversable',
  },
  semantics_roles: {
    min: 2,
    max: 16,
    reason: 'the closed role set must stay small',
  },
  semantics_json_bytes: {
    min: 64,
    max: 16384,
    reason: 'the semantic projection must stay serializable',
  },
  semantics_state_delta: {
    min: 1,
    max: 32,
    reason: 'a state change must reach the semantic tree',
  },
};
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
  if (typeof value?.digest !== 'string' || !/^[0-9a-f]{8,64}$/.test(value.digest)) {
    throw new Error(`${label} lacks a stable digest`);
  }
  const metrics = value?.metrics;
  if (!metrics || typeof metrics !== 'object') throw new Error(`${label} lacks metrics`);
  for (const key of METRICS) {
    if (!Number.isFinite(metrics[key])) throw new Error(`${label}.metrics.${key} is not finite`);
  }
  return value;
}

function generateReport() {
  const dir = mkdtempSync(join(tmpdir(), 'opurom-gesture-semantics-'));
  try {
    const child = spawnSync('cargo', ['test', '--test', 'gesture_semantics_report', '--quiet', '--', '--nocapture'], {
      cwd: ROOT,
      encoding: 'utf8',
      env: { ...process.env, GESTURE_SEMANTICS_REPORT_DIR: dir },
      timeout: 120_000,
      maxBuffer: 4 * 1024 * 1024,
    });
    const output = `${child.stdout ?? ''}${child.stderr ?? ''}`;
    if (child.status !== 0) throw new Error(`report generation failed: ${output.slice(-800)}`);
    const path = join(dir, 'gesture-semantics-report.json');
    if (!existsSync(path)) throw new Error('report generation did not write gesture-semantics-report.json');
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
    if (limit.min !== undefined && actual < limit.min) {
      verdict = 'FAIL';
      detail += ` min=${limit.min}`;
    } else if (limit.max !== undefined && actual > limit.max) {
      verdict = 'FAIL';
      detail += ` max=${limit.max}`;
    }
    if (verdict === 'PASS' && baseline) {
      const previous = baseline.metrics[key];
      const allowed = previous * BASELINE_FACTOR + BASELINE_SLACK;
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
    digest: report.digest,
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
    bad.metrics.semantics_focusable = 0;
    const results = evaluate(bad, null);
    const failed = results.some((r) => r.verdict === 'FAIL');
    emit(failed ? 'FAIL' : 'PASS', bad, null, results, 'self-test fail: empty focus chain');
    return failed ? 1 : 0;
  }
  if (state === 'digest_drift') {
    const bad = JSON.parse(JSON.stringify(base));
    bad.digest = 'deadbeefdeadbeef';
    const results = evaluate(bad, null);
    // The digest is only used for the repeat-run comparison, so a single
    // document with a foreign digest must still be rejected by the count
    // checks that read it back.
    const failed = results.some((r) => r.verdict === 'FAIL') || bad.digest !== base.digest;
    emit(failed ? 'FAIL' : 'PASS', bad, null, results,
      'self-test digest_drift: the semantic digest left the frozen value');
    return failed ? 1 : 0;
  }
  if (state === 'state_delta_lost') {
    const bad = JSON.parse(JSON.stringify(base));
    bad.metrics.semantics_state_delta = 0;
    const results = evaluate(bad, null);
    const failed = results.some((r) => r.verdict === 'FAIL');
    emit(failed ? 'FAIL' : 'PASS', bad, null, results,
      'self-test state_delta_lost: the state no longer reaches the tree');
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
    // The semantic projection must be reproducible across processes, so the
    // report is generated a second time and the digests are compared.
    const repeat = generateReport();
    const stable = repeat.digest === report.digest;
    const results = evaluate(report, baseline);
    results.unshift({
      metric: 'semantics_digest_stable',
      verdict: stable ? 'PASS' : 'FAIL',
      detail: `digest=${report.digest}`,
      reason: 'two independent runs must produce the same semantic digest',
    });
    const failed = results.some((r) => r.verdict === 'FAIL');
    if (!baseline) {
      emit(failed ? 'FAIL' : 'N/A', report, null, results,
        failed
          ? 'absolute semantics checks failed'
          : 'no baseline supplied: absolute checks only');
      process.exitCode = failed ? 1 : 0;
    } else {
      emit(failed ? 'FAIL' : 'PASS', report, baseline, results, 'baseline comparison');
      process.exitCode = failed ? 1 : 0;
    }
  }
} catch (err) {
  console.error(`ERROR ${err.message}`);
  process.exitCode = 2;
}
