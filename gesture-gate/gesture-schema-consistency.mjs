#!/usr/bin/env node
// Validate schema/source parity across a gesture evidence archive.
//
// Usage:
//   node gesture-gate/gesture-schema-consistency.mjs --archive <dir>
//   node gesture-gate/gesture-schema-consistency.mjs --self-test pass|schema|source|missing
//
// Exit codes: 0 PASS, 1 FAIL, 2 usage/IO error.
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const ROOT = resolve(dirname(new URL(import.meta.url).pathname), '..');
const ARCHIVER = join(ROOT, 'gesture-gate', 'gesture-evidence-archive.mjs');
const SOURCE = 'flutter-3.50.0:ab2912035e160d9cad6306df307576da6128ebdb';
const SCHEMA = 'opurom.gesture.consistency/1';
const EXPECTED = {
  'joint.json': 'opurom.gesture.joint.evidence/1',
  'perf.json': 'opurom.gesture.gate/1',
  'fairness.json': 'opurom.gesture.fairness/1',
  'bounce.json': 'opurom.gesture.bounce.gate/1',
  'semantics.json': 'opurom.gesture.semantics.gate/1',
  'nested.json': 'opurom.gesture.nested.gate/1',
  'parallel.json': 'opurom.gesture.parallel.gate/1',
  'advanced.json': 'opurom.gesture.advanced.gate/1',
  'replay.json': 'opurom.gesture.replay/1',
  'manifest.json': 'opurom.gesture.manifest/1',
};
const JOINT_ORDER = [
  'perf.json',
  'bounce.json',
  'semantics.json',
  'nested.json',
  'parallel.json',
  'advanced.json',
];

function parseArgs(argv) {
  const args = { archive: null, selfTest: null };
  for (let i = 2; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--archive') args.archive = argv[++i];
    else if (arg === '--self-test') args.selfTest = argv[++i];
    else throw new Error(`unknown argument: ${arg}`);
  }
  return args;
}

function readJson(dir, name, optional = false) {
  const path = join(dir, name);
  if (!existsSync(path)) {
    if (optional) return null;
    throw new Error(`${name} is missing`);
  }
  try { return JSON.parse(readFileSync(path, 'utf8')); }
  catch (err) { throw new Error(`${name} is not JSON: ${err.message}`); }
}

function evaluate(dir) {
  const checks = [];
  const record = (check, verdict, detail) => {
    checks.push({ check, verdict, detail });
    return verdict === 'PASS';
  };
  const manifest = readJson(dir, 'manifest.json');
  const expectedNames = Object.keys(EXPECTED).filter((name) => name !== 'manifest.json');
  record('manifest_file_set',
    manifest.files.length === expectedNames.length
      && expectedNames.every((name) => manifest.files.some((file) => file.name === name))
      ? 'PASS' : 'FAIL',
    `files=${manifest.files.length}`);
  record('files_present',
    expectedNames.every((name) => existsSync(join(dir, name)))
      && existsSync(join(dir, 'manifest.json.sha256'))
      ? 'PASS' : 'FAIL',
    `expected=${expectedNames.length}`);
  let schemaOk = true;
  let sourceOk = true;
  const schemas = {};
  for (const [name, schema] of Object.entries(EXPECTED)) {
    const doc = readJson(dir, name, true);
    if (!doc) {
      schemaOk = false;
      sourceOk = false;
      schemas[name] = null;
      continue;
    }
    schemas[name] = doc.schema;
    if (doc.schema !== schema) schemaOk = false;
    if (doc.source !== SOURCE) sourceOk = false;
  }
  record('schema_versions', schemaOk ? 'PASS' : 'FAIL', `documents=${expectedNames.length}`);
  record('source_pin', sourceOk ? 'PASS' : 'FAIL', `source=${SOURCE}`);
  const joint = readJson(dir, 'joint.json', true);
  if (!joint) return { status: 'FAIL', checks, schemas };
  const jointCheckNames = new Set(
    (joint.results ?? [])
      .filter((item) => typeof item.check === 'string' && item.check.endsWith('_gate_status'))
      .map((item) => item.check.replace('_gate_status', '.json')),
  );
  const sourceNames = JOINT_ORDER.filter((name) => {
    const key = `${name.replace('.json', '')}_gate_status`;
    return (joint.results ?? []).some((item) => item.check === key);
  });
  const parity = sourceNames.length === JOINT_ORDER.length
    && sourceNames.every((name) => jointCheckNames.has(name));
  record('joint_evidence_parity', parity ? 'PASS' : 'FAIL',
    `sources=${sourceNames.length}/${JOINT_ORDER.length}`);
  const replay = readJson(dir, 'replay.json', true);
  if (!replay) return { status: 'FAIL', checks, schemas };
  const replayKeys = new Set(Object.keys(replay).filter((key) => key.endsWith('_order')));
  const complete = JOINT_ORDER.every((name) => replayKeys.has(`${name.replace('.json', '')}_order`));
  record('replay_projection_complete', complete ? 'PASS' : 'FAIL',
    `orders=${replayKeys.size}`);
  const failed = checks.some((item) => item.verdict === 'FAIL');
  return { status: failed ? 'FAIL' : 'PASS', checks, schemas };
}

