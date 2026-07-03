import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const fixtureDir = path.join(root, 'docs', 'api', 'fixtures', 'client-ci', 'v1');
const manifest = readJson(path.join(fixtureDir, 'manifest.json'));

const fixtures = new Map();
for (const entry of manifest.fixtures ?? []) {
  const fixture = readJson(path.join(fixtureDir, entry.file));
  assert.equal(fixture.fixture, entry.id, `${entry.file} fixture id mismatch`);
  fixtures.set(entry.id, fixture);
}

for (const entry of manifest.fixtures) {
  assert(fixtures.has(entry.id), `missing client CI fixture ${entry.id}`);
}

assertHarnessPlan(fixtures.get('harness-plan'));
assertCiJobs(fixtures.get('ci-jobs'));
assertDownstreamConsumption(fixtures.get('downstream-consumption'));
assertManualHardwareGates(fixtures.get('manual-hardware-gates'));
assertSeedData(fixtures.get('seed-data-profile'));
assertHarnessScript();
assertWorkflow();
assertNoFixtureLeaks();

console.log(`Verified ${fixtures.size} client CI smoke fixtures and ${manifest.required_ci_jobs.length} CI job definitions.`);

function assertHarnessPlan(fixture) {
  assert.equal(manifest.docker_target.default_port, 48027, 'client CI smoke target must use port 48027');
  assert(manifest.docker_target.readiness_url.includes(':48027/health/ready'), 'readiness URL must use :48027');
  assert.equal(fixture.default_base_url, manifest.docker_target.public_base_url, 'harness plan base URL must match manifest');
  const steps = byId(fixture.steps, 'id');
  for (const step of manifest.harness.required_steps) {
    const entry = steps.get(step);
    assert(entry, `missing harness step ${step}`);
    assert(nonEmpty(entry.command), `${step} missing command`);
    assert(nonEmpty(entry.expected_evidence), `${step} missing expected evidence`);
  }
  const checks = byId(fixture.public_surface_checks, 'id');
  for (const check of ['ready', 'live', 'events']) {
    assert(checks.has(check), `missing public surface check ${check}`);
  }
  for (const verifier of manifest.required_verifiers) {
    assert(
      fixture.contract_verifier_commands.includes(`node ${verifier}`),
      `harness plan missing verifier command for ${verifier}`
    );
    assert(fs.existsSync(path.join(root, verifier)), `missing verifier file ${verifier}`);
  }
}

function assertCiJobs(fixture) {
  const jobs = byId(fixture.jobs, 'id');
  for (const job of manifest.required_ci_jobs) {
    const entry = jobs.get(job);
    assert(entry, `missing CI job ${job}`);
    assert(nonEmpty(entry.runner), `${job} missing runner`);
    assertNonEmptyArray(entry.commands, `${job} missing commands`);
    assert(nonEmpty(entry.purpose), `${job} missing purpose`);
  }
  assert(fixture.policy.artifact_rule.includes('SBOM'), 'CI policy must mention SBOM');
  assert(/provenance|attestation/i.test(fixture.policy.artifact_rule), 'CI policy must mention provenance or attestation');
  assert(fixture.policy.secret_rule.includes('never checked into the repository'), 'CI policy must keep secrets out of the repository');
  const bindingJob = jobs.get('binding_generation_readiness');
  for (const target of ['TypeScript/Tauri', 'Dart/Flutter', 'Kotlin Android/Fire TV', 'Swift iOS/tvOS']) {
    assert(bindingJob.targets.includes(target), `binding job missing target ${target}`);
  }
}

function assertDownstreamConsumption(fixture) {
  assertNonEmptyArray(fixture.required_before_phase_complete, 'downstream consumption missing required commands');
  for (const command of [
    'node scripts/client-smoke-harness.mjs --plan',
    'node scripts/verify-client-ci-smoke.mjs',
    'node scripts/verify-client-contracts.mjs',
    'node scripts/verify-client-fixtures.mjs',
    'node scripts/verify-client-bindings.mjs'
  ]) {
    assert(fixture.required_before_phase_complete.includes(command), `downstream consumption missing ${command}`);
  }
  const phases = byId(fixture.phases, 'phase');
  for (const phase of manifest.required_downstream_phases) {
    const entry = phases.get(phase);
    assert(entry, `missing downstream phase ${phase}`);
    assertNonEmptyArray(entry.must_consume, `${phase} missing consumed fixtures`);
    assert(entry.release_gate.includes('Docker smoke harness'), `${phase} release gate must consume Docker smoke harness`);
  }
}

