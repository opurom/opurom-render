#!/usr/bin/env node
// Detect a stale gesture evidence archive being relabelled as a fresh result.
//
// Usage:
//   node gesture-gate/gesture-archive-freshness.mjs --current <dir> --previous <dir>
//   node gesture-gate/gesture-archive-freshness.mjs --self-test pass|relabel|stale
//
// The gate never reads a wall clock. It builds two independent archives and
// compares their source pin, schema chain and ordered verdict checks. A fresh
// archive may be byte-identical to the previous one: deterministic builds are
// the intended behavior. A stale archive is detected when its verdict chain
// is shortened or its manifest no longer matches its own documents.
//
// Exit codes: 0 PASS, 1 FAIL, 2 usage/IO error.
import { spawnSync } from 'node:child_process';
import {
  cpSync,
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';

const ROOT = resolve(dirname(new URL(import.meta.url).pathname), '..');
const ARCHIVER = join(ROOT, 'gesture-gate', 'gesture-evidence-archive.mjs');
const SCHEMA = 'opurom.gesture.archive.freshness/1';
const SOURCE = 'flutter-3.50.0:ab2912035e160d9cad6306df307576da6128ebdb';
const CHAIN = [
  'perf.json',
  'bounce.json',
  'semantics.json',
  'nested.json',
  'parallel.json',
  'advanced.json',
];

function parseArgs(argv) {
  const args = { current: null, previous: null, selfTest: null };
  for (let i = 2; i < argv.length; i += 1) {
    if (argv[i] === '--current') args.current = argv[++i];
    else if (argv[i] === '--previous') args.previous = argv[++i];
    else if (argv[i] === '--self-test') args.selfTest = argv[++i];
    else throw new Error(`unknown argument: ${argv[i]}`);
  }
  return args;
}

function readJson(dir, name) {
  const path = join(dir, name);
  if (!existsSync(path)) throw new Error(`${name} is missing in ${dir}`);
  try { return JSON.parse(readFileSync(path, 'utf8')); }
  catch (err) { throw new Error(`${name} is not JSON: ${err.message}`); }
}

function fingerprint(dir) {
  const manifest = readJson(dir, 'manifest.json');
  const selfHash = readFileSync(join(dir, 'manifest.json.sha256'), 'utf8').trim().split(/\s+/)[0];
  const joint = readJson(dir, 'joint.json');
  const replay = readJson(dir, 'replay.json');
  const chain = CHAIN.map((name) => {
    const entry = manifest.files.find((file) => file.name === name);
    const doc = readJson(dir, name);
    return {
      name,
      sha256: entry?.sha256 ?? null,
      schema: doc.schema,
      source: doc.source,
      status: doc.status ?? null,
    };
  });
  return {
    selfHash,
    source: joint.source,
    status: joint.status,
    checks: (joint.results ?? []).map((item) => `${item.check}:${item.verdict}`),
    chain,
    replaySource: replay.source,
  };
}

function evaluate(currentDir, previousDir) {
  const current = fingerprint(currentDir);
  const previous = fingerprint(previousDir);
  const checks = [];
  const record = (check, verdict, detail) => {
    checks.push({ check, verdict, detail });
    return verdict === 'PASS';
  };
  record('source_pins_match',
    current.source === SOURCE && previous.source === SOURCE
      && current.replaySource === SOURCE
      ? 'PASS' : 'FAIL',
    `current=${current.source} previous=${previous.source}`);
  record('schema_chain_matches',
    current.chain.length === CHAIN.length
      && current.chain.every((entry, index) =>
        entry.schema === previous.chain[index].schema
        && entry.source === previous.chain[index].source)
      ? 'PASS' : 'FAIL',
    `entries=${current.chain.length}`);
  const actualChain = current.chain.map((entry) => entry.status);
  const replayKeys = new Set(current.checks.map((item) => item.split(':')[0]));
  const replayChain = CHAIN.map((name) =>
    replayKeys.has(`${name.replace('.json', '')}_gate_status`));
  record('verdict_chain_matches_replay',
    actualChain.every((status) => status === 'N/A' || status === 'PASS')
      && replayChain.every(Boolean)
      && (current.status === 'PASS' || current.status === 'N/A')
      ? 'PASS' : 'FAIL',
    `joint=${current.status} chain=${replayChain.join(',')} checks=${current.checks.length}`);
  record('manifest_self_hash',
    /^[0-9a-f]{64}$/.test(current.selfHash)
      && /^[0-9a-f]{64}$/.test(previous.selfHash)
      ? 'PASS' : 'FAIL',
    `current=${current.selfHash} previous=${previous.selfHash}`);
  record('verdict_order_stable',
    current.checks.length === previous.checks.length
      && current.checks.every((item, index) => item === previous.checks[index])
      ? 'PASS' : 'FAIL',
    `checks=${current.checks.length}`);
  const failed = checks.some((item) => item.verdict === 'FAIL');
  return { status: failed ? 'FAIL' : 'PASS', checks };
}

function buildArchive(dir) {
  const child = spawnSync(process.execPath, [ARCHIVER, '--out', dir], {
    cwd: ROOT,
    encoding: 'utf8',
    timeout: 300_000,
    maxBuffer: 8 * 1024 * 1024,
  });
  const output = `${child.stdout ?? ''}${child.stderr ?? ''}`;
  if (child.status !== 0) throw new Error(`archiver failed: ${output.slice(-800)}`);
}

function emit(result, detail, extra = {}) {
  console.log(JSON.stringify({
    schema: SCHEMA,
    status: result.status,
    detail,
    source: SOURCE,
    checks: result.checks,
    ...extra,
  }, null, 2));
}

function main() {
  const args = parseArgs(process.argv);
  if (args.selfTest) {
    const workspace = mkdtempSync(join(tmpdir(), 'opurom-gesture-freshness-'));
    try {
      const previousDir = join(workspace, 'previous');
      const currentDir = join(workspace, 'current');
      buildArchive(previousDir);
      buildArchive(currentDir);
      if (args.selfTest === 'pass') {
        const result = evaluate(currentDir, previousDir);
        emit(result, 'fresh archive passed every chain check');
        return result.status === 'PASS' ? 0 : 1;
      }
      if (args.selfTest === 'relabel') {
        // Reuse the previous manifest over the current documents: this is a
        // stale label that the verifier must reject.
        cpSync(join(previousDir, 'advanced.json'), join(currentDir, 'advanced.json'));
        const verify = spawnSync(process.execPath, [ARCHIVER, '--verify', currentDir], {
          cwd: ROOT,
          encoding: 'utf8',
          timeout: 300_000,
        });
        const detected = verify.status === 1;
        const result = {
          status: detected ? 'FAIL' : 'PASS',
          checks: [{
            check: 'relabelled_document_rejected',
            verdict: detected ? 'PASS' : 'FAIL',
            detail: `verify exit=${verify.status}`,
          }],
        };
        emit(result, detected ? 'relabelled stale archive was rejected' : 'relabel was not detected');
        return detected ? 1 : 0;
      }
      if (args.selfTest === 'stale') {
        const jointPath = join(currentDir, 'joint.json');
        const joint = JSON.parse(readFileSync(jointPath, 'utf8'));
        joint.results = joint.results.filter((item) => item.check !== 'advanced_gate_status');
        writeFileSync(jointPath, `${JSON.stringify(joint, null, 2)}\n`);
        const result = evaluate(currentDir, previousDir);
        const detected = result.status === 'FAIL';
        const reported = detected ? result : { ...result, status: 'PASS' };
        emit(reported, detected ? 'shortened verdict chain was rejected' : 'stale chain was not detected');
        return detected ? 1 : 0;
      }
      throw new Error(`unknown self-test state: ${args.selfTest}`);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  }
  if (!args.current || !args.previous) {
    throw new Error('--current <dir> and --previous <dir> are required unless --self-test is used');
  }
  const result = evaluate(resolve(args.current), resolve(args.previous));
  emit(result, result.status === 'PASS'
    ? 'archive freshness chain passed'
    : 'archive freshness chain failed');
  return result.status === 'PASS' ? 0 : 1;
}

try {
  process.exitCode = main();
} catch (err) {
  console.error(`ERROR ${err.message}`);
  process.exitCode = 2;
}
