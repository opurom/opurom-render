#!/usr/bin/env node
// Joint gesture fairness + performance gate. Additive only: it does not edit
// shared CI, IR, ABI or renderer sources.
//
// Usage:
//   node gesture-gate/gesture-fairness-perf-gate.mjs \
//     [--perf <opurom.gesture.gate/1 JSON>] \
//     [--fairness <opurom.gesture.fairness/1 JSON>] \
//     [--baseline <joint JSON>] [--write-baseline <joint JSON>] \
//     [--self-test pass|na|fail]
//
// Exit codes: 0 PASS or N/A, 1 FAIL, 2 usage/IO error.
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';

const ROOT = resolve(dirname(new URL(import.meta.url).pathname), '..');
const PERF_GATE = join(ROOT, 'gesture-gate', 'gesture-perf-gate.mjs');
const FAIRNESS_TEST = 'multi_pointer_fairness';
const FAIRNESS_CASES = [
  'simultaneous_downs_open_independent_arenas',
  'same_frame_race_resolves_by_join_order',
  'hold_delays_sweep_until_release',
  'cancel_rolls_back_every_member_and_timer_state',
  'single_winner_terminal_state_excludes_other_members',
  'fairness_boundary_first_join_order_is_stable',
  'multi_pointer_contest_is_deterministic_on_replay',
];
const JOINT_SCHEMA = 'opurom.gesture.joint/1';
const FAIRNESS_SCHEMA = 'opurom.gesture.fairness/1';
const PERF_SCHEMA = 'opurom.gesture.gate/1';
const JOINT_SOURCE = 'flutter-3.50.0:ab2912035e160d9cad6306df307576da6128ebdb';

function parseArgs(argv) {
  const args = { perf: null, fairness: null, baseline: null, writeBaseline: null, selfTest: null, raw: false };
  for (let i = 2; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--perf') args.perf = argv[++i];
    else if (arg === '--fairness') args.fairness = argv[++i];
    else if (arg === '--baseline') args.baseline = argv[++i];
    else if (arg === '--write-baseline') args.writeBaseline = argv[++i];
    else if (arg === '--self-test') args.selfTest = argv[++i];
    else if (arg === '--raw') args.raw = true;
    else throw new Error(`unknown argument: ${arg}`);
  }
  return args;
}

function readJson(path, label, schema) {
  if (!path || !existsSync(resolve(path))) throw new Error(`${label} file is missing: ${path}`);
  let value;
  try { value = JSON.parse(readFileSync(resolve(path), 'utf8')); }
  catch (err) { throw new Error(`${label} is not JSON: ${err.message}`); }
  if (value?.schema !== schema) throw new Error(`${label} has schema ${value?.schema}; expected ${schema}`);
  return value;
}

function generatePerfReport() {
  const child = spawnSync(process.execPath, [PERF_GATE], {
    cwd: ROOT,
    encoding: 'utf8',
    timeout: 180_000,
    maxBuffer: 4 * 1024 * 1024,
  });
  const output = `${child.stdout ?? ''}${child.stderr ?? ''}`;
  if (child.status !== 0) throw new Error(`performance gate failed (${child.status}): ${output.slice(-800)}`);
  return readJsonFromText(output, PERF_SCHEMA, 'performance gate output');
}

function readJsonFromText(text, schema, label) {
  const start = text.indexOf('{');
  if (start < 0) throw new Error(`${label} has no JSON object`);
  try {
    const value = JSON.parse(text.slice(start));
    if (value?.schema !== schema) throw new Error(`${label} has schema ${value?.schema}; expected ${schema}`);
    return value;
  } catch (err) {
    throw new Error(`${label} is not valid JSON: ${err.message}`);
  }
}

function generateFairnessReport() {
  const child = spawnSync('cargo', ['test', '--test', FAIRNESS_TEST, '--quiet'], {
    cwd: ROOT,
    encoding: 'utf8',
    timeout: 180_000,
    maxBuffer: 4 * 1024 * 1024,
  });
  const output = `${child.stdout ?? ''}${child.stderr ?? ''}`;
  if (child.status !== 0) throw new Error(`fairness tests failed: ${output.slice(-1200)}`);
  return {
    schema: FAIRNESS_SCHEMA,
    source: JOINT_SOURCE,
    test: FAIRNESS_TEST,
    cases: FAIRNESS_CASES.slice(),
    runner_status: child.status,
  };
}

function makeJoint(perf, fairness) {
  return {
    schema: JOINT_SCHEMA,
    source: JOINT_SOURCE,
    generated_by: basename(new URL(import.meta.url).pathname),
    perf,
    fairness,
  };
}

