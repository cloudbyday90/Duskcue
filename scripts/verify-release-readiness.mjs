/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 *
 * This program is free software: licensed under AGPL-3.0
 * See LICENSE file for details.
 */

import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const fixtureDir = path.join(root, 'docs', 'api', 'fixtures', 'release', 'v1');
const manifest = readJson(path.join(fixtureDir, 'manifest.json'));

const fixtures = new Map();
for (const entry of manifest.fixtures ?? []) {
  const fixture = readJson(path.join(fixtureDir, entry.file));
  assert.equal(fixture.fixture, entry.id, `${entry.file} fixture id mismatch`);
  fixtures.set(entry.id, fixture);
}

for (const entry of manifest.fixtures) {
  assert(fixtures.has(entry.id), `missing release fixture ${entry.id}`);
}

const requiredPlatforms = manifest.required_platforms;
assert.equal(new Set(requiredPlatforms).size, requiredPlatforms.length, 'required platform list contains duplicates');

assertPlatformReleaseChecklists(fixtures.get('platform-release-checklists'));
assertCiReleasePlaceholders(fixtures.get('ci-release-placeholders'));
assertVersioningPolicy(fixtures.get('versioning-policy'));
assertReleaseChannelPolicy(fixtures.get('release-channel-policy'));
assertSmokeRollbackPolicy(fixtures.get('smoke-rollback-policy'));
assertPrivacyPermissionsReview(fixtures.get('privacy-permissions-review'));
assertAndroidTvGooglePlayReadiness(fixtures.get('android-tv-google-tv-readiness'));
assertNoFixtureLeaks();

console.log(`Verified ${fixtures.size} release readiness fixtures for ${requiredPlatforms.length} platforms.`);

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function assertPlatformReleaseChecklists(fixture) {
  const platforms = byId(fixture.platforms, 'platform');
  for (const platform of requiredPlatforms) {
    const entry = platforms.get(platform);
    assert(entry, `missing release checklist platform ${platform}`);
    for (const field of manifest.required_identity_fields) {
      assert.notEqual(entry[field], undefined, `${platform} missing identity field ${field}`);
      if (Array.isArray(entry[field])) {
        assert(entry[field].length > 0, `${platform} identity field ${field} is empty`);
      } else {
        assert(nonEmpty(entry[field]), `${platform} identity field ${field} is empty`);
      }
    }
    assert(
      /placeholder|not applicable|Store signs|App Signing|Notarization/i.test(entry.certificate_or_key_material),
      `${platform} must describe certificate/key placeholder handling`
    );
    assert(
      /never checked into repository/i.test(entry.certificate_or_key_material) || /Store signs/i.test(entry.notarization_or_store_signing),
      `${platform} must keep signing material out of repository`
    );
  }
}

function assertCiReleasePlaceholders(fixture) {
  assert(fixture.policy.secrets_rule.includes('CI secrets'), 'CI policy must require secret storage');
  assert(fixture.policy.artifact_rule.includes('SBOM'), 'CI policy must require SBOM');
  assert(fixture.policy.artifact_rule.includes('provenance'), 'CI policy must require provenance');
  const jobs = byId(fixture.jobs, 'platform');
  for (const platform of requiredPlatforms) {
    const entry = jobs.get(platform);
    assert(entry, `missing CI placeholder platform ${platform}`);
    for (const field of manifest.required_ci_fields) {
      assert(nonEmpty(entry[field]), `${platform} missing CI field ${field}`);
    }
    assert(/\.(aab|apk|ipa|msi|msixupload|dmg|AppImage|pkg|wgt|ipk)$/.test(entry.artifact), `${platform} artifact needs release package extension`);
    assert(/sbom|spdx|cyclonedx/i.test(entry.sbom), `${platform} SBOM field must identify SBOM output`);
    assert(/attestation|provenance/i.test(entry.provenance), `${platform} provenance field must identify attestation/provenance output`);
  }
}

function assertVersioningPolicy(fixture) {
  assert(fixture.policy.human_version.includes('SemVer'), 'version policy must use SemVer-compatible public version');
  assert(fixture.policy.compatibility_rule.includes('server API contract'), 'version policy must require server contract compatibility');
  const targets = byId(fixture.targets, 'target');
  for (const target of ['server', 'web', 'desktop', 'mobile_android', 'mobile_ios', 'tv_clients']) {
    const entry = targets.get(target);
    assert(entry, `missing versioning target ${target}`);
    assert(nonEmpty(entry.version_source), `${target} missing version source`);
    assert(nonEmpty(entry.monotonic_field), `${target} missing monotonic field`);
    assert(nonEmpty(entry.display_field), `${target} missing display field`);
    assert(nonEmpty(entry.rule), `${target} missing versioning rule`);
  }
  assert(targets.get('mobile_android').rule.includes('versionCode'), 'Android rule must mention versionCode');
  assert(targets.get('mobile_ios').rule.includes('CFBundleVersion'), 'Apple rule must mention CFBundleVersion');
}

