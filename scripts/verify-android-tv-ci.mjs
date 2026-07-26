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
const workflow = read('.github/workflows/client-ci-smoke.yml');
const manifest = readJson('docs/api/fixtures/client-ci/v1/manifest.json');
const jobs = readJson('docs/api/fixtures/client-ci/v1/ci-jobs.json');
const harnessPlan = readJson('docs/api/fixtures/client-ci/v1/harness-plan.json');

assert(fs.existsSync(path.join(root, 'scripts', 'android-tv-emulator-smoke.mjs')), 'missing Android TV emulator smoke script');

for (const token of [
  'android_tv_conformance:',
  'android_tv_emulator_smoke:',
  'clients/tv/android',
  'docs/branding/assets/store/android-tv/**',
  ':app:testDebugUnitTest',
  ':app:lintDebug',
  ':app:assembleDebug',
  'scripts/verify-client-contracts.mjs',
  'scripts/verify-client-fixtures.mjs',
  'scripts/verify-playback-conformance.mjs',
  'scripts/verify-auth-conformance.mjs',
  'scripts/verify-tv-deeplink-conformance.mjs',
  'scripts/verify-tv-surface-fixtures.mjs',
  'scripts/verify-accessibility-input.mjs',
  'scripts/verify-client-diagnostics.mjs',
  'scripts/client-smoke-harness.mjs --plan',
  'scripts/verify-client-ci-smoke.mjs',
  'scripts/verify-android-tv-release-readiness.mjs',
  'scripts/verify-nvidia-shield-validation.mjs',
  'scripts/android-tv-emulator-smoke.mjs',
  'target: android-tv',
  'profile: tv_1080p',
  'workflow_dispatch',
  'contents: read',
  'app-debug.apk',
  'lint-results-debug.html',
  'test-results/testDebugUnitTest'
]) {
  assert(workflow.includes(token), `Android TV CI workflow missing ${token}`);
}

for (const jobId of ['android_tv_conformance', 'android_tv_emulator_smoke']) {
  assert(manifest.required_ci_jobs.includes(jobId), `client CI manifest missing ${jobId}`);
  assert(jobs.jobs.some((job) => job.id === jobId), `client CI jobs fixture missing ${jobId}`);
}

for (const command of ['node scripts/verify-android-tv-ci.mjs', 'node scripts/verify-android-tv-release-readiness.mjs', 'node scripts/verify-nvidia-shield-validation.mjs']) {
  assert(manifest.required_verifiers.includes(command.replace('node ', '')), `client CI manifest missing ${command}`);
  assert(harnessPlan.contract_verifier_commands.includes(command), `client CI harness plan missing ${command}`);
}

console.log('Verified Android TV CI, emulator smoke, fixture, and artifact wiring.');

function read(relativePath) {
  return fs.readFileSync(path.join(root, relativePath), 'utf8');
}

function readJson(relativePath) {
  return JSON.parse(read(relativePath));
}