function evaluate(joint, baseline) {
  const results = [];
  const perfStatus = joint?.perf?.status;
  results.push({
    check: 'perf_gate_status',
    verdict: perfStatus === 'PASS' || perfStatus === 'N/A' ? 'PASS' : 'FAIL',
    detail: `status=${perfStatus ?? 'missing'}`,
    reason: 'perf gate must not FAIL',
  });
  const perfMetrics = joint?.perf?.metrics ?? joint?.perf?.results ?? null;
  results.push({
    check: 'perf_machine_report',
    verdict: perfMetrics && (!Array.isArray(perfMetrics) || perfMetrics.length > 0) ? 'PASS' : 'FAIL',
    detail: Array.isArray(perfMetrics) ? `results=${perfMetrics.length}` : `metrics=${Object.keys(perfMetrics ?? {}).length}`,
    reason: 'machine-readable perf evidence must be present',
  });
  const cases = joint?.fairness?.cases ?? [];
  results.push({
    check: 'fairness_case_set',
    verdict: FAIRNESS_CASES.every((name) => cases.includes(name)) ? 'PASS' : 'FAIL',
    detail: `cases=${cases.length}`,
    reason: 'all seven fairness cases must be present',
  });
  results.push({
    check: 'fairness_runner_status',
    verdict: joint?.fairness?.runner_status === 0 ? 'PASS' : 'FAIL',
    detail: `exit=${joint?.fairness?.runner_status}`,
    reason: 'cargo test multi_pointer_fairness must exit 0',
  });
  if (baseline) {
    const bCases = baseline?.fairness?.cases ?? [];
    const bResults = baseline?.perf?.results ?? [];
    const currentResults = joint?.perf?.results ?? [];
    const bMetrics = new Map(bResults.map((item) => [item.metric, item.detail]));
    const currentMetrics = new Map(currentResults.map((item) => [item.metric, item.detail]));
    const sameCases = bCases.length === FAIRNESS_CASES.length
      && FAIRNESS_CASES.every((name) => bCases.includes(name));
    const sameMetrics = bMetrics.size > 0
      && [...bMetrics.keys()].every((key) => currentMetrics.has(key));
    results.push({
      check: 'baseline_comparability',
      verdict: sameCases && sameMetrics ? 'PASS' : 'FAIL',
      detail: sameCases && sameMetrics ? 'baseline schema and fields match' : 'baseline lacks matching fairness or perf results',
      reason: 'a frozen baseline must carry the same evidence fields',
    });
  }
  return results;
}

function emit(status, joint, baseline, results, note, raw = false) {
  const payload = {
    schema: JOINT_SCHEMA,
    status,
    note,
    source: joint.source,
    baseline: baseline?.source ?? null,
    results,
  };
  if (raw) {
    payload.perf = joint.perf;
    payload.fairness = joint.fairness;
  }
  console.log(JSON.stringify(payload, null, 2));
}

function main() {
  const args = parseArgs(process.argv);
  if (args.selfTest) {
    const perf = generatePerfReport();
    const fairness = generateFairnessReport();
    const joint = makeJoint(perf, fairness);
    if (args.selfTest === 'pass') {
      const results = evaluate(joint, null);
      const ok = results.every((r) => r.verdict === 'PASS');
      emit(ok ? 'PASS' : 'FAIL', joint, null, results, 'self-test pass: fresh joint evidence', args.raw);
      return ok ? 0 : 1;
    }
    if (args.selfTest === 'na') {
      emit('N/A', joint, null, [], 'self-test N/A: no baseline supplied', args.raw);
      return 0;
    }
    if (args.selfTest === 'fail') {
      const bad = JSON.parse(JSON.stringify(joint));
      bad.fairness.cases = bad.fairness.cases.filter((name) => name !== 'hold_delays_sweep_until_release');
      const results = evaluate(bad, null);
      const failed = results.some((r) => r.verdict === 'FAIL');
      emit(failed ? 'FAIL' : 'PASS', bad, null, results, 'self-test fail: fairness case set drift', args.raw);
      return failed ? 1 : 0;
    }
    throw new Error(`unknown self-test state: ${args.selfTest}`);
  }

  const perf = args.perf ? readJson(args.perf, 'perf report', PERF_SCHEMA) : generatePerfReport();
  const fairness = args.fairness ? readJson(args.fairness, 'fairness report', FAIRNESS_SCHEMA) : generateFairnessReport();
  const joint = makeJoint(perf, fairness);
  if (args.writeBaseline) {
    const path = resolve(args.writeBaseline);
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, `${JSON.stringify(joint, null, 2)}\n`);
  }
  const baseline = args.baseline ? readJson(args.baseline, 'baseline', JOINT_SCHEMA) : null;
  if (!baseline) {
    const results = evaluate(joint, null);
    const failed = results.some((r) => r.verdict === 'FAIL');
    emit(failed ? 'FAIL' : 'N/A', joint, null, results,
      failed ? 'absolute joint checks failed' : 'no baseline supplied: absolute checks only', args.raw);
    return failed ? 1 : 0;
  }
  const results = evaluate(joint, baseline);
  const failed = results.some((r) => r.verdict === 'FAIL');
  emit(failed ? 'FAIL' : 'PASS', joint, baseline, results, 'joint baseline comparison', args.raw);
  return failed ? 1 : 0;
}

try {
  process.exitCode = main();
} catch (err) {
  console.error(`ERROR ${err.message}`);
  process.exitCode = 2;
}
