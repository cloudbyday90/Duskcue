import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const fixtureDir = path.join(root, 'docs', 'api', 'fixtures', 'device-lab', 'v1');
const manifest = readJson(path.join(fixtureDir, 'manifest.json'));

const fixtures = new Map();
for (const entry of manifest.fixtures ?? []) {
  const fixture = readJson(path.join(fixtureDir, entry.file));
  assert.equal(fixture.fixture, entry.id, `${entry.file} fixture id mismatch`);
  fixtures.set(entry.id, fixture);
}

for (const entry of manifest.fixtures) {
  assert(fixtures.has(entry.id), `missing device lab fixture ${entry.id}`);
}

assert.equal(manifest.docker_target.default_port, 48027, 'device lab smoke target must use port 48027');
assert.equal(manifest.docker_target.readiness_path, '/health/ready', 'readiness path must remain /health/ready');

const requiredPlatforms = manifest.required_platforms;
assert.equal(new Set(requiredPlatforms).size, requiredPlatforms.length, 'required platform list contains duplicates');

assertDeviceMatrix(fixtures.get('device-matrix'));
assertMediaCapabilityMatrix(fixtures.get('media-capability-matrix'));
assertManualSmokeScripts(fixtures.get('manual-smoke-scripts'));
assertReleaseValidationPolicy(fixtures.get('release-validation-policy'));
assertKnownPlatformLimitations(fixtures.get('known-platform-limitations'));
assertHardwareGapReport(fixtures.get('hardware-gap-report'));
assertNoFixtureLeaks();

console.log(`Verified ${fixtures.size} device lab fixtures for ${requiredPlatforms.length} platforms.`);

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function assertDeviceMatrix(fixture) {
  const platforms = byId(fixture.platforms, 'id');
  for (const platform of requiredPlatforms) {
    const entry = platforms.get(platform);
    assert(entry, `missing device matrix platform ${platform}`);
    assert(entry.family, `${platform} missing family`);
    assert(entry.minimum_target, `${platform} missing minimum target`);
    assert(entry.minimum_target.os_version, `${platform} minimum target missing OS version`);
    assert(entry.minimum_target.release_validation, `${platform} minimum target missing release validation`);
    assert(Array.isArray(entry.representative_targets) && entry.representative_targets.length > 0, `${platform} missing representative targets`);
    assert(nonEmpty(entry.browser_webview_engine), `${platform} missing browser/webview engine`);
    assertNonEmptyArray(entry.remote_input, `${platform} missing remote input behavior`);
    assertNonEmptyArray(entry.storage_constraints, `${platform} missing storage constraints`);
    assertNonEmptyArray(entry.known_limitations, `${platform} missing known limitations`);
    assert(
      [entry.minimum_target, ...entry.representative_targets].some((target) =>
        /required/i.test(target.release_validation ?? '')
      ),
      `${platform} needs at least one release-required target`
    );
  }
}

function assertMediaCapabilityMatrix(fixture) {
  assert(fixture.baseline_direct_play, 'missing baseline direct-play profile');
  const platforms = byId(fixture.platforms, 'id');
  for (const platform of requiredPlatforms) {
    const entry = platforms.get(platform);
    assert(entry, `missing media capability platform ${platform}`);
    for (const field of manifest.required_capability_fields) {
      assert.notEqual(entry[field], undefined, `${platform} missing capability field ${field}`);
      if (Array.isArray(entry[field])) {
        assert(entry[field].length > 0, `${platform} capability field ${field} is empty`);
      } else {
        assert(nonEmpty(entry[field]), `${platform} capability field ${field} is empty`);
      }
    }
  }
}

function assertManualSmokeScripts(fixture) {
  assert.equal(fixture.docker_target.default_port, 48027, 'manual smoke scripts must target port 48027');
  assert(fixture.docker_target.readiness_url.includes(':48027/health/ready'), 'readiness URL must use :48027');
  const steps = byId(fixture.common_steps, 'id');
  for (const step of manifest.required_smoke_steps) {
    const entry = steps.get(step);
    assert(entry, `missing common smoke step ${step}`);
    assert(nonEmpty(entry.expected_evidence), `${step} missing expected evidence`);
  }
  const platformScripts = byId(fixture.platform_scripts, 'platform');
  for (const platform of requiredPlatforms) {
    const script = platformScripts.get(platform);
    assert(script, `missing smoke script platform ${platform}`);
    assertNonEmptyArray(script.setup, `${platform} smoke script missing setup`);
    assertNonEmptyArray(script.extra_steps, `${platform} smoke script missing extra steps`);
    assertNonEmptyArray(script.evidence, `${platform} smoke script missing evidence`);
  }
}

function assertReleaseValidationPolicy(fixture) {
  assert(fixture.policy.release_claim_rule.includes('release_required'), 'release policy must reference release_required');
  const platforms = byId(fixture.platforms, 'platform');
  for (const platform of requiredPlatforms) {
    const entry = platforms.get(platform);
    assert(entry, `missing release policy platform ${platform}`);
    assertNonEmptyArray(entry.release_required, `${platform} release policy missing required hardware`);
    assertNonEmptyArray(entry.best_effort, `${platform} release policy missing best-effort hardware`);
    assert.equal(typeof entry.blocked_without_hardware, 'boolean', `${platform} missing blocked_without_hardware boolean`);
  }
}

function assertKnownPlatformLimitations(fixture) {
  const platforms = byId(fixture.limitations, 'platform');
  for (const platform of requiredPlatforms) {
    const entry = platforms.get(platform);
    assert(entry, `missing limitation platform ${platform}`);
    assertNonEmptyArray(entry.items, `${platform} missing limitation items`);
    for (const item of entry.items) {
      assert(nonEmpty(item.id), `${platform} limitation missing id`);
      assert(nonEmpty(item.limitation), `${platform} limitation missing description`);
      assert(nonEmpty(item.workaround), `${platform} limitation missing workaround`);
      assert(nonEmpty(item.fallback), `${platform} limitation missing fallback`);
    }
  }
}

function assertHardwareGapReport(fixture) {
  assert(fixture.policy.allowed_gap_rule.includes('Phase 16d'), 'gap policy must allow Phase 16d definition-only gaps');
  const gaps = byId(fixture.gaps, 'platform');
  for (const platform of requiredPlatforms) {
    const entry = gaps.get(platform);
    assert(entry, `missing hardware gap platform ${platform}`);
    assert(nonEmpty(entry.gap), `${platform} gap missing description`);
    assert(nonEmpty(entry.required_before), `${platform} gap missing required_before`);
    assert.equal(typeof entry.release_blocking, 'boolean', `${platform} gap missing release_blocking boolean`);
  }
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
    assert(!pattern.test(content), `device lab fixture content matched forbidden pattern ${pattern}`);
  }
}
