/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 *
 * This program is free software: licensed under AGPL-3.0
 * See LICENSE file for details.
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const manifestPath = path.join(root, 'docs', 'api', 'client-contracts.v1.json');
const bindingPath = path.join(root, 'docs', 'api', 'client-binding-targets.v1.json');

const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
const bindingTargets = JSON.parse(fs.readFileSync(bindingPath, 'utf8'));
const failures = [];

const requiredTargets = [
  'typescript_tauri',
  'dart_flutter',
  'kotlin_android_firetv',
  'swift_tvos_ios',
  'roku_brightscript',
  'samsung_tizen_webos',
  'windows_xbox',
];

const requiredAdapters = [
  'base_url_resolver',
  'bearer_token_provider',
  'reauth_handler',
  'timeout_retry_policy',
  'problem_details_mapper',
  'pagination_helper',
  'cache_etag_store',
  'sse_event_decoder',
  'secure_storage_adapter',
  'diagnostics_redactor',
];

function fail(message) {
  failures.push(message);
}

if (bindingTargets.contract_manifest !== 'docs/api/client-contracts.v1.json') {
  fail('Binding target matrix must reference docs/api/client-contracts.v1.json.');
}

const manifestDomains = new Set(manifest.phase16d?.required_domains ?? []);
if (manifestDomains.size === 0) {
  fail('Client contract manifest has no Phase 16d required domains.');
}

const adapterContracts = new Map((bindingTargets.adapter_contracts ?? []).map((adapter) => [adapter.id, adapter]));
for (const adapterId of requiredAdapters) {
  if (!(bindingTargets.required_shared_adapters ?? []).includes(adapterId)) {
    fail(`Missing required shared adapter id: ${adapterId}`);
  }
  const contract = adapterContracts.get(adapterId);
  if (!contract) {
    fail(`Missing adapter contract for ${adapterId}`);
  } else {
    if (!contract.purpose) fail(`Adapter ${adapterId} must define a purpose.`);
    if (!Array.isArray(contract.must_not) || contract.must_not.length === 0) {
      fail(`Adapter ${adapterId} must define at least one must_not guardrail.`);
    }
  }
}

const targets = new Map((bindingTargets.targets ?? []).map((target) => [target.id, target]));
for (const targetId of requiredTargets) {
  const target = targets.get(targetId);
  if (!target) {
    fail(`Missing binding target: ${targetId}`);
    continue;
  }

  for (const field of ['display_name', 'language', 'current_strategy', 'future_generation', 'contract_mode']) {
    if (!target[field]) {
      fail(`Binding target ${targetId} is missing ${field}.`);
    }
  }

  if (!Array.isArray(target.platforms) || target.platforms.length === 0) {
    fail(`Binding target ${targetId} must list at least one platform.`);
  }

  for (const domain of manifestDomains) {
    if (!(target.required_domains ?? []).includes(domain)) {
      fail(`Binding target ${targetId} is missing required domain ${domain}.`);
    }
  }

  for (const adapterId of requiredAdapters) {
    if (!(target.shared_adapters ?? []).includes(adapterId)) {
      fail(`Binding target ${targetId} is missing shared adapter ${adapterId}.`);
    }
  }

  if (!Array.isArray(target.fixture_requirements) || target.fixture_requirements.length < 3) {
    fail(`Binding target ${targetId} must define at least three fixture requirements.`);
  }
}

if (failures.length > 0) {
  for (const failure of failures) {
    console.error(failure);
  }
  process.exit(1);
}

console.log(`Verified ${targets.size} client binding targets and ${requiredAdapters.length} shared adapters.`);
