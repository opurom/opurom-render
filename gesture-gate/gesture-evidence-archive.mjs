#!/usr/bin/env node
// Archive joint gesture evidence as fixed JSON with SHA-256 manifests.
// Additive only: never edits shared CI, IR, ABI or renderer sources.
//
// Usage:
//   node gesture-gate/gesture-evidence-archive.mjs --out <dir> [--joint <json>]
//   node gesture-gate/gesture-evidence-archive.mjs --verify <dir>
//   node gesture-gate/gesture-evidence-archive.mjs --self-test pass|tamper|missing
//
// Exit codes: 0 PASS, 1 FAIL, 2 usage/IO error.
import { spawnSync } from 'node:child_process';
import {
  createHash,
} from 'node:crypto';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';

const ROOT = resolve(dirname(new URL(import.meta.url).pathname), '..');
const JOINT_GATE = join(ROOT, 'gesture-gate', 'gesture-fairness-perf-gate.mjs');
const SCHEMA = 'opurom.gesture.evidence/1';
const MANIFEST_SCHEMA = 'opurom.gesture.manifest/1';
const FILES = {
  joint: 'joint.json',
  perf: 'perf.json',
  fairness: 'fairness.json',
  bounce: 'bounce.json',
  semantics: 'semantics.json',
  nested: 'nested.json',
  parallel: 'parallel.json',
  advanced: 'advanced.json',
  replay: 'replay.json',
};

function parseArgs(argv) {
  const args = { out: null, joint: null, verify: null, selfTest: null };
  for (let i = 2; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--out') args.out = argv[++i];
    else if (arg === '--joint') args.joint = argv[++i];
    else if (arg === '--verify') args.verify = argv[++i];
    else if (arg === '--self-test') args.selfTest = argv[++i];
    else throw new Error(`unknown argument: ${arg}`);
  }
  return args;
}

function sha256(text) {
  return createHash('sha256').update(text).digest('hex');
}

function stableJson(value) {
  if (Array.isArray(value)) return `[${value.map(stableJson).join(',')}]`;
  if (value && typeof value === 'object') {
    return `{${Object.keys(value).sort().map((key) =>
      `${JSON.stringify(key)}:${stableJson(value[key])}`).join(',')}}`;
  }
  return JSON.stringify(value);
}

function readJson(path, label, schema) {
  if (!path || !existsSync(resolve(path))) throw new Error(`${label} file missing: ${path}`);
  let value;
  try { value = JSON.parse(readFileSync(resolve(path), 'utf8')); }
  catch (err) { throw new Error(`${label} is not JSON: ${err.message}`); }
  if (schema && value?.schema !== schema) {
    throw new Error(`${label} schema ${value?.schema}; expected ${schema}`);
  }
  return value;
}

function generateJoint() {
  const child = spawnSync(process.execPath, [JOINT_GATE, '--raw'], {
    cwd: ROOT,
    encoding: 'utf8',
    timeout: 180_000,
    maxBuffer: 8 * 1024 * 1024,
  });
  const output = `${child.stdout ?? ''}${child.stderr ?? ''}`;
  if (child.status !== 0) throw new Error(`joint gate failed (${child.status}): ${output.slice(-1000)}`);
  const start = output.indexOf('{');
  if (start < 0) throw new Error('joint gate produced no JSON');
  const value = JSON.parse(output.slice(start));
  if (value.schema !== 'opurom.gesture.joint/1') throw new Error('joint gate schema mismatch');
  return value;
}

