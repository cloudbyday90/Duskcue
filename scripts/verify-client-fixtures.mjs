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
const fixtureDir = path.join(root, 'docs', 'api', 'fixtures', 'client', 'v1');
const manifest = readJson(path.join(fixtureDir, 'manifest.json'));
const contract = readJson(path.join(root, 'docs', 'api', 'client-contracts.v1.json'));

const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const dateTimePattern = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/;
const platformContentIdPattern = /^duskcue:(movie|episode):[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const forbiddenValuePatterns = [
  /[A-Za-z]:\\/,
  /\\\\[^\\]+\\/,
  /\/mnt\//,
  /\/home\//,
  /\/var\//,
  /bearer\s+[A-Za-z0-9._-]+/i,
  /token=/i,
  /signature=/i,
  /X-Amz-Signature=/i
];

const enumAllowLists = {
  media_type: ['movie', 'episode', 'show'],
  playback_type: ['direct_play', 'direct_stream', 'transcode'],
  playback_action: ['start_playback', 'show_detail', 'unavailable'],
  quality_mode: ['auto', 'maximum', 'manual'],
  network_type: ['wifi', 'cellular', 'ethernet', 'unknown'],
  package_format: ['hls_fmp4', 'mp4'],
  priority: ['low', 'normal', 'high', 'critical'],
  provider: ['fcm', 'apns', 'unifiedpush'],
  text_direction: ['ltr', 'rtl'],
  client_platform: ['android', 'ios', 'desktop', 'web', 'android_tv', 'fire_tv', 'tvos', 'roku', 'tizen', 'webos', 'xbox'],
  availability: ['playable', 'missing_file', 'metadata_incomplete', 'access_revoked'],
  section_type: ['continue', 'next_up', 'new_episodes', 'recommended'],
  segment_type: ['intro', 'credits', 'recap'],
  watch_state: ['completed', 'stopped', 'playing'],
  status: ['ready', 'pending', 'queued', 'preparing', 'failed', 'revoked', 'expired', 'deleted']
};

const requiredDenialCases = [
  'revoked-session',
  'missing-library-access',
  'unavailable-media-file',
  'expired-signed-url',
  'transcode-unavailable',
  'quota-policy-denial',
  'stale-client-state',
  'tv-access-denied'
];

const requiredDomains = contract.phase16d?.required_domains ?? [];
assert.deepEqual(manifest.required_domains, requiredDomains, 'client fixture manifest must track Phase 16d required domains');

const fixtureEntries = manifest.fixtures ?? [];
const fixturesById = new Map(fixtureEntries.map((entry) => [entry.id, entry]));

for (const id of manifest.required_fixture_ids ?? []) {
  assert(fixturesById.has(id), `missing required fixture manifest entry ${id}`);
}

const coveredDomains = new Set();
const seenIds = new Set();

for (const entry of fixtureEntries) {
  assert(!seenIds.has(entry.id), `duplicate fixture id ${entry.id}`);
  seenIds.add(entry.id);

  assert(entry.file.endsWith('.json'), `${entry.id} file must be JSON`);
  assert(Array.isArray(entry.domains) && entry.domains.length > 0, `${entry.id} must list covered domains`);
  assert(Array.isArray(entry.covers) && entry.covers.length > 0, `${entry.id} must list coverage labels`);

  for (const domain of entry.domains) coveredDomains.add(domain);

  const fixture = readJson(path.join(fixtureDir, entry.file));
  assert.equal(fixture.fixture, entry.id, `${entry.file} fixture id mismatch`);
  assertNoPrivateValues(fixture, entry.id);
  assertDateTimes(fixture, entry.id);
  assertEnums(fixture, entry.id);
  assertUuidFields(fixture, entry.id);
  assertLocalizedStrings(fixture, entry.id);
  assertRequests(fixture, entry.id);

  if (entry.order_by) {
    assertFixtureOrdering(entry.order_by, fixture, entry.id);
  }

  if (entry.id === 'client-denial-cases') {
    assertDenialCases(fixture);
  }
}

for (const domain of requiredDomains) {
  assert(coveredDomains.has(domain), `client fixtures do not cover required domain ${domain}`);
}

console.log(`Verified ${fixtureEntries.length} client contract fixtures across ${coveredDomains.size} domains.`);

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function assertNoPrivateValues(value, context) {
  if (value === null || value === undefined) return;
  if (typeof value === 'string') {
    for (const pattern of forbiddenValuePatterns) {
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
        assert(
          /fixture|preview|redacted|Bearer/.test(nested),
          `${context}.${key} token-like field must use a fixture or redacted placeholder`
        );
      }
      assertNoPrivateValues(nested, `${context}.${key}`);
    }
  }
}

