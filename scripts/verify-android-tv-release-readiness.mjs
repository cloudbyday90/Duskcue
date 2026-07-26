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
const fixture = readJson('docs/api/fixtures/release/v1/android-tv-google-tv-readiness.json');
const releaseManifest = readJson('docs/api/fixtures/release/v1/manifest.json');
const platformChecklists = readJson('docs/api/fixtures/release/v1/platform-release-checklists.json');
const ciPlaceholders = readJson('docs/api/fixtures/release/v1/ci-release-placeholders.json');
const versioning = readJson('docs/api/fixtures/release/v1/versioning-policy.json');
const gradle = read('clients/tv/android/app/build.gradle.kts');
const appManifest = read('clients/tv/android/app/src/main/AndroidManifest.xml');
const readinessDoc = read('docs/ci/ANDROID_TV_RELEASE_READINESS.md');

assert(releaseManifest.fixtures.some((entry) => entry.id === fixture.fixture), 'release manifest missing Android TV readiness fixture');
assert.equal(fixture.application.application_id, 'com.duskcue.tv', 'Android TV application ID must match the native project');
assert.equal(fixture.application.target_sdk, 36, 'Android TV readiness target SDK must match the configured target');
assert(fixture.application.target_sdk >= fixture.application.play_tv_target_sdk_floor, 'Android TV target SDK falls below the Play TV floor');
assert.equal(fixture.application.minimum_sdk, 26, 'Android TV minimum SDK must preserve Watch Next compatibility');
assert.equal(fixture.application.package_registration.deadline, '2026-09-30', 'Android package registration deadline must remain explicit');

for (const token of [
  'applicationId = "com.duskcue.tv"',
  'minSdk = 26',
  'targetSdk = 36',
  'duskcueVersionCode',
  'duskcueVersionName',
  '2_100_000_000'
]) {
  assert(gradle.includes(token), `Android TV Gradle configuration missing ${token}`);
}

for (const token of [
  'android.software.leanback',
  'android.hardware.touchscreen',
  'android.intent.category.LEANBACK_LAUNCHER',
  '@mipmap/tv_banner'
]) {
  assert(appManifest.includes(token), `Android TV manifest missing ${token}`);
}
assert(!appManifest.includes('android:roundIcon'), 'Android TV manifest must use adaptive launcher icon instead of deprecated roundIcon');

const artifacts = byId(fixture.artifacts, 'id');
for (const id of ['debug_apk', 'play_release_bundle', 'release_apk']) {
  const artifact = artifacts.get(id);
  assert(artifact, `missing Android TV ${id} artifact placeholder`);
  assert(nonEmpty(artifact.path), `${id} artifact path is required`);
  assert(nonEmpty(artifact.command), `${id} artifact command is required`);
}
assert(artifacts.get('play_release_bundle').command.includes(':app:bundleRelease'), 'Play release artifact must use bundleRelease');
assert.equal(artifacts.get('play_release_bundle').play_upload_artifact, true, 'Android TV AAB must be the intended Play upload artifact');
assert.equal(artifacts.get('play_release_bundle').ready_now, false, 'Android TV AAB must remain unready until protected signing exists');

for (const field of ['play_app_signing', 'upload_key', 'repository_rule', 'compromise_action']) {
  assert(nonEmpty(fixture.signing[field]), `Android TV signing fixture missing ${field}`);
}
assert.equal(fixture.signing.secret_slots.length, 4, 'Android TV signing fixture must name all protected upload-key slots');
assert(fixture.signing.repository_rule.includes('No signing file'), 'Android TV signing rule must forbid committed key material');