function emit(result, detail) {
  console.log(JSON.stringify({
    schema: SCHEMA,
    status: result.status,
    detail,
    source: SOURCE,
    checks: result.checks,
    schemas: result.schemas,
  }, null, 2));
}

function buildArchive() {
  const dir = mkdtempSync(join(tmpdir(), 'opurom-gesture-consistency-'));
  const child = spawnSync(process.execPath, [ARCHIVER, '--out', dir], {
    cwd: ROOT,
    encoding: 'utf8',
    timeout: 300_000,
    maxBuffer: 8 * 1024 * 1024,
  });
  const output = `${child.stdout ?? ''}${child.stderr ?? ''}`;
  if (child.status !== 0) throw new Error(`archiver failed: ${output.slice(-800)}`);
  return dir;
}

function main() {
  const args = parseArgs(process.argv);
  if (args.selfTest) {
    const dir = buildArchive();
    try {
      if (args.selfTest === 'pass') {
        const result = evaluate(dir);
        emit(result, 'archive schemas and sources are aligned');
        return result.status === 'PASS' ? 0 : 1;
      }
      if (args.selfTest === 'schema') {
        const path = join(dir, 'parallel.json');
        const doc = JSON.parse(readFileSync(path, 'utf8'));
        doc.schema = 'opurom.gesture.unknown/9';
        writeFileSync(path, `${JSON.stringify(doc, null, 2)}\n`);
        const result = evaluate(dir);
        const failed = result.status === 'FAIL';
        emit(result, failed ? 'schema drift was rejected' : 'schema drift was not rejected');
        return failed ? 1 : 0;
      }
      if (args.selfTest === 'source') {
        const path = join(dir, 'advanced.json');
        const doc = JSON.parse(readFileSync(path, 'utf8'));
        doc.source = 'flutter-0.0.0:unknown';
        writeFileSync(path, `${JSON.stringify(doc, null, 2)}\n`);
        const result = evaluate(dir);
        const failed = result.status === 'FAIL';
        emit(result, failed ? 'source drift was rejected' : 'source drift was not rejected');
        return failed ? 1 : 0;
      }
      if (args.selfTest === 'missing') {
        rmSync(join(dir, 'nested.json'), { force: true });
        const result = evaluate(dir);
        const failed = result.status === 'FAIL';
        emit(result, failed ? 'missing evidence was rejected' : 'missing evidence was not rejected');
        return failed ? 1 : 0;
      }
      throw new Error(`unknown self-test state: ${args.selfTest}`);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  }
  if (!args.archive) throw new Error('--archive <dir> is required unless --self-test is used');
  const result = evaluate(resolve(args.archive));
  emit(result, result.status === 'PASS'
    ? 'archive schema and source consistency passed'
    : 'archive schema and source consistency failed');
  return result.status === 'PASS' ? 0 : 1;
}

try {
  process.exitCode = main();
} catch (err) {
  console.error(`ERROR ${err.message}`);
  process.exitCode = 2;
}