function canonicalInputs(joint) {
  const perf = joint?.perf ?? null;
  const fairness = joint?.fairness ?? null;
  const bounce = joint?.bounce ?? null;
  const semantics = joint?.semantics ?? null;
  const nested = joint?.nested ?? null;
  const parallel = joint?.parallel ?? null;
  const advanced = joint?.advanced ?? null;
  if (!perf || !fairness || !bounce || !semantics || !nested || !parallel || !advanced) {
    throw new Error('joint evidence lacks perf, fairness, bounce, semantics, nested, parallel or advanced data');
  }
  if (!Array.isArray(perf.results) || perf.results.length === 0) {
    throw new Error('perf evidence lacks machine results');
  }
  if (!Array.isArray(fairness.cases) || fairness.cases.length !== 7) {
    throw new Error('fairness evidence must contain exactly seven cases');
  }
  if (!Array.isArray(bounce.results) || bounce.results.length === 0) {
    throw new Error('bounce evidence lacks machine results');
  }
  if (!Array.isArray(semantics.results) || semantics.results.length === 0) {
    throw new Error('semantics evidence lacks machine results');
  }
  if (!Array.isArray(nested.results) || nested.results.length === 0) {
    throw new Error('nested evidence lacks machine results');
  }
  if (!Array.isArray(parallel.results) || parallel.results.length === 0) {
    throw new Error('parallel evidence lacks machine results');
  }
  if (!Array.isArray(advanced.results) || advanced.results.length === 0) {
    throw new Error('advanced evidence lacks machine results');
  }
  const perfCanonical = {
    schema: perf.schema,
    status: perf.status,
    source: perf.source,
    clock: perf.clock ?? 'logical',
    metrics: Object.fromEntries(perf.results.map((item) => [item.metric, item.detail])),
  };
  const fairnessCanonical = {
    schema: fairness.schema,
    source: fairness.source,
    test: fairness.test,
    cases: fairness.cases.slice().sort(),
    runner_status: fairness.runner_status,
  };
  const bounceCanonical = {
    schema: bounce.schema,
    status: bounce.status,
    source: bounce.source,
    metrics: Object.fromEntries(bounce.results.map((item) => [item.metric, item.detail])),
  };
  const semanticsCanonical = {
    schema: semantics.schema,
    status: semantics.status,
    source: semantics.source,
    digest: semantics.digest ?? null,
    metrics: Object.fromEntries(semantics.results.map((item) => [item.metric, item.detail])),
  };
  const nestedCanonical = {
    schema: nested.schema,
    status: nested.status,
    source: nested.source,
    metrics: Object.fromEntries(nested.results.map((item) => [item.metric, item.detail])),
  };
  const parallelCanonical = {
    schema: parallel.schema,
    status: parallel.status,
    source: parallel.source,
    metrics: Object.fromEntries(parallel.results.map((item) => [item.metric, item.detail])),
  };
  const advancedCanonical = {
    schema: advanced.schema,
    status: advanced.status,
    source: advanced.source,
    metrics: Object.fromEntries(advanced.results.map((item) => [item.metric, item.detail])),
  };
  const replayCanonical = {
    schema: 'opurom.gesture.replay/1',
    source: fairness.source,
    cases: fairnessCanonical.cases,
    perf_order: perfCanonical.metrics,
    bounce_order: bounceCanonical.metrics,
    semantics_order: semanticsCanonical.metrics,
    nested_order: nestedCanonical.metrics,
    parallel_order: parallelCanonical.metrics,
    advanced_order: advancedCanonical.metrics,
  };
  return {
    perfCanonical,
    fairnessCanonical,
    bounceCanonical,
    semanticsCanonical,
    nestedCanonical,
    parallelCanonical,
    advancedCanonical,
    replayCanonical,
  };
}

function writeArchive(outDir, joint) {
  const {
    perfCanonical,
    fairnessCanonical,
    bounceCanonical,
    semanticsCanonical,
    nestedCanonical,
    parallelCanonical,
    advancedCanonical,
    replayCanonical,
  } = canonicalInputs(joint);
  const documents = {
    joint: {
      schema: 'opurom.gesture.joint.evidence/1',
      status: joint.status,
      source: joint.source,
      note: joint.note,
      baseline: joint.baseline ?? null,
      results: joint.results,
    },
    perf: perfCanonical,
    fairness: fairnessCanonical,
    bounce: bounceCanonical,
    semantics: semanticsCanonical,
    nested: nestedCanonical,
    parallel: parallelCanonical,
    advanced: advancedCanonical,
    replay: replayCanonical,
  };
  mkdirSync(resolve(outDir), { recursive: true });
  const entries = [];
  for (const [key, filename] of Object.entries(FILES)) {
    const text = `${stableJson(documents[key])}\n`;
    writeFileSync(join(resolve(outDir), filename), text);
    entries.push({ name: filename, bytes: Buffer.byteLength(text), sha256: sha256(text) });
  }
  const manifest = {
    schema: MANIFEST_SCHEMA,
    archive: SCHEMA,
    source: joint.source,
    files: entries,
  };
  const manifestText = `${stableJson(manifest)}\n`;
  writeFileSync(join(resolve(outDir), 'manifest.json'), manifestText);
  const selfHash = sha256(manifestText);
  writeFileSync(join(resolve(outDir), 'manifest.json.sha256'), `${selfHash}  manifest.json\n`);
  return { manifest, selfHash, documents };
}