function assertManualHardwareGates(fixture) {
  assert(fixture.policy.ci_boundary.includes('CI may validate'), 'manual gate policy must define CI boundary');
  assert(fixture.policy.manual_boundary.includes('manual or release-gate'), 'manual gate policy must define manual boundary');
  const platforms = byId(fixture.platforms, 'platform');
  for (const platform of manifest.required_platforms) {
    const entry = platforms.get(platform);
    assert(entry, `missing manual hardware gate platform ${platform}`);
    assertNonEmptyArray(entry.manual_checks, `${platform} missing manual checks`);
  }
}

function assertSeedData(fixture) {
  assert.equal(fixture.profile, 'client_smoke_seed_v1', 'unexpected seed profile id');
  assertNonEmptyArray(fixture.media_roots, 'seed profile missing media roots');
  for (const mediaRoot of fixture.media_roots) {
    assert(!path.isAbsolute(mediaRoot.relative_path), `${mediaRoot.id} seed path must be relative`);
    assert(!mediaRoot.relative_path.includes('..'), `${mediaRoot.id} seed path must not escape repository`);
    assertNonEmptyArray(mediaRoot.files, `${mediaRoot.id} seed root missing files`);
    for (const file of mediaRoot.files) {
      assert(nonEmpty(file.name), `${mediaRoot.id} seed file missing name`);
      assert(nonEmpty(file.content), `${mediaRoot.id} seed file missing content`);
    }
  }
  for (const rule of fixture.redaction_rules) {
    assert(/Do not include/i.test(rule), `seed redaction rule must be explicit: ${rule}`);
  }
}

function assertHarnessScript() {
  const scriptPath = path.join(root, manifest.harness.script);
  assert(fs.existsSync(scriptPath), `missing harness script ${manifest.harness.script}`);
  const content = fs.readFileSync(scriptPath, 'utf8');
  for (const token of ['--plan', '--run', 'docker', 'compose', '48027', 'seedRepresentativeData', 'waitForReadiness']) {
    assert(content.includes(token), `harness script missing ${token}`);
  }
}

function assertWorkflow() {
  const workflowPath = path.join(root, '.github', 'workflows', 'client-ci-smoke.yml');
  assert(fs.existsSync(workflowPath), 'missing client CI smoke workflow');
  const content = fs.readFileSync(workflowPath, 'utf8');
  for (const job of manifest.required_ci_jobs) {
    assert(content.includes(job), `workflow missing job ${job}`);
  }
  for (const verifier of manifest.required_verifiers) {
    assert(content.includes(verifier), `workflow missing verifier ${verifier}`);
  }
  assert(content.includes('workflow_dispatch'), 'workflow must support manual release-gate runs');
  assert(content.includes('--run'), 'workflow must expose real Docker smoke run');
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function byId(items, key) {
  assert(Array.isArray(items), `expected array for ${key}`);
  return new Map(items.map((item) => [item[key], item]));
}

function assertNonEmptyArray(value, message) {
  assert(Array.isArray(value) && value.length > 0, message);
}

function nonEmpty(value) {
  return typeof value === 'string' ? value.trim().length > 0 : Boolean(value);
}

function assertNoFixtureLeaks() {
  const content = fs
    .readdirSync(fixtureDir)
    .filter((file) => file.endsWith('.json'))
    .map((file) => fs.readFileSync(path.join(fixtureDir, file), 'utf8'))
    .join('\n');
  const leakPatterns = [
    /Bearer\s+[A-Za-z0-9._-]+/,
    /Authorization:\s*[A-Za-z]/,
    /password=/i,
    /token=[A-Za-z0-9._-]+/,
    /signature=[A-Za-z0-9._-]+/,
    /X-Api-Key/i,
    /file:\/\//i,
    /C:\\Users\\/i,
    /\/Users\/[^/\s]+/,
    /\/home\/[^/\s]+/,
    /\/mnt\/media\//i
  ];
  for (const pattern of leakPatterns) {
    assert(!pattern.test(content), `client CI fixture content matched forbidden pattern ${pattern}`);
  }
}
