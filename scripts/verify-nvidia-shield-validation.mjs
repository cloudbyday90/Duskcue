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
const fixture = readJson('docs/api/fixtures/device-lab/v1/nvidia-shield-validation.json');
const deviceLabManifest = readJson('docs/api/fixtures/device-lab/v1/manifest.json');
const deviceMatrix = readJson('docs/api/fixtures/device-lab/v1/device-matrix.json');
const releasePolicy = readJson('docs/api/fixtures/device-lab/v1/release-validation-policy.json');
const androidDiagnostics = read('clients/tv/android/app/src/main/java/com/duskcue/tv/diagnostics/TvDiagnostics.kt');
const capabilityCollector = read('clients/tv/android/app/src/main/java/com/duskcue/tv/diagnostics/TvDeviceCapabilityCollector.kt');
const androidTvDoc = read('docs/design/ANDROID_TV.md');
const runbook = read('docs/ci/NVIDIA_SHIELD_VALIDATION.md');
const harness = read('scripts/nvidia-shield-validation.mjs');
const workflow = read('.github/workflows/client-ci-smoke.yml');
const clientCiManifest = readJson('docs/api/fixtures/client-ci/v1/manifest.json');
const clientCiJobs = readJson('docs/api/fixtures/client-ci/v1/ci-jobs.json');
const clientCiHarness = readJson('docs/api/fixtures/client-ci/v1/harness-plan.json');

assert.equal(fixture.fixture, 'nvidia-shield-validation', 'SHIELD fixture id mismatch');
assert.equal(fixture.status, 'repository_ready_physical_evidence_pending', 'SHIELD fixture must not claim unobserved hardware evidence');
assert(deviceLabManifest.fixtures.some((entry) => entry.id === fixture.fixture), 'device-lab manifest missing SHIELD fixture');
assert.equal(fixture.platform, 'android_tv_google_tv', 'SHIELD fixture platform mismatch');
assert.deepEqual(fixture.device_targets.map((target) => target.id), ['shield_tv', 'shield_tv_pro'], 'SHIELD targets must include TV and Pro');

const androidTvTargets = byId(deviceMatrix.platforms, 'id').get('android_tv_google_tv');
assert(androidTvTargets.representative_targets.some((target) => /NVIDIA SHIELD TV/.test(target.name)), 'Android TV matrix missing SHIELD target');
assert(androidTvTargets.representative_targets.some((target) => /NVIDIA SHIELD TV Pro/.test(target.name)), 'Android TV matrix missing SHIELD Pro target');
const androidTvReleasePolicy = byId(releasePolicy.platforms, 'platform').get('android_tv_google_tv');
assert(androidTvReleasePolicy.release_required.some((target) => /NVIDIA SHIELD/.test(target)), 'Android TV release policy missing SHIELD gate');

const caseIds = fixture.test_cases.map((testCase) => testCase.id);
for (const id of [
  'google_play_visibility', 'ethernet_playback', 'wifi_playback', 'hdr_display_modes', 'audio_route_and_passthrough',
  'subtitle_caption_fallback', 'ai_upscaling_interaction', 'remote_and_gamepad', 'standby_resume', 'watch_next', 'diagnostics_capture'
]) {
  assert(caseIds.includes(id), `SHIELD validation missing ${id}`);
}
assert(fixture.test_cases.every((testCase) => testCase.observation && testCase.evidence), 'every SHIELD test needs observation and evidence rules');
assert.deepEqual(fixture.evidence_contract.result_values, ['passed', 'failed', 'not_supported', 'not_tested'], 'SHIELD evidence result vocabulary drift');
for (const field of ['test_case_id', 'device_target', 'firmware_version', 'network_transport', 'display_and_audio_chain', 'redacted_diagnostics_reference']) {
  assert(fixture.evidence_contract.required_fields.includes(field), `SHIELD evidence contract missing ${field}`);
}

for (const token of ['TvDeviceCapabilityReport', 'network_connection_class', 'audio_output_encodings', 'display_hdr_types']) {
  assert(androidDiagnostics.includes(token), `Android diagnostics missing SHIELD capability field ${token}`);
}
for (const token of ['MediaCodecList', 'AudioManager.GET_DEVICES_OUTPUTS', 'TRANSPORT_ETHERNET', 'TRANSPORT_WIFI', 'nvidia_shield']) {
  assert(capabilityCollector.includes(token), `Android capability collector missing ${token}`);
}
for (const token of ['NVIDIA_SHIELD_VALIDATION.md', 'Task 13', 'physical evidence']) {
  assert(androidTvDoc.includes(token), `Android TV design document missing ${token}`);
}
for (const token of ['NVIDIA SHIELD TV Pro', 'AI-Enhanced', 'TrueHD', 'DTS', 'Ethernet', 'Wi-Fi', 'not_tested']) {
  assert(runbook.includes(token), `SHIELD runbook missing ${token}`);
}
for (const token of ['--plan', '--serial', '--network', 'android.software.leanback', 'nvidia_shield', 'manual_required_test_cases']) {
  assert(harness.includes(token), `SHIELD capture script missing ${token}`);
}
assert(harness.includes('new RegExp(`${key}=([^\\\\s]+)`)'), 'SHIELD capture script must parse package values through a whitespace-safe pattern');
assert(workflow.includes('scripts/verify-nvidia-shield-validation.mjs'), 'Android TV workflow missing SHIELD static verifier');
assert(clientCiManifest.required_verifiers.includes('scripts/verify-nvidia-shield-validation.mjs'), 'client CI manifest missing SHIELD verifier');
assert(clientCiJobs.jobs.find((job) => job.id === 'android_tv_conformance').commands.includes('node scripts/verify-nvidia-shield-validation.mjs'), 'Android TV CI fixture missing SHIELD verifier');
assert(clientCiHarness.contract_verifier_commands.includes('node scripts/verify-nvidia-shield-validation.mjs'), 'client CI harness missing SHIELD verifier');
assertNoSecrets(fixture);
console.log('Verified NVIDIA SHIELD validation fixture, capability capture, and physical-evidence boundaries.');

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
    assert(!pattern.test(serialized), `SHIELD fixture matched forbidden secret pattern ${pattern}`);
  }
}