function verifyArchive(dir) {
  const root = resolve(dir);
  const manifestPath = join(root, 'manifest.json');
  if (!existsSync(manifestPath)) throw new Error('manifest.json is missing');
  const manifestText = readFileSync(manifestPath, 'utf8');
  const manifest = readJson(manifestPath, 'manifest', MANIFEST_SCHEMA);
  const expectedSelf = readFileSync(join(root, 'manifest.json.sha256'), 'utf8').trim().split(/\s+/)[0];
  if (sha256(manifestText) !== expectedSelf) {
    return { ok: false, detail: 'manifest.json hash does not match manifest.json.sha256' };
  }
  for (const file of manifest.files) {
    const path = join(root, file.name);
    if (!existsSync(path)) return { ok: false, detail: `${file.name} is missing` };
    const text = readFileSync(path, 'utf8');
    if (sha256(text) !== file.sha256) return { ok: false, detail: `${file.name} hash mismatch` };
    if (Buffer.byteLength(text) !== file.bytes) return { ok: false, detail: `${file.name} size mismatch` };
  }
  // Replay consistency: serialize twice from the archived documents using a
  // fixed ordering and confirm the manifest hash remains identical.
  const replayed = stableJson(readJson(join(root, FILES.replay), 'replay', null));
  if (sha256(`${replayed}\n`) !== manifest.files.find((f) => f.name === FILES.replay).sha256) {
    return { ok: false, detail: 'replay JSON does not hash to the manifest entry' };
  }
  return {
    ok: true,
    detail: `verified ${manifest.files.length} files`,
    manifest_sha256: expectedSelf,
  };
}

function emit(status, detail, extra = {}) {
  console.log(JSON.stringify({
    schema: 'opurom.gesture.archive.result/1',
    status,
    detail,
    ...extra,
  }, null, 2));
}

function main() {
  const args = parseArgs(process.argv);
  if (args.selfTest) {
    const tmp = mkdtempSync(join(tmpdir(), 'opurom-gesture-archive-'));
    try {
      const joint = generateJoint();
      const archived = writeArchive(tmp, joint);
      if (args.selfTest === 'pass') {
        const verdict = verifyArchive(tmp);
        emit(verdict.ok ? 'PASS' : 'FAIL', verdict.detail, { files: archived.manifest.files.length });
        return verdict.ok ? 0 : 1;
      }
      if (args.selfTest === 'tamper') {
        writeFileSync(join(tmp, FILES.fairness), `${stableJson({ tampered: true })}\n`);
        const verdict = verifyArchive(tmp);
        const failed = !verdict.ok;
        emit(failed ? 'FAIL' : 'PASS', verdict.detail, { expected: 'fairness hash mismatch' });
        return failed ? 1 : 0;
      }
      if (args.selfTest === 'missing') {
        rmSync(join(tmp, FILES.perf), { force: true });
        const verdict = verifyArchive(tmp);
        const failed = !verdict.ok;
        emit(failed ? 'FAIL' : 'PASS', verdict.detail, { expected: 'perf.json missing' });
        return failed ? 1 : 0;
      }
      throw new Error(`unknown self-test state: ${args.selfTest}`);
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  }
  if (args.verify) {
    const verdict = verifyArchive(args.verify);
    emit(verdict.ok ? 'PASS' : 'FAIL', verdict.detail, verdict);
    return verdict.ok ? 0 : 1;
  }
  if (!args.out) throw new Error('--out <dir> is required unless --verify or --self-test is used');
  const joint = args.joint ? readJson(args.joint, 'joint', 'opurom.gesture.joint/1') : generateJoint();
  const archived = writeArchive(args.out, joint);
  const verdict = verifyArchive(args.out);
  emit(verdict.ok ? 'PASS' : 'FAIL', verdict.detail, {
    out: resolve(args.out),
    manifest_sha256: archived.selfHash,
    files: archived.manifest.files,
  });
  return verdict.ok ? 0 : 1;
}

try {
  process.exitCode = main();
} catch (err) {
  console.error(`ERROR ${err.message}`);
  process.exitCode = 2;
}
