#!/usr/bin/env node
// Joint gesture fairness + performance gate. Additive only: it does not edit
// shared CI, IR, ABI or renderer sources.
//
// Usage:
//   node gesture-gate/gesture-fairness-perf-gate.mjs \
//     [--perf <opurom.gesture.gate/1 JSON>] \
//     [--fairness <opurom.gesture.fairness/1 JSON>] \
//     [--bounce <opurom.gesture.bounce.gate/1 JSON>] \
//     [--baseline <joint JSON>] [--write-baseline <joint JSON>] \
//     [--self-test pass|na|fail]
//
// Exit codes: 0 PASS or N/A, 1 FAIL, 2 usage/IO error.
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';

const ROOT = resolve(dirname(new URL(import.meta.url).pathname), '..');
const PERF_GATE = join(ROOT, 'gesture-gate', 'gesture-perf-gate.mjs');
const BOUNCE_GATE = join(ROOT, 'gesture-gate', 'gesture-bounce-gate.mjs');
const SEMANTICS_GATE = join(ROOT, 'gesture-gate', 'gesture-semantics-gate.mjs');
const NESTED_GATE = join(ROOT, 'gesture-gate', 'gesture-nested-gate.mjs');
const PARALLEL_GATE = join(ROOT, 'gesture-gate', 'gesture-parallel-gate.mjs');
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
const BOUNCE_SCHEMA = 'opurom.gesture.bounce.gate/1';
const SEMANTICS_SCHEMA = 'opurom.gesture.semantics.gate/1';
const NESTED_SCHEMA = 'opurom.gesture.nested.gate/1';
const PARALLEL_SCHEMA = 'opurom.gesture.parallel.gate/1';
const JOINT_SOURCE = 'flutter-3.50.0:ab2912035e160d9cad6306df307576da6128ebdb';