const assets = byId(fixture.store_assets, 'id');
for (const [id, dimensions] of [['play_icon', [512, 512]], ['play_tv_banner', [1280, 720]], ['runtime_tv_banner', [320, 180]]]) {
  const asset = assets.get(id);
  assert(asset, `missing Android TV asset ${id}`);
  assert.equal(asset.status === 'ready_for_store_review' || asset.status === 'checked_in', true, `${id} must be ready`);
  assert.deepEqual(readPngDetails(asset.path).dimensions, dimensions, `${id} dimensions mismatch`);
  assert(nonEmpty(asset.alt_text), `${id} needs alt text`);
}
for (const id of ['play_tv_banner', 'runtime_tv_banner']) {
  assert.equal(readPngDetails(assets.get(id).path).colorType, 2, `${id} must be a non-transparent truecolor PNG`);
}
assert(Array.isArray(assets.get('runtime_tv_banner').density_variants), 'runtime TV banner density variants are required');
for (const variant of assets.get('runtime_tv_banner').density_variants) {
  const [width, height] = variant.dimensions.split('x').map(Number);
  const details = readPngDetails(variant.path);
  assert.deepEqual(details.dimensions, [width, height], `runtime TV banner density variant ${variant.path} dimensions mismatch`);
  assert.equal(details.colorType, 2, `runtime TV banner density variant ${variant.path} must be a non-transparent truecolor PNG`);
}
assert.equal(assets.get('tv_screenshots').status, 'pending_real_capture', 'TV screenshots must remain honest until captured');

for (const field of ['privacy_policy', 'data_safety', 'content_rating', 'target_audience', 'ads_declaration', 'app_access']) {
  assert(fixture.play_app_content[field].startsWith('external_pending'), `${field} must remain owner-verified external evidence`);
}
assert(fixture.play_app_content.declaration_rule.includes('Never predeclare'), 'Data Safety rule must prevent unsupported no-data claim');
assert.equal(fixture.reviewer_access.status, 'external_pending', 'reviewer access must not be faked in repository fixtures');
assert(fixture.quality_evidence.manual_release_required.length >= 4, 'Android TV readiness must preserve manual device gates');

const releaseChecklist = byId(platformChecklists.platforms, 'platform').get('android_tv_google_tv');
assert.equal(releaseChecklist.bundle_or_package_name, fixture.application.application_id, 'shared release checklist must use the native Android TV package');
const ciPlaceholder = byId(ciPlaceholders.jobs, 'platform').get('android_tv_google_tv');
assert(ciPlaceholder.build_command.includes(':app:bundleRelease'), 'shared CI placeholder must use the Android TV bundle command');
const tvVersioning = byId(versioning.targets, 'target').get('android_tv_google_tv');
assert(tvVersioning.rule.includes('versionCode'), 'Android TV versioning rule must require versionCode');

for (const token of [
  'Android TV release track',
  '2026-09-30',
  'Data Safety',
  'content rating',
  'App Access',
  'DUSKCUE_ANDROID_TV_UPLOAD_KEYSTORE',
  'play-banner-1280x720.png',
  'mipmap-xhdpi/tv_banner.png',
  'pending_real_capture',
  'SBOM',
  'provenance',
  'rollback'
]) {
  assert(readinessDoc.includes(token), `Android TV release readiness document missing ${token}`);
}

assertNoSecrets(fixture);
console.log('Verified Android TV / Google TV release-readiness placeholders and assets.');

function read(relativePath) {
  return fs.readFileSync(path.join(root, relativePath), 'utf8');
}

function readJson(relativePath) {
  return JSON.parse(read(relativePath));
}

function readPngDetails(relativePath) {
  const buffer = fs.readFileSync(path.join(root, relativePath));
  assert.equal(buffer.subarray(1, 4).toString('ascii'), 'PNG', `${relativePath} must be a PNG`);
  return {
    dimensions: [buffer.readUInt32BE(16), buffer.readUInt32BE(20)],
    colorType: buffer[25]
  };
}

function byId(items, key) {
  assert(Array.isArray(items), `expected ${key} array`);
  return new Map(items.map((item) => [item[key], item]));
}

function nonEmpty(value) {
  return typeof value === 'string' ? value.trim().length > 0 : Boolean(value);
}

function assertNoSecrets(value) {
  const serialized = JSON.stringify(value);
  for (const pattern of [/Bearer\s+[A-Za-z0-9._-]+/, /password=[^\s]+/i, /token=[A-Za-z0-9._-]+/i, /-----BEGIN [A-Z ]+-----/]) {
    assert(!pattern.test(serialized), `Android TV release fixture matched forbidden secret pattern ${pattern}`);
  }
}
