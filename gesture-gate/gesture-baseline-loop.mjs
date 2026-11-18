#!/usr/bin/env node
// Closed-loop baseline fixture for the six standalone gesture gates.
//
// Usage:
//   node gesture-gate/gesture-baseline-loop.mjs
//   node gesture-gate/gesture-baseline-loop.mjs --self-test pass|degrade|missing|parallel_faults
//
// The script proves, for perf, bounce, semantics, nested, parallel and
// advanced:
//   1. a baseline can be frozen from a generated report;
//   2. the same report passes against that baseline;
//   3. a metric enlarged beyond `baseline * 2 + 8` fails;
//   4. a missing baseline is ERROR (exit 2), not a silent PASS.
//
// Exit codes: 0 PASS, 1 FAIL, 2 usage/IO error.
import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';

const ROOT = resolve(dirname(new URL(import.meta.url).pathname), '..');
const GATES = ['perf', 'bounce', 'semantics', 'nested', 'parallel', 'advanced'];
const SCHEMA = 'opurom.gesture.baseline.loop/1';
const SOURCE = 'flutter-3.50.0:ab2912035e160d9cad6306df307576da6128ebdb';

function parseArgs(argv) {
  const args = { selfTest: null };
  for (let i = 2; i < argv.length; i += 1) {
    if (argv[i] === '--self-test') args.selfTest = argv[++i];
    else throw new Error(`unknown argument: ${argv[i]}`);
  }
  return args;
}

function run(gate, argv, label) {
  const script = join(ROOT, 'gesture-gate', `gesture-${gate}-gate.mjs`);
  const child = spawnSync(process.execPath, [script, ...argv], {
    cwd: ROOT,
    encoding: 'utf8',
    timeout: 300_000,
    maxBuffer: 8 * 1024 * 1024,
  });
  const stdout = child.stdout ?? '';
  const start = stdout.indexOf('{');
  let json = null;
  if (start >= 0) {
    try { json = JSON.parse(stdout.slice(start)); } catch { json = null; }
  }
  return { code: child.status, json, label };
}

function evaluate(workspace, mode) {
  const checks = [];
  const record = (gate, check, verdict, detail) => {
    checks.push({ gate, check, verdict, detail });
    return verdict === 'PASS';
  };
  for (const gate of GATES) {
    const baselinePath = join(workspace, `${gate}-baseline.json`);
    const reportPath = join(workspace, `${gate}-report.json`);
    const baseline = run(gate, ['--write-baseline', baselinePath], `${gate} write baseline`);
    record(gate, 'baseline_write',
      baseline.code === 0 && existsSync(baselinePath) ? 'PASS' : 'FAIL',
      `exit=${baseline.code}`);
    if (!existsSync(baselinePath)) continue;

    // Use the same report object for both the frozen baseline and the
    // comparison, so the test never depends on two separate report runs.
    // Reuse the exact bytes that the gate itself wrote as the baseline.
    // Re-serializing can drop fields that the gate validates.
    const baselineText = readFileSync(baselinePath, 'utf8');
    writeFileSync(reportPath, baselineText);
    const compare = run(gate,
      ['--report', reportPath, '--baseline', baselinePath],
      `${gate} baseline compare`);
    const compareOk = compare.code === 0
      && (compare.json?.status === 'PASS' || compare.json?.status === 'N/A');
    record(gate, 'baseline_compare', compareOk ? 'PASS' : 'FAIL',
      `status=${compare.json?.status ?? 'none'} exit=${compare.code}`);

    if (mode === 'degrade') {
      // Reuse the generated report, but replace its metrics with values far
      // above the baseline budget to exercise the regression branch.
      const degraded = JSON.parse(readFileSync(reportPath, 'utf8'));
      for (const key of Object.keys(degraded.metrics ?? {})) {
        degraded.metrics[key] = Math.round(degraded.metrics[key] * 4) + 100;
      }
      const degradedPath = join(workspace, `${gate}-degraded.json`);
      writeFileSync(degradedPath, `${JSON.stringify(degraded, null, 2)}\n`);
      const degradedResult = run(gate,
        ['--report', degradedPath, '--baseline', baselinePath],
        `${gate} degraded compare`);
      record(gate, 'degradation_detected',
        degradedResult.code === 1 && degradedResult.json?.status === 'FAIL' ? 'PASS' : 'FAIL',
        `status=${degradedResult.json?.status ?? 'none'}`);
    }

    if (mode === 'missing') {
      const missingResult = run(gate,
        ['--report', reportPath, '--baseline', join(workspace, 'absent-baseline.json')],
        `${gate} missing baseline`);
      record(gate, 'missing_baseline_error',
        missingResult.code === 2 ? 'PASS' : 'FAIL',
        `exit=${missingResult.code}`);
    }
  }
  const failed = checks.some((item) => item.verdict === 'FAIL');
  return { status: failed ? 'FAIL' : 'PASS', checks };
}

function emit(result, mode) {
  console.log(JSON.stringify({
    schema: SCHEMA,
    status: result.status,
    detail: `baseline loop ${mode}: ${result.checks.length} checks`,
    source: SOURCE,
    checks: result.checks,
  }, null, 2));
}

