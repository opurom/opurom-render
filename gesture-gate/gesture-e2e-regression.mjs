#!/usr/bin/env node
// End-to-end regression for the gesture joint gate and its evidence archive.
//
// Usage:
//   node gesture-gate/gesture-e2e-regression.mjs --out <dir>
//   node gesture-gate/gesture-e2e-regression.mjs --self-test pass|tamper|missing
//
// The script drives the real binaries only through their public entry points:
//   1. gesture-fairness-perf-gate.mjs --write-baseline -> frozen joint baseline
//   2. gesture-fairness-perf-gate.mjs --baseline --raw    -> raw joint evidence
//   3. gesture-evidence-archive.mjs --out --joint        -> SHA-256 archive
//   4. gesture-evidence-archive.mjs --verify              -> manifest check
//   5. gesture-fairness-perf-gate.mjs --baseline --raw    -> replayed verdict
//   6. gesture-schema-consistency.mjs --archive           -> schema/source parity
//   7. injected tamper / missing / verdict drift          -> expected FAIL
//
// Additive only: it never edits shared CI, IR, ABI or renderer sources.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  appendFileSync,
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
const JOINT_GATE = join(ROOT, 'gesture-gate', 'gesture-fairness-perf-gate.mjs');
const ARCHIVER = join(ROOT, 'gesture-gate', 'gesture-evidence-archive.mjs');
const CONSISTENCY = join(ROOT, 'gesture-gate', 'gesture-schema-consistency.mjs');
const FRESHNESS = join(ROOT, 'gesture-gate', 'gesture-archive-freshness.mjs');
const BASELINE_LOOP = join(ROOT, 'gesture-gate', 'gesture-baseline-loop.mjs');
const E2E_SCHEMA = 'opurom.gesture.e2e/1';
const SOURCE = 'flutter-3.50.0:ab2912035e160d9cad6306df307576da6128ebdb';
const ARCHIVE_FILES = [
  'joint.json',
  'perf.json',
  'fairness.json',
  'bounce.json',
  'semantics.json',
  'nested.json',
  'parallel.json',
  'advanced.json',
  'replay.json',
];