function assertReleaseChannelPolicy(fixture) {
  const channels = byId(fixture.channels, 'id');
  for (const channel of manifest.required_release_channels) {
    const entry = channels.get(channel);
    assert(entry, `missing release channel ${channel}`);
    assertNonEmptyArray(entry.allowed_artifacts, `${channel} missing allowed artifacts`);
    assert(nonEmpty(entry.promotion_gate), `${channel} missing promotion gate`);
    assert(nonEmpty(entry.rollback_expectation), `${channel} missing rollback expectation`);
  }
  const platformMap = byId(fixture.platform_channel_map, 'platform');
  for (const platform of requiredPlatforms) {
    const entry = platformMap.get(platform);
    assert(entry, `missing release channel map platform ${platform}`);
    for (const channel of manifest.required_release_channels) {
      assert(nonEmpty(entry[channel]), `${platform} missing ${channel} channel mapping`);
    }
  }
}

function assertSmokeRollbackPolicy(fixture) {
  assert.equal(fixture.docker_target.default_port, 48027, 'release smoke target must use port 48027');
  assert(fixture.docker_target.readiness_url.includes(':48027/health/ready'), 'readiness URL must use :48027');
  const commonSteps = byId(fixture.common_release_blocking_steps, 'id');
  for (const step of manifest.required_smoke_steps) {
    const entry = commonSteps.get(step);
    assert(entry, `missing common smoke step ${step}`);
    assert(nonEmpty(entry.expected_evidence), `${step} missing expected evidence`);
  }
  const platforms = byId(fixture.platforms, 'platform');
  for (const platform of requiredPlatforms) {
    const entry = platforms.get(platform);
    assert(entry, `missing smoke rollback platform ${platform}`);
    assertNonEmptyArray(entry.release_blocking_smoke, `${platform} missing release-blocking smoke tests`);
    assert(entry.release_blocking_smoke.length >= 4, `${platform} needs at least four release smoke tests`);
    assert(nonEmpty(entry.rollback_expectation), `${platform} missing rollback expectation`);
    assert(nonEmpty(entry.update_expectation), `${platform} missing update expectation`);
  }
}

function assertPrivacyPermissionsReview(fixture) {
  const globalDisclosures = byId(fixture.global_disclosures, 'id');
  for (const disclosure of ['self_hosted_server', 'account_identity', 'playback_activity', 'no_ad_tracking', 'diagnostics_redaction']) {
    assert(globalDisclosures.has(disclosure), `missing global disclosure ${disclosure}`);
  }
  const platforms = byId(fixture.platforms, 'platform');
  for (const platform of requiredPlatforms) {
    const entry = platforms.get(platform);
    assert(entry, `missing privacy review platform ${platform}`);
    assertNonEmptyArray(entry.permission_descriptions, `${platform} missing permission descriptions`);
    assertNonEmptyArray(entry.privacy_labels, `${platform} missing privacy labels`);
    assertNonEmptyArray(entry.review_notes, `${platform} missing review notes`);
    assert(entry.review_notes.some((note) => /catalog|self-hosted|server/i.test(note)), `${platform} review notes must explain self-hosted/no-catalog posture`);
  }
}

function assertAndroidTvGooglePlayReadiness(fixture) {
  assert.equal(fixture.application.application_id, 'com.duskcue.tv', 'Android TV package must match native application id');
  assert(fixture.application.target_sdk >= fixture.application.play_tv_target_sdk_floor, 'Android TV target SDK falls below Play TV policy floor');
  assert.equal(fixture.application.package_registration.status, 'external_pending', 'Android TV package registration remains external evidence');
  assert.equal(fixture.application.package_registration.deadline, '2026-09-30', 'Android TV package registration deadline must be recorded');
  assertNonEmptyArray(fixture.artifacts, 'Android TV release artifacts missing');
  assertNonEmptyArray(fixture.signing.secret_slots, 'Android TV signing secret slots missing');
  assert(fixture.signing.repository_rule.includes('No signing file'), 'Android TV signing rule must forbid repository secrets');
  const assets = byId(fixture.store_assets, 'id');
  for (const asset of ['play_icon', 'play_tv_banner', 'runtime_tv_banner', 'tv_screenshots']) {
    assert(assets.has(asset), `Android TV release assets missing ${asset}`);
  }
  assert.equal(assets.get('tv_screenshots').status, 'pending_real_capture', 'Android TV screenshots must not be faked');
  assert(fixture.play_app_content.data_safety.includes('external_pending'), 'Android TV Data Safety evidence must remain owner-verified');
  assert(fixture.play_app_content.content_rating.includes('external_pending'), 'Android TV content rating must remain external evidence');
  assert.equal(fixture.reviewer_access.status, 'external_pending', 'Android TV reviewer access must remain external evidence');
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
    assert(!pattern.test(content), `release fixture content matched forbidden pattern ${pattern}`);
  }
}
