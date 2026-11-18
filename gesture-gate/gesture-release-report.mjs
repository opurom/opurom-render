#!/usr/bin/env node
// Produce a machine-readable release-readiness report for the gesture module.
// Additive only: it never edits shared CI, IR, ABI or renderer sources.
//
// Usage:
//   node gesture-gate/gesture-release-report.mjs [--out <json>]
//   node gesture-gate/gesture-release-report.mjs --self-test pass|fault
//
// Exit codes: 0 PASS, 1 FAIL, 2 usage/IO error.
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';

const ROOT = resolve(dirname(new URL(import.meta.url).pathname), '..');
const SCHEMA = 'opurom.gesture.release/1';
const SOURCE = 'flutter-3.50.0:ab2912035e160d9cad6306df307576da6128ebdb';
const BASE = 'origin/render/p0-parity';
const GATES = ['perf', 'bounce', 'semantics', 'nested', 'parallel', 'advanced'];

function parseArgs(argv) {
  const args = { out: null, selfTest: null };
  for (let i = 2; i < argv.length; i += 1) {
    if (argv[i] === '--out') args.out = argv[++i];
    else if (argv[i] === '--self-test') args.selfTest = argv[++i];
    else throw new Error(`unknown argument: ${argv[i]}`);
  }
  return args;
}

function shell(cmd, argv, label) {
  const child = spawnSync(cmd, argv, {
    cwd: ROOT,
    encoding: 'utf8',
    timeout: 900_000,
    maxBuffer: 16 * 1024 * 1024,
  });
  const stdout = child.stdout ?? '';
  const start = stdout.indexOf('{');
  let json = null;
  if (start >= 0) {
    try { json = JSON.parse(stdout.slice(start)); } catch { json = null; }
  }
  return { code: child.status, stdout, stderr: child.stderr ?? '', json, label };
}

function node(script, argv, label) {
  return shell(process.execPath, [join(ROOT, 'gesture-gate', script), ...argv], label);
}

function count(label, expected, actual) {
  return {
    check: label,
    verdict: actual === expected ? 'PASS' : 'FAIL',
    detail: `expected=${expected} actual=${actual}`,
  };
}

function gitCount(rangeArg) {
  const result = shell('git', ['rev-list', '--count', `${BASE}..HEAD`], 'commit count');
  return result.code === 0 ? Number(result.stdout.trim()) : -1;
}

function gitFiles() {
  const result = shell('git', ['diff', '--name-only', `${BASE}..HEAD`], 'file count');
  return result.code === 0
    ? result.stdout.split('\n').filter(Boolean).length
    : -1;
}

function collect() {
  const checks = [];
  const record = (check, verdict, detail) => checks.push({ check, verdict, detail });
  const commits = gitCount();
  const files = gitFiles();
  record('commit_range', commits > 0 ? 'PASS' : 'FAIL', `commits=${commits}`);
  record('file_range', files > 0 ? 'PASS' : 'FAIL', `files=${files}`);

  // Core evidence gates.
  const trace = node('gesture-trace-gate.mjs', ['--self-test', 'pass'], 'trace');
  record('trace', trace.code === 0 && trace.json?.status === 'PASS' ? 'PASS' : 'FAIL',
    `exit=${trace.code}`);
  const traceFault = node('gesture-trace-gate.mjs', ['--self-test', 'drift'], 'trace drift');
  record('trace_fault', traceFault.code === 1 ? 'PASS' : 'FAIL', `exit=${traceFault.code}`);

  for (const script of ['gesture-evidence-archive.mjs',
    'gesture-schema-consistency.mjs',
    'gesture-archive-freshness.mjs',
    'gesture-e2e-regression.mjs']) {
    const pass = node(script, ['--self-test', 'pass'], `${script} pass`);
    record(`${script}:pass`, pass.code === 0 ? 'PASS' : 'FAIL', `exit=${pass.code}`);
    const faults = script.includes('consistency')
      ? ['schema', 'source', 'missing']
      : script.includes('freshness')
        ? ['relabel', 'stale']
        : ['tamper', 'missing'];
    for (const fault of faults) {
      const injected = node(script, ['--self-test', fault], `${script} ${fault}`);
      record(`${script}:${fault}`, injected.code === 1 ? 'PASS' : 'FAIL',
        `exit=${injected.code}`);
    }
  }

  for (const gate of GATES) {
    const pass = node(`gesture-${gate}-gate.mjs`, ['--self-test', 'pass'], `${gate} pass`);
    const na = node(`gesture-${gate}-gate.mjs`, ['--self-test', 'na'], `${gate} na`);
    const fail = node(`gesture-${gate}-gate.mjs`, ['--self-test', 'fail'], `${gate} fail`);
    record(`${gate}:pass`, pass.code === 0 ? 'PASS' : 'FAIL', `exit=${pass.code}`);
    record(`${gate}:na`, na.code === 0 ? 'PASS' : 'FAIL', `exit=${na.code}`);
    record(`${gate}:fail`, fail.code === 1 ? 'PASS' : 'FAIL', `exit=${fail.code}`);
  }

  for (const mode of ['pass', 'degrade', 'missing', 'parallel_faults',
    'gate_faults', 'baseline_faults']) {
    const result = node('gesture-baseline-loop.mjs', ['--self-test', mode], mode);
    const expected = mode === 'pass' ? 0 : 1;
    record(`baseline_loop:${mode}`, result.code === expected ? 'PASS' : 'FAIL',
      `exit=${result.code}`);
  }

  const failed = checks.filter((item) => item.verdict === 'FAIL');
  return {
    status: failed.length === 0 ? 'PASS' : 'FAIL',
    source: SOURCE,
    scope: {
      base: BASE,
      commits,
      files,
    },
    checks,
    failed: failed.length,
  };
}

function main() {
  const args = parseArgs(process.argv);
  if (args.selfTest) {
    const simulated = args.selfTest === 'fault';
    const report = {
      schema: SCHEMA,
      status: simulated ? 'FAIL' : 'PASS',
      source: SOURCE,
      scope: { base: BASE, commits: simulated ? -1 : 1, files: simulated ? -1 : 1 },
      checks: [count('self_test', simulated ? 0 : 1, simulated ? -1 : 1)],
      failed: simulated ? 1 : 0,
    };
    console.log(JSON.stringify(report, null, 2));
    return simulated ? 1 : 0;
  }
  const result = collect();
  const report = {
    schema: SCHEMA,
    status: result.status,
    source: SOURCE,
    scope: result.scope,
    checks: result.checks,
    failed: result.failed,
  };
  if (args.out) {
    const path = resolve(args.out);
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, `${JSON.stringify(report, null, 2)}\n`);
    console.log(`RELEASE_REPORT=${path}`);
  } else {
    console.log(JSON.stringify(report, null, 2));
  }
  return result.status === 'PASS' ? 0 : 1;
}

try {
  process.exitCode = main();
} catch (err) {
  console.error(`ERROR ${err.message}`);
  process.exitCode = 2;
}