function runFaultMatrix(gate, states) {
  let allDetected = true;
  const checks = [];
  for (const state of states) {
    const child = spawnSync(process.execPath,
      [join(ROOT, 'gesture-gate', `gesture-${gate}-gate.mjs`), '--self-test', state],
      { cwd: ROOT, encoding: 'utf8', timeout: 300_000, maxBuffer: 8 * 1024 * 1024 });
    const detected = child.status === 1;
    if (!detected) allDetected = false;
    checks.push({
      gate,
      check: state,
      verdict: detected ? 'PASS' : 'FAIL',
      detail: detected ? 'expected FAIL verdict' : `exit=${child.status}`,
    });
  }
  console.log(JSON.stringify({
    schema: SCHEMA,
    status: allDetected ? 'FAIL' : 'PASS',
    detail: `${gate} fault matrix: ${states.join(', ')}`,
    source: SOURCE,
    checks,
  }, null, 2));
  return allDetected ? 1 : 0;
}

function main() {
  const args = parseArgs(process.argv);
  const mode = args.selfTest ?? 'pass';
  if (!['pass', 'degrade', 'missing', 'parallel_faults', 'gate_faults', 'baseline_faults'].includes(mode)) {
    throw new Error(`unknown self-test state: ${mode}`);
  }
  if (mode === 'parallel_faults') {
    return runFaultMatrix('parallel', ['no_independent_win', 'replay_drift', 'no_deadline']);
  }
  if (mode === 'baseline_faults') {
    const workspace = mkdtempSync(join(tmpdir(), 'opurom-gesture-baseline-fault-'));
    try {
      let allDetected = true;
      const checks = [];
      for (const gate of GATES) {
        const baselinePath = join(workspace, `${gate}-baseline.json`);
        const baseline = run(gate, ['--write-baseline', baselinePath], `${gate} write baseline`);
        if (baseline.code !== 0 || !existsSync(baselinePath)) {
          checks.push({ gate, check: 'baseline_regression', verdict: 'FAIL',
            detail: 'baseline write failed' });
          allDetected = false;
          continue;
        }
        const report = JSON.parse(readFileSync(baselinePath, 'utf8'));
        for (const key of Object.keys(report.metrics ?? {})) {
          const original = report.metrics[key];
          // A +100 increment stays inside the `baseline*2+8` budget for
          // large counts, so use the same super-budget factor as the
          // degrade self test to exercise the regression branch.
          report.metrics[key] = Math.round(original * 4) + 100;
          const driftedPath = join(workspace, `${gate}-${key}-drift.json`);
          writeFileSync(driftedPath, `${JSON.stringify(report, null, 2)}\n`);
          const drifted = run(gate,
            ['--report', driftedPath, '--baseline', baselinePath],
            `${gate} ${key} regression`);
          const detected = drifted.code === 1;
          if (!detected) allDetected = false;
          checks.push({ gate, check: `${key}:4x+100`, verdict: detected ? 'PASS' : 'FAIL',
            detail: detected ? 'regression rejected' : `exit=${drifted.code}` });
          report.metrics[key] = original;
        }
      }
      console.log(JSON.stringify({
        schema: SCHEMA,
        status: allDetected ? 'FAIL' : 'PASS',
        detail: `baseline regression matrix: ${checks.length} injections`,
        source: SOURCE,
        checks,
      }, null, 2));
      return allDetected ? 1 : 0;
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  }
  if (mode === 'gate_faults') {
    let allDetected = true;
    for (const [gate, states] of Object.entries({
      perf: ['burst_drift', 'slop_drift'],
      bounce: ['transfer_drift', 'overshoot_drift'],
      semantics: ['digest_drift', 'state_delta_lost'],
      nested: ['dropped_drag', 'notification_flat'],
      parallel: ['no_independent_win', 'replay_drift', 'no_deadline'],
      advanced: ['rotation_flat', 'start_origin_lost'],
    })) {
      if (runFaultMatrix(gate, states) !== 1) allDetected = false;
    }
    console.log(JSON.stringify({
      schema: SCHEMA,
      status: allDetected ? 'FAIL' : 'PASS',
      detail: 'gate fault matrix: 13 injections across six gates',
      source: SOURCE,
      checks: [],
    }, null, 2));
    return allDetected ? 1 : 0;
  }
  const workspace = mkdtempSync(join(tmpdir(), 'opurom-gesture-baseline-'));
  try {
    const result = evaluate(workspace, mode);
    // For the injected modes the emitted status is the verdict of the
    // injected run: a detected regression is reported as FAIL (exit 1).
    const injectedDetected = result.status === 'PASS';
    const reported = mode === 'pass'
      ? result
      : { ...result, status: injectedDetected ? 'FAIL' : 'PASS' };
    emit(reported, mode);
    if (mode === 'pass') return result.status === 'PASS' ? 0 : 1;
    return injectedDetected ? 1 : 0;
  } finally {
    rmSync(workspace, { recursive: true, force: true });
  }
}

try {
  process.exitCode = main();
} catch (err) {
  console.error(`ERROR ${err.message}`);
  process.exitCode = 2;
}