function assertDateTimes(value, context) {
  visit(value, (key, nested, nestedContext) => {
    if (nested === null) return;
    if (/(^|_)(at|expires_at|generated_at|recorded_at|last_seen_at|created_at|updated_at)$/.test(key)) {
      assert.equal(typeof nested, 'string', `${nestedContext} must be a string date-time`);
      assert.match(nested, dateTimePattern, `${nestedContext} must be RFC3339 UTC`);
    }
  }, context);
}

function assertEnums(value, context) {
  visit(value, (key, nested, nestedContext) => {
    const allowed = enumAllowLists[key];
    if (!allowed || nested === null) return;
    if (typeof nested !== 'string') return;
    assert(allowed.includes(nested), `${nestedContext} has unsupported enum value ${nested}`);
  }, context);
}

function assertUuidFields(value, context) {
  visit(value, (key, nested, nestedContext) => {
    if (nested === null || typeof nested !== 'string') return;
    if (key === 'platform_content_id') {
      assert.match(nested, platformContentIdPattern, `${nestedContext} must be a canonical platform content id`);
      return;
    }
    if (
      key.endsWith('_id') &&
      !['device_id', 'event_id', 'trace_id', 'type_id'].includes(key) &&
      !key.endsWith('content_id')
    ) {
      assert.match(nested, uuidPattern, `${nestedContext} must be a stable UUID`);
    }
  }, context);
}

function assertLocalizedStrings(value, context) {
  visit(value, (key, nested, nestedContext) => {
    if (!['title', 'display_name', 'message', 'body'].includes(key) || nested === null) return;
    if (typeof nested !== 'string') return;
    assert(nested.trim().length > 0, `${nestedContext} must not be empty`);
    assert(!/^[a-z0-9_.-]+$/.test(nested), `${nestedContext} looks like a client localization key`);
  }, context);
}

function assertRequests(fixture, context) {
  const requests = [];
  if (fixture.request) requests.push(fixture.request);
  for (const item of fixture.sequence ?? []) {
    if (item.request) requests.push(item.request);
  }
  for (const item of fixture.cases ?? []) {
    if (item.request) requests.push(item.request);
  }
  assert(requests.length > 0, `${context} must include at least one request shape`);
  for (const request of requests) {
    assert(['GET', 'POST', 'PUT', 'PATCH', 'DELETE'].includes(request.method), `${context} has invalid method`);
    assert(request.path.startsWith('/'), `${context} request path must be absolute`);
  }
}

function assertFixtureOrdering(orderBy, fixture, context) {
  if (orderBy === 'name_asc') {
    const names = fixture.body.libraries.map((library) => library.name);
    assert.deepEqual(names, [...names].sort((a, b) => a.localeCompare(b)), `${context} libraries must be name sorted`);
    return;
  }
  if (orderBy === 'score_desc_then_title_asc') {
    const results = fixture.body.results;
    for (let index = 1; index < results.length; index += 1) {
      const previous = results[index - 1];
      const current = results[index];
      assert(
        previous.score > current.score || (previous.score === current.score && previous.title <= current.title),
        `${context} search results must sort by score desc then title asc`
      );
    }
    return;
  }
  if (orderBy === 'sort_title_asc') {
    const titles = fixture.body.collections.map((collection) => collection.sort_title);
    assert.deepEqual(titles, [...titles].sort((a, b) => a.localeCompare(b)), `${context} collections must be sorted`);
    return;
  }
  if (orderBy === 'section_order') {
    const expected = ['continue', 'next_up', 'new_episodes', 'recommended'];
    assert.deepEqual(
      fixture.body.surface.sections.map((section) => section.section_type),
      expected,
      `${context} TV section order must be stable`
    );
  }
}

function assertDenialCases(fixture) {
  const cases = new Map(fixture.cases.map((entry) => [entry.id, entry]));
  for (const id of requiredDenialCases) {
    assert(cases.has(id), `missing denial case ${id}`);
  }
  for (const item of fixture.cases) {
    assert(item.status >= 400, `${item.id} must be an error status`);
    assert(item.problem, `${item.id} must include Problem Details`);
    assert.equal(item.problem.status, item.status, `${item.id} problem status mismatch`);
    assert(item.problem.type.startsWith('/errors/'), `${item.id} problem type must be a local error URI`);
    assert.match(item.problem.title, /^[A-Z0-9_]+$/, `${item.id} problem title must be an error code`);
    assert(item.problem.trace_id.startsWith('fixture-'), `${item.id} trace_id must be a fixture placeholder`);
    assert(item.expected_client_action, `${item.id} must define expected client action`);
  }
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
