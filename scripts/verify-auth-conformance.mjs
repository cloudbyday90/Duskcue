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
const fixtureDir = path.join(root, 'docs', 'api', 'fixtures', 'auth', 'v1');
const manifest = readJson(path.join(fixtureDir, 'manifest.json'));

const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const dateTimePattern = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/;
const forbiddenPatterns = [
  /bearer\s+(?!token\b)[A-Za-z0-9._-]+/i,
  /mv_[A-Za-z0-9._-]+/,
  /token=/i,
  /signature=/i,
  /[A-Za-z]:\\/,
  /\/mnt\//,
  /\/home\//,
  /\/var\//
];

const fixtures = new Map();
for (const entry of manifest.fixtures ?? []) {
  const fixture = readJson(path.join(fixtureDir, entry.file));
  assert.equal(fixture.fixture, entry.id, `${entry.file} fixture id mismatch`);
  assertNoPrivateValues(fixture, entry.id);
  assertDateTimes(fixture, entry.id);
  assertUuidFields(fixture, entry.id);
  fixtures.set(entry.id, fixture);
}

for (const id of manifest.required_fixture_ids) {
  assert(fixtures.has(id), `missing auth fixture ${id}`);
}

assertAuthFlows(fixtures.get('auth-flow-matrix'));
assertSessionLifecycle(fixtures.get('session-lifecycle'));
assertSecureStorage(fixtures.get('secure-storage-policy'));
assertSwitching(fixtures.get('server-user-switching'));
assertNegativeCases(fixtures.get('auth-negative-cases'));

console.log(`Verified ${fixtures.size} auth/session conformance fixtures.`);

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function assertAuthFlows(fixture) {
  const flows = new Map(fixture.flows.map((flow) => [flow.id, flow]));
  for (const flow of ['device_linking', 'passkey_login', 'fallback_login', 'reauth']) {
    assert(flows.has(flow), `missing auth flow ${flow}`);
  }
  for (const flow of fixture.flows) {
    assert(flow.steps.length > 0, `${flow.id} needs steps`);
    assert(flow.client_requirements.length > 0, `${flow.id} needs client requirements`);
    for (const step of flow.steps) {
      assertRequest(step.request, `${flow.id}.request`);
    }
  }
}

function assertSessionLifecycle(fixture) {
  const flows = new Map(fixture.flows.map((flow) => [flow.id, flow]));
  for (const flow of ['logout', 'logout_all', 'session_delete', 'expired_session', 'session_kicked']) {
    assert(flows.has(flow), `missing session lifecycle flow ${flow}`);
  }
  assert.equal(flows.get('logout').expect_client_state.clear_bearer_token, true);
  assert.equal(flows.get('logout_all').expect_client_state.invalidate_other_device_sessions, true);
  assert.equal(flows.get('expired_session').expect_client_state.keep_selected_server, true);
  assert.equal(flows.get('session_kicked').event.event, 'session_kicked');
}

function assertSecureStorage(fixture) {
  const items = new Map(fixture.items.map((item) => [item.id, item]));
  for (const itemId of manifest.required_storage_items) {
    assert(items.has(itemId), `missing storage item ${itemId}`);
  }
  for (const item of fixture.items) {
    assert(['secret', 'private', 'protected_metadata', 'non_secret'].includes(item.classification));
    assert(Array.isArray(item.allowed_storage), `${item.id} needs allowed storage`);
    assert(Array.isArray(item.forbidden_storage), `${item.id} needs forbidden storage`);
    if (item.classification === 'secret') {
      assert(item.forbidden_storage.includes('logs'), `${item.id} secret must be forbidden in logs`);
      assert(item.forbidden_storage.some((value) => value.includes('diagnostics')), `${item.id} secret must be forbidden in diagnostics`);
    }
  }
}

function assertSwitching(fixture) {
  const cases = new Map(fixture.cases.map((item) => [item.id, item]));
  for (const caseId of manifest.required_switching_cases) {
    assert(cases.has(caseId), `missing switching case ${caseId}`);
  }
  assert.equal(cases.get('switch_server').expect_client_state.do_not_send_previous_server_token, true);
  assert.equal(cases.get('switch_user').expect_client_state.clear_download_inventory_scope, true);
  assert.equal(cases.get('local_network_tls_failure').expect_client_state.do_not_downgrade_to_http_when_exposed, true);
}

function assertNegativeCases(fixture) {
  const cases = new Map(fixture.cases.map((item) => [item.id, item]));
  for (const caseId of manifest.required_negative_cases) {
    assert(cases.has(caseId), `missing negative case ${caseId}`);
  }
  for (const item of fixture.cases) {
    assertRequest(item.request, `${item.id}.request`);
    assertProblem(item.problem, item.id);
    assert(item.expected_client_action, `${item.id} needs expected client action`);
  }
}

function assertRequest(request, context) {
  assert(request, `${context} missing request`);
  assert(['GET', 'POST', 'PUT', 'PATCH', 'DELETE'].includes(request.method), `${context} invalid method`);
  assert(request.path.startsWith('/api/v1/'), `${context} path must be API-relative`);
}

function assertProblem(problem, context) {
  assert(problem, `${context} missing problem`);
  assert(problem.type.startsWith('/errors/'), `${context} problem type must be local`);
  assert.match(problem.title, /^[A-Z0-9_]+$/, `${context} problem title must be an error code`);
  assert(problem.status >= 400, `${context} problem status must be an error`);
  assert(problem.trace_id.startsWith('fixture-'), `${context} trace_id must be fixture placeholder`);
}

function assertNoPrivateValues(value, context) {
  if (value === null || value === undefined) return;
  if (typeof value === 'string') {
    for (const pattern of forbiddenPatterns) {
      assert(!pattern.test(value), `${context} leaks private value: ${value}`);
    }
    return;
  }
  if (Array.isArray(value)) {
    value.forEach((entry, index) => assertNoPrivateValues(entry, `${context}[${index}]`));
    return;
  }
  if (typeof value === 'object') {
    for (const [key, nested] of Object.entries(value)) {
      if (key.toLowerCase().includes('token') && typeof nested === 'string') {
        assert(/fixture|placeholder|preview|redacted/.test(nested), `${context}.${key} token field must be placeholder`);
      }
      assertNoPrivateValues(nested, `${context}.${key}`);
    }
  }
}

function assertDateTimes(value, context) {
  visit(value, (key, nested, nestedContext) => {
    if (nested === null) return;
    if (['expires_at', 'occurred_at'].includes(key)) {
      assert.equal(typeof nested, 'string', `${nestedContext} must be a string date-time`);
      assert.match(nested, dateTimePattern, `${nestedContext} must be RFC3339 UTC`);
    }
  }, context);
}

function assertUuidFields(value, context) {
  visit(value, (key, nested, nestedContext) => {
    if (nested === null || typeof nested !== 'string') return;
    if (key.endsWith('_id') && !['challenge_id', 'device_id', 'rp_id', 'trace_id'].includes(key)) {
      assert.match(nested, uuidPattern, `${nestedContext} must be a stable UUID`);
    }
  }, context);
}

function visit(value, visitor, context, key = '') {
  visitor(key, value, context);
  if (Array.isArray(value)) {
    value.forEach((entry, index) => visit(entry, visitor, `${context}[${index}]`, key));
    return;
  }
  if (value && typeof value === 'object') {
    for (const [nestedKey, nestedValue] of Object.entries(value)) {
      visit(nestedValue, visitor, `${context}.${nestedKey}`, nestedKey);
    }
  }
}