function parseArgs(argv) {
  const args = {
    perf: null,
    fairness: null,
    bounce: null,
    semantics: null,
    nested: null,
    parallel: null,
    baseline: null,
    writeBaseline: null,
    selfTest: null,
    raw: false,
  };
  for (let i = 2; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--perf') args.perf = argv[++i];
    else if (arg === '--fairness') args.fairness = argv[++i];
    else if (arg === '--bounce') args.bounce = argv[++i];
    else if (arg === '--semantics') args.semantics = argv[++i];
    else if (arg === '--nested') args.nested = argv[++i];
    else if (arg === '--parallel') args.parallel = argv[++i];
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

function generateBounceReport() {
  const child = spawnSync(process.execPath, [BOUNCE_GATE], {
    cwd: ROOT,
    encoding: 'utf8',
    timeout: 180_000,
    maxBuffer: 4 * 1024 * 1024,
  });
  const output = `${child.stdout ?? ''}${child.stderr ?? ''}`;
  if (child.status !== 0) throw new Error(`bounce gate failed (${child.status}): ${output.slice(-800)}`);
  return readJsonFromText(output, BOUNCE_SCHEMA, 'bounce gate output');
}

function generateSemanticsReport() {
  const child = spawnSync(process.execPath, [SEMANTICS_GATE], {
    cwd: ROOT,
    encoding: 'utf8',
    timeout: 180_000,
    maxBuffer: 4 * 1024 * 1024,
  });
  const output = `${child.stdout ?? ''}${child.stderr ?? ''}`;
  if (child.status !== 0) throw new Error(`semantics gate failed (${child.status}): ${output.slice(-800)}`);
  return readJsonFromText(output, SEMANTICS_SCHEMA, 'semantics gate output');
}

function generateNestedReport() {
  const child = spawnSync(process.execPath, [NESTED_GATE], {
    cwd: ROOT,
    encoding: 'utf8',
    timeout: 180_000,
    maxBuffer: 4 * 1024 * 1024,
  });
  const output = `${child.stdout ?? ''}${child.stderr ?? ''}`;
  if (child.status !== 0) throw new Error(`nested gate failed (${child.status}): ${output.slice(-800)}`);
  return readJsonFromText(output, NESTED_SCHEMA, 'nested gate output');
}

function generateParallelReport() {
  const child = spawnSync(process.execPath, [PARALLEL_GATE], {
    cwd: ROOT,
    encoding: 'utf8',
    timeout: 180_000,
    maxBuffer: 4 * 1024 * 1024,
  });
  const output = `${child.stdout ?? ''}${child.stderr ?? ''}`;
  if (child.status !== 0) throw new Error(`parallel gate failed (${child.status}): ${output.slice(-800)}`);
  return readJsonFromText(output, PARALLEL_SCHEMA, 'parallel gate output');
}

function makeJoint(perf, fairness, bounce, semantics, nested, parallel) {
  return {
    schema: JOINT_SCHEMA,
    source: JOINT_SOURCE,
    generated_by: basename(new URL(import.meta.url).pathname),
    perf,
    fairness,
    bounce,
    semantics,
    nested,
    parallel,
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
  const bounceStatus = joint?.bounce?.status;
  results.push({
    check: 'bounce_gate_status',
    verdict: bounceStatus === 'PASS' || bounceStatus === 'N/A' ? 'PASS' : 'FAIL',
    detail: `status=${bounceStatus ?? 'missing'}`,
    reason: 'the bounce physics gate must not FAIL',
  });
  const bounceResults = joint?.bounce?.results ?? [];
  results.push({
    check: 'bounce_machine_report',
    verdict: Array.isArray(bounceResults) && bounceResults.length > 0 ? 'PASS' : 'FAIL',
    detail: `results=${Array.isArray(bounceResults) ? bounceResults.length : 0}`,
    reason: 'machine-readable bounce evidence must be present',
  });
  const semanticsStatus = joint?.semantics?.status;
  results.push({
    check: 'semantics_gate_status',
    verdict: semanticsStatus === 'PASS' || semanticsStatus === 'N/A' ? 'PASS' : 'FAIL',
    detail: `status=${semanticsStatus ?? 'missing'}`,
    reason: 'the semantics gate must not FAIL',
  });
  const semanticsResults = joint?.semantics?.results ?? [];
  results.push({
    check: 'semantics_machine_report',
    verdict: Array.isArray(semanticsResults) && semanticsResults.length > 0 ? 'PASS' : 'FAIL',
    detail: `results=${Array.isArray(semanticsResults) ? semanticsResults.length : 0}`,
    reason: 'machine-readable semantics evidence must be present',
  });
  const nestedStatus = joint?.nested?.status;
  results.push({
    check: 'nested_gate_status',
    verdict: nestedStatus === 'PASS' || nestedStatus === 'N/A' ? 'PASS' : 'FAIL',
    detail: `status=${nestedStatus ?? 'missing'}`,
    reason: 'the nested scroll gate must not FAIL',
  });
  const nestedResults = joint?.nested?.results ?? [];
  results.push({
    check: 'nested_machine_report',
    verdict: Array.isArray(nestedResults) && nestedResults.length > 0 ? 'PASS' : 'FAIL',
    detail: `results=${Array.isArray(nestedResults) ? nestedResults.length : 0}`,
    reason: 'machine-readable nested scroll evidence must be present',
  });
  const parallelStatus = joint?.parallel?.status;
  results.push({
    check: 'parallel_gate_status',
    verdict: parallelStatus === 'PASS' || parallelStatus === 'N/A' ? 'PASS' : 'FAIL',
    detail: `status=${parallelStatus ?? 'missing'}`,
    reason: 'the parallel pointer gate must not FAIL',
  });
  const parallelResults = joint?.parallel?.results ?? [];
  results.push({
    check: 'parallel_machine_report',
    verdict: Array.isArray(parallelResults) && parallelResults.length > 0 ? 'PASS' : 'FAIL',
    detail: `results=${Array.isArray(parallelResults) ? parallelResults.length : 0}`,
    reason: 'machine-readable parallel pointer evidence must be present',
  });
  if (baseline) {
    const bCases = baseline?.fairness?.cases ?? [];
    const bResults = baseline?.perf?.results ?? [];
    const currentResults = joint?.perf?.results ?? [];
    const bBounce = baseline?.bounce?.results ?? [];
    const currentBounce = joint?.bounce?.results ?? [];
    const bSemantics = baseline?.semantics?.results ?? [];
    const currentSemantics = joint?.semantics?.results ?? [];
    const bNested = baseline?.nested?.results ?? [];
    const currentNested = joint?.nested?.results ?? [];
    const bParallel = baseline?.parallel?.results ?? [];
    const currentParallel = joint?.parallel?.results ?? [];
    const bMetrics = new Map(bResults.map((item) => [item.metric, item.detail]));
    const currentMetrics = new Map(currentResults.map((item) => [item.metric, item.detail]));
    const bBounceMetrics = new Map(bBounce.map((item) => [item.metric, item.detail]));
    const currentBounceMetrics = new Map(currentBounce.map((item) => [item.metric, item.detail]));
    const sameCases = bCases.length === FAIRNESS_CASES.length
      && FAIRNESS_CASES.every((name) => bCases.includes(name));
    const sameMetrics = bMetrics.size > 0
      && [...bMetrics.keys()].every((key) => currentMetrics.has(key));
    const sameBounce = bBounceMetrics.size > 0
      && [...bBounceMetrics.keys()].every((key) => currentBounceMetrics.has(key));
    const bSemanticsMetrics = new Map(bSemantics.map((item) => [item.metric, item.detail]));
    const currentSemanticsMetrics = new Map(currentSemantics.map((item) => [item.metric, item.detail]));
    const sameSemantics = bSemanticsMetrics.size > 0
      && [...bSemanticsMetrics.keys()].every((key) => currentSemanticsMetrics.has(key));
    const bNestedMetrics = new Map(bNested.map((item) => [item.metric, item.detail]));
    const currentNestedMetrics = new Map(currentNested.map((item) => [item.metric, item.detail]));
    const sameNested = bNestedMetrics.size > 0
      && [...bNestedMetrics.keys()].every((key) => currentNestedMetrics.has(key));
    const bParallelMetrics = new Map(bParallel.map((item) => [item.metric, item.detail]));
    const currentParallelMetrics = new Map(currentParallel.map((item) => [item.metric, item.detail]));
    const sameParallel = bParallelMetrics.size > 0
      && [...bParallelMetrics.keys()].every((key) => currentParallelMetrics.has(key));
    results.push({
      check: 'baseline_comparability',
      verdict: sameCases && sameMetrics && sameBounce && sameSemantics && sameNested && sameParallel
        ? 'PASS' : 'FAIL',
      detail: sameCases && sameMetrics && sameBounce && sameSemantics && sameNested && sameParallel
        ? 'baseline schema and fields match'
        : 'baseline lacks matching fairness, perf, bounce, semantics, nested or parallel results',
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
    payload.bounce = joint.bounce;
    payload.semantics = joint.semantics;
    payload.nested = joint.nested;
    payload.parallel = joint.parallel;
  }
  console.log(JSON.stringify(payload, null, 2));
}

function main() {
  const args = parseArgs(process.argv);
  if (args.selfTest) {
    const perf = generatePerfReport();
    const fairness = generateFairnessReport();
    const bounce = generateBounceReport();
    const semantics = generateSemanticsReport();
    const nested = generateNestedReport();
    const parallel = generateParallelReport();
    const joint = makeJoint(perf, fairness, bounce, semantics, nested, parallel);
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
  const bounce = args.bounce ? readJson(args.bounce, 'bounce report', BOUNCE_SCHEMA) : generateBounceReport();
  const semantics = args.semantics
    ? readJson(args.semantics, 'semantics report', SEMANTICS_SCHEMA)
    : generateSemanticsReport();
  const nested = args.nested
    ? readJson(args.nested, 'nested report', NESTED_SCHEMA)
    : generateNestedReport();
  const parallel = args.parallel
    ? readJson(args.parallel, 'parallel report', PARALLEL_SCHEMA)
    : generateParallelReport();
  const joint = makeJoint(perf, fairness, bounce, semantics, nested, parallel);
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