function parseArgs(argv) {
  const args = { out: null, selfTest: null };
  for (let i = 2; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--out') args.out = argv[++i];
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

function runNode(script, argv, label) {
  const child = spawnSync(process.execPath, [script, ...argv], {
    cwd: ROOT,
    encoding: 'utf8',
    timeout: 300_000,
    maxBuffer: 8 * 1024 * 1024,
  });
  const stdout = child.stdout ?? '';
  const stderr = child.stderr ?? '';
  let json = null;
  const start = stdout.indexOf('{');
  if (start >= 0) {
    try { json = JSON.parse(stdout.slice(start)); }
    catch { json = null; }
  }
  return { code: child.status, stdout, stderr, json, label };
}

function describeFailure(result) {
  const tail = `${result.stdout}${result.stderr}`.trim().split('\n').slice(-4).join(' | ');
  return `${result.label} exited ${result.code}: ${tail}`;
}

function copyTree(src, dest) {
  cpSync(src, dest, { recursive: true });
  return dest;
}

// Step 1-3: freeze a baseline, produce raw joint evidence and archive it.
function buildArchive(archiveDir) {
  const workspace = mkdtempSync(join(tmpdir(), 'opurom-gesture-e2e-'));
  const baselinePath = join(workspace, 'baseline.json');
  const baseline = runNode(JOINT_GATE, ['--write-baseline', baselinePath], 'joint baseline');
  if (baseline.code !== 0) throw new Error(describeFailure(baseline));
  if (!existsSync(baselinePath)) throw new Error('joint baseline file was not written');

  const evidence = runNode(JOINT_GATE, ['--baseline', baselinePath, '--raw'], 'joint evidence');
  if (evidence.code !== 0) throw new Error(describeFailure(evidence));
  if (evidence.json?.schema !== 'opurom.gesture.joint/1') {
    throw new Error('joint evidence has an unexpected schema');
  }
  const evidencePath = join(workspace, 'evidence.json');
  writeFileSync(evidencePath, `${JSON.stringify(evidence.json, null, 2)}\n`);

  const archive = runNode(ARCHIVER, ['--out', archiveDir, '--joint', evidencePath], 'archive');
  if (archive.code !== 0) throw new Error(describeFailure(archive));
  if (archive.json?.status !== 'PASS') {
    throw new Error(`archiver did not report PASS: ${archive.json?.status}`);
  }
  return { workspace, baselinePath, evidence, evidencePath, archive };
}

// Step 4: verify the SHA-256 manifest of an archive directory.
function verifyArchive(archiveDir) {
  const result = runNode(ARCHIVER, ['--verify', archiveDir], 'verify');
  return result;
}

// The full regression: archive, manifest check, verdict replay and the two
// injected fault probes.
function runRegression(archiveDir) {
  const checks = [];
  const record = (check, verdict, detail) => {
    checks.push({ check, verdict, detail });
    return verdict === 'PASS';
  };

  const built = buildArchive(archiveDir);
  const { workspace, baselinePath, evidence } = built;

  record('joint_evidence_status',
    evidence.json.status === 'PASS' ? 'PASS' : 'FAIL',
    `status=${evidence.json.status}`);

  const verify = verifyArchive(archiveDir);
  record('manifest_verified',
    verify.code === 0 && verify.json?.status === 'PASS' ? 'PASS' : 'FAIL',
    verify.json?.detail ?? describeFailure(verify));

  const manifestPath = join(archiveDir, 'manifest.json');
  const manifestText = readFileSync(manifestPath, 'utf8');
  const manifest = JSON.parse(manifestText);
  const manifestHash = sha256(manifestText);
  const recordedSelf = readFileSync(join(archiveDir, 'manifest.json.sha256'), 'utf8')
    .trim().split(/\s+/)[0];
  record('manifest_self_hash',
    manifestHash === recordedSelf ? 'PASS' : 'FAIL',
    `sha256=${manifestHash}`);

  const archivedFiles = ARCHIVE_FILES.every((name) => existsSync(join(archiveDir, name)));
  record('archive_files_present',
    archivedFiles && manifest.files.length === ARCHIVE_FILES.length ? 'PASS' : 'FAIL',
    `files=${manifest.files.length}`);

  // The archived bounce evidence must carry the same metrics as the joint
  // verdict it was built from.
  const archivedBounce = JSON.parse(readFileSync(join(archiveDir, 'bounce.json'), 'utf8'));
  const jointBounceMetrics = Object.fromEntries(
    (evidence.json.bounce?.results ?? []).map((item) => [item.metric, item.detail]),
  );
  record('bounce_evidence_stable',
    stableJson(archivedBounce.metrics ?? {}) === stableJson(jointBounceMetrics) ? 'PASS' : 'FAIL',
    `metrics=${Object.keys(archivedBounce.metrics ?? {}).length}`);

  // The archived semantics evidence must carry the same metrics and digest
  // as the joint verdict it was built from.
  const archivedSemantics = JSON.parse(readFileSync(join(archiveDir, 'semantics.json'), 'utf8'));
  const jointSemanticsMetrics = Object.fromEntries(
    (evidence.json.semantics?.results ?? []).map((item) => [item.metric, item.detail]),
  );
  record('semantics_evidence_stable',
    stableJson(archivedSemantics.metrics ?? {}) === stableJson(jointSemanticsMetrics)
      && (archivedSemantics.digest ?? null) === (evidence.json.semantics?.digest ?? null)
      ? 'PASS' : 'FAIL',
    `metrics=${Object.keys(archivedSemantics.metrics ?? {}).length}`);

  // The archived nested scroll evidence must carry the same metrics as the
  // joint verdict it was built from.
  const archivedNested = JSON.parse(readFileSync(join(archiveDir, 'nested.json'), 'utf8'));
  const jointNestedMetrics = Object.fromEntries(
    (evidence.json.nested?.results ?? []).map((item) => [item.metric, item.detail]),
  );
  record('nested_evidence_stable',
    stableJson(archivedNested.metrics ?? {}) === stableJson(jointNestedMetrics) ? 'PASS' : 'FAIL',
    `metrics=${Object.keys(archivedNested.metrics ?? {}).length}`);

  // The archived parallel pointer evidence must carry the same metrics as the
  // joint verdict it was built from.
  const archivedParallel = JSON.parse(readFileSync(join(archiveDir, 'parallel.json'), 'utf8'));
  const jointParallelMetrics = Object.fromEntries(
    (evidence.json.parallel?.results ?? []).map((item) => [item.metric, item.detail]),
  );
  record('parallel_evidence_stable',
    stableJson(archivedParallel.metrics ?? {}) === stableJson(jointParallelMetrics) ? 'PASS' : 'FAIL',
    `metrics=${Object.keys(archivedParallel.metrics ?? {}).length}`);

  // The archived advanced gesture evidence must carry the same metrics as the
  // joint verdict it was built from.
  const archivedAdvanced = JSON.parse(readFileSync(join(archiveDir, 'advanced.json'), 'utf8'));
  const jointAdvancedMetrics = Object.fromEntries(
    (evidence.json.advanced?.results ?? []).map((item) => [item.metric, item.detail]),
  );
  record('advanced_evidence_stable',
    stableJson(archivedAdvanced.metrics ?? {}) === stableJson(jointAdvancedMetrics) ? 'PASS' : 'FAIL',
    `metrics=${Object.keys(archivedAdvanced.metrics ?? {}).length}`);

  // Replay: regenerate the verdict from a fresh run and compare it with the
  // archived one. A deterministic gate must produce an identical verdict.
  const replay = runNode(JOINT_GATE, ['--baseline', baselinePath, '--raw'], 'joint replay');
  const replayStable = replay.code === 0
    && replay.json?.status === evidence.json.status
    && stableJson(replay.json?.results) === stableJson(evidence.json?.results)
    && stableJson(replay.json?.perf?.results) === stableJson(evidence.json?.perf?.results);
  record('replay_verdict_stable', replayStable ? 'PASS' : 'FAIL',
    replayStable ? 'replayed verdict matches the archived verdict'
      : `replay status=${replay.json?.status ?? 'none'}`);

  // Determinism of the archive itself: a second archive of the replayed
  // evidence must hash to the same manifest.
  const replayEvidencePath = join(workspace, 'replay-evidence.json');
  writeFileSync(replayEvidencePath, `${JSON.stringify(replay.json, null, 2)}\n`);
  const secondDir = join(workspace, 'archive-second');
  const second = runNode(ARCHIVER, ['--out', secondDir, '--joint', replayEvidencePath], 'archive replay');
  const secondHash = second.code === 0
    ? sha256(readFileSync(join(secondDir, 'manifest.json'), 'utf8'))
    : '';
  record('archive_deterministic',
    secondHash === manifestHash ? 'PASS' : 'FAIL',
    `manifest=${manifestHash}`);

  // Injected tamper: the verifier must reject a modified archive.
  const tamperedDir = copyTree(archiveDir, join(workspace, 'archive-tampered'));
  appendFileSync(join(tamperedDir, 'fairness.json'), ' ');
  const tamperResult = verifyArchive(tamperedDir);
  record('tamper_detected',
    tamperResult.code === 1 && tamperResult.json?.status === 'FAIL' ? 'PASS' : 'FAIL',
    tamperResult.json?.detail ?? describeFailure(tamperResult));

  // Injected missing evidence: the verifier must reject an incomplete archive.
  const missingDir = copyTree(archiveDir, join(workspace, 'archive-missing'));
  rmSync(join(missingDir, 'perf.json'), { force: true });
  const missingResult = verifyArchive(missingDir);
  record('missing_detected',
    missingResult.code === 1 && missingResult.json?.status === 'FAIL' ? 'PASS' : 'FAIL',
    missingResult.json?.detail ?? describeFailure(missingResult));

  // Cross-document schema and source parity over the archived evidence.
  const consistency = runNode(CONSISTENCY, ['--archive', archiveDir], 'schema consistency');
  record('schema_consistency',
    consistency.code === 0 && consistency.json?.status === 'PASS' ? 'PASS' : 'FAIL',
    consistency.json?.detail ?? describeFailure(consistency));

  // Injected verdict drift: a modified joint result must be rejected by the
  // consistency gate, closing the blind spot of single-file hash tampering.
  const driftDir = copyTree(archiveDir, join(workspace, 'archive-verdict-drift'));
  const driftPath = join(driftDir, 'joint.json');
  const drift = JSON.parse(readFileSync(driftPath, 'utf8'));
  drift.results = drift.results.map((item) =>
    item.check === 'perf_gate_status' ? { ...item, verdict: 'FAIL' } : item);
  writeFileSync(driftPath, `${JSON.stringify(drift, null, 2)}\n`);
  const driftResult = runNode(CONSISTENCY, ['--archive', driftDir], 'verdict drift');
  record('verdict_drift_detected',
    driftResult.code === 1
      && driftResult.json?.status === 'FAIL'
      ? 'PASS' : 'FAIL',
    driftResult.json?.detail ?? describeFailure(driftResult));

  // Archive freshness: compare the current archive with a second independent
  // build, then inject a stale document into a copy and require rejection.
  const previousDir = join(workspace, 'archive-previous');
  const previous = runNode(ARCHIVER,
    ['--out', previousDir, '--joint', built.evidencePath], 'previous archive');
  if (previous.code !== 0 || previous.json?.status !== 'PASS') {
    record('archive_freshness', 'FAIL', describeFailure(previous));
    record('stale_document_detected', 'FAIL', describeFailure(previous));
  } else {
    const freshness = runNode(FRESHNESS,
      ['--current', archiveDir, '--previous', previousDir], 'archive freshness');
    record('archive_freshness',
      freshness.code === 0 && freshness.json?.status === 'PASS' ? 'PASS' : 'FAIL',
      freshness.json?.detail ?? describeFailure(freshness));

    const staleDir = copyTree(archiveDir, join(workspace, 'archive-stale'));
    const staleDoc = JSON.parse(readFileSync(join(previousDir, 'advanced.json'), 'utf8'));
    staleDoc.status = 'FAIL';
    writeFileSync(join(staleDir, 'advanced.json'), `${JSON.stringify(staleDoc, null, 2)}\n`);
    const stale = verifyArchive(staleDir);
    record('stale_document_detected',
      stale.code === 1 && stale.json?.status === 'FAIL' ? 'PASS' : 'FAIL',
      stale.json?.detail ?? describeFailure(stale));
  }

  // The gates themselves must be falsifiable: an injected absolute-limit
  // fault and an injected baseline regression both have to be rejected.
  const gateFaults = runNode(BASELINE_LOOP, ['--self-test', 'gate_faults'], 'gate fault matrix');
  record('gate_faults_rejected',
    gateFaults.code === 1 ? 'PASS' : 'FAIL',
    `exit=${gateFaults.code}`);
  const baselineFaults = runNode(BASELINE_LOOP, ['--self-test', 'baseline_faults'],
    'baseline fault matrix');
  record('baseline_faults_rejected',
    baselineFaults.code === 1 ? 'PASS' : 'FAIL',
    `exit=${baselineFaults.code}`);

  rmSync(workspace, { recursive: true, force: true });
  const failed = checks.some((item) => item.verdict === 'FAIL');
  return { status: failed ? 'FAIL' : 'PASS', checks };
}

function emit(status, detail, extra = {}) {
  console.log(JSON.stringify({
    schema: E2E_SCHEMA,
    status,
    detail,
    source: SOURCE,
    ...extra,
  }, null, 2));
}

function main() {
  const args = parseArgs(process.argv);
  if (args.selfTest) {
    const workspace = mkdtempSync(join(tmpdir(), 'opurom-gesture-e2e-self-'));
    try {
      const archiveDir = join(workspace, 'archive');
      if (args.selfTest === 'pass') {
        const result = runRegression(archiveDir);
        emit(result.status, result.status === 'PASS'
          ? 'archive, manifest, replay and fault probes behaved as expected'
          : 'end-to-end regression failed', { checks: result.checks });
        return result.status === 'PASS' ? 0 : 1;
      }
      if (args.selfTest === 'tamper' || args.selfTest === 'missing') {
        const built = buildArchive(archiveDir);
        if (args.selfTest === 'tamper') {
          appendFileSync(join(archiveDir, 'fairness.json'), ' ');
        } else {
          rmSync(join(archiveDir, 'perf.json'), { force: true });
        }
        const verdict = verifyArchive(archiveDir);
        const failed = verdict.code === 1 && verdict.json?.status === 'FAIL';
        emit(failed ? 'FAIL' : 'PASS',
          failed ? `injected ${args.selfTest} fault was rejected` : `injected ${args.selfTest} fault was not rejected`,
          { expected: 'FAIL', detail_code: verdict.code });
        rmSync(built.workspace, { recursive: true, force: true });
        return failed ? 1 : 0;
      }
      throw new Error(`unknown self-test state: ${args.selfTest}`);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  }
  if (!args.out) throw new Error('--out <dir> is required unless --self-test is used');
  const result = runRegression(resolve(args.out));
  emit(result.status, result.status === 'PASS'
    ? 'end-to-end gesture gate regression passed'
    : 'end-to-end gesture gate regression failed',
  { out: resolve(args.out), checks: result.checks });
  return result.status === 'PASS' ? 0 : 1;
}

try {
  process.exitCode = main();
} catch (err) {
  console.error(`ERROR ${err.message}`);
  process.exitCode = 2;
}
