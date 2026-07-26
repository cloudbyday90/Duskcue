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
const fixture = readJson('docs/api/fixtures/device-lab/v1/sony-bravia-validation.json');
const deviceLabManifest = readJson('docs/api/fixtures/device-lab/v1/manifest.json');
const deviceMatrix = readJson('docs/api/fixtures/device-lab/v1/device-matrix.json');
const releasePolicy = readJson('docs/api/fixtures/device-lab/v1/release-validation-policy.json');
const androidDiagnostics = read('clients/tv/android/app/src/main/java/com/duskcue/tv/diagnostics/TvDiagnostics.kt');
const capabilityCollector = read('clients/tv/android/app/src/main/java/com/duskcue/tv/diagnostics/TvDeviceCapabilityCollector.kt');
const capabilityTest = read('clients/tv/android/app/src/test/java/com/duskcue/tv/diagnostics/TvDeviceCapabilityCollectorTest.kt');
const androidTvDoc = read('docs/design/ANDROID_TV.md');
const runbook = read('docs/ci/SONY_BRAVIA_VALIDATION.md');
const harness = read('scripts/sony-bravia-validation.mjs');
const workflow = read('.github/workflows/client-ci-smoke.yml');
const clientCiManifest = readJson('docs/api/fixtures/client-ci/v1/manifest.json');
const clientCiJobs = readJson('docs/api/fixtures/client-ci/v1/ci-jobs.json');
const clientCiHarness = readJson('docs/api/fixtures/client-ci/v1/harness-plan.json');

assert.equal(fixture.fixture, 'sony-bravia-validation', 'BRAVIA fixture id mismatch');
assert.equal(fixture.status, 'repository_ready_physical_evidence_pending', 'BRAVIA fixture must not claim unobserved hardware evidence');
assert(deviceLabManifest.fixtures.some((entry) => entry.id === fixture.fixture), 'device-lab manifest missing BRAVIA fixture');
assert.equal(fixture.platform, 'android_tv_google_tv', 'BRAVIA fixture platform mismatch');
assert.deepEqual(fixture.device_targets.map((target) => target.id), ['bravia_google_tv', 'bravia_android_tv'], 'BRAVIA targets must include Google TV and Android TV');

const androidTvTargets = byId(deviceMatrix.platforms, 'id').get('android_tv_google_tv');
assert(androidTvTargets.representative_targets.some((target) => /Sony BRAVIA Google TV/.test(target.name)), 'Android TV matrix missing BRAVIA Google TV target');
assert(androidTvTargets.representative_targets.some((target) => /Sony BRAVIA Android TV/.test(target.name)), 'Android TV matrix missing BRAVIA Android TV target');
const androidTvReleasePolicy = byId(releasePolicy.platforms, 'platform').get('android_tv_google_tv');
assert(androidTvReleasePolicy.release_required.some((target) => /Sony BRAVIA Google TV/.test(target)), 'Android TV release policy missing BRAVIA Google TV gate');
assert(androidTvReleasePolicy.release_required.some((target) => /Sony BRAVIA Android TV/.test(target)), 'Android TV release policy missing BRAVIA Android TV gate');

const caseIds = fixture.test_cases.map((testCase) => testCase.id);
for (const id of [
  'google_play_visibility', 'launcher_and_hls_playback', 'direct_play_and_direct_stream_fallback', 'hdr_dolby_vision_behavior',
  'audio_passthrough_or_downmix', 'subtitle_caption_fallback', 'remote_focus_and_back', 'standby_resume', 'voice_and_deep_link_entry',
  'watch_next', 'diagnostics_capture'
]) {
  assert(caseIds.includes(id), `BRAVIA validation missing ${id}`);
}
assert(fixture.test_cases.every((testCase) => testCase.observation && testCase.evidence), 'every BRAVIA test needs observation and evidence rules');
assert.deepEqual(fixture.evidence_contract.result_values, ['passed', 'failed', 'not_supported', 'not_tested'], 'BRAVIA evidence result vocabulary drift');
for (const field of ['test_case_id', 'device_target', 'bravia_model', 'experience_generation', 'firmware_version', 'display_and_audio_chain', 'redacted_diagnostics_reference']) {
  assert(fixture.evidence_contract.required_fields.includes(field), `BRAVIA evidence contract missing ${field}`);
}

for (const token of ['TvDeviceCapabilityReport', 'network_connection_class', 'audio_output_encodings', 'display_hdr_types']) {
  assert(androidDiagnostics.includes(token), `Android diagnostics missing BRAVIA capability field ${token}`);
}
for (const token of ['MediaCodecList', 'AudioManager.GET_DEVICES_OUTPUTS', 'sony_bravia']) {
  assert(capabilityCollector.includes(token), `Android capability collector missing BRAVIA capability field ${token}`);
}
assert(capabilityTest.includes('sony_bravia'), 'Android capability tests must cover Sony BRAVIA classification');
for (const token of ['SONY_BRAVIA_VALIDATION.md', 'Task 14', 'physical evidence']) {
  assert(androidTvDoc.includes(token), `Android TV design document missing ${token}`);
}
for (const token of ['Sony BRAVIA Google TV', 'Sony BRAVIA Android TV', 'eARC', 'Dolby Vision', 'Watch Next', 'not_tested']) {
  assert(runbook.includes(token), `BRAVIA runbook missing ${token}`);
}
for (const token of ['--plan', '--serial', '--experience', 'android.software.leanback', 'com.android.vending', 'sony_bravia', 'manual_required_test_cases']) {
  assert(harness.includes(token), `BRAVIA capture script missing ${token}`);
}
assert(harness.includes('new RegExp(`${key}=([^\\\\s]+)`)'), 'BRAVIA capture script must parse package values through a whitespace-safe pattern');
assert(workflow.includes('scripts/verify-sony-bravia-validation.mjs'), 'Android TV workflow missing BRAVIA static verifier');
assert(clientCiManifest.required_verifiers.includes('scripts/verify-sony-bravia-validation.mjs'), 'client CI manifest missing BRAVIA verifier');
assert(clientCiJobs.jobs.find((job) => job.id === 'android_tv_conformance').commands.includes('node scripts/verify-sony-bravia-validation.mjs'), 'Android TV CI fixture missing BRAVIA verifier');
assert(clientCiHarness.contract_verifier_commands.includes('node scripts/verify-sony-bravia-validation.mjs'), 'client CI harness missing BRAVIA verifier');
assertNoSecrets(fixture);
console.log('Verified Sony BRAVIA validation fixture, capability capture, and physical-evidence boundaries.');

function read(relativePath) {
  return fs.readFileSync(path.join(root, relativePath), 'utf8');
}

function readJson(relativePath) {
  return JSON.parse(read(relativePath));
}

function byId(items, key) {
  return new Map(items.map((item) => [item[key], item]));
}

function assertNoSecrets(value) {
  const serialized = JSON.stringify(value);
  for (const pattern of [/Bearer\s+[A-Za-z0-9._-]+/, /password=[^\s]+/i, /token=[A-Za-z0-9._-]+/i, /-----BEGIN [A-Z ]+-----/]) {
    assert(!pattern.test(serialized), `BRAVIA fixture matched forbidden secret pattern ${pattern}`);
  }
}
