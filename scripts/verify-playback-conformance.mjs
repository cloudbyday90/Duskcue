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
const fixtureDir = path.join(root, 'docs', 'api', 'fixtures', 'playback', 'v1');
const manifest = readJson(path.join(fixtureDir, 'manifest.json'));

const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const dateTimePattern = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/;
const forbiddenValuePatterns = [
  /bearer\s+[A-Za-z0-9._-]+/i,
  /token=/i,
  /signature=/i,
  /X-Amz-Signature=/i,
  /[A-Za-z]:\\/,
  /\/mnt\//,
  /\/home\//,
  /\/var\//
];

const fixtureEntries = manifest.fixtures ?? [];
const fixtureById = new Map(fixtureEntries.map((entry) => [entry.id, entry]));
for (const id of manifest.required_fixture_ids ?? []) {
  assert(fixtureById.has(id), `missing playback fixture entry ${id}`);
}

const fixtures = new Map();
for (const entry of fixtureEntries) {
  const fixture = readJson(path.join(fixtureDir, entry.file));
  assert.equal(fixture.fixture, entry.id, `${entry.file} fixture id mismatch`);
  assertNoPrivateValues(fixture, entry.id);
  assertDateTimes(fixture, entry.id);
  assertUuidFields(fixture, entry.id);
  fixtures.set(entry.id, fixture);
}

assertStateMachine(fixtures.get('playback-state-machine'));
assertTrackSelection(fixtures.get('playback-track-selection'));
assertStreamPaths(fixtures.get('playback-stream-paths'));
assertMediaSession(fixtures.get('playback-media-session-remote'));
assertQoe(fixtures.get('playback-qoe-metrics'));
assertCrossDeviceResume(fixtures.get('playback-cross-device-resume'));
assertErrorReporting(fixtures.get('playback-error-reporting'));

console.log(`Verified ${fixtureEntries.length} playback conformance fixtures.`);

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function assertStateMachine(fixture) {
  const events = fixture.events.map((event) => event.event);
  for (const required of manifest.required_state_events) {
    assert(events.includes(required), `state machine missing ${required}`);
  }
  assertInOrder(events, [
    'start',
    'resume_seek',
    'first_frame',
    'heartbeat_playing',
    'pause',
    'resume',
    'seek',
    'heartbeat_after_seek',
    'stop'
  ]);
  for (const event of fixture.events) {
    if (event.request) assertPlaybackRequest(event.request, `state-machine.${event.event}`);
    if (typeof event.position_ms === 'number') {
      assert(event.position_ms >= 0 && event.position_ms <= fixture.duration_ms, `${event.event} position out of range`);
    }
  }
}

function assertTrackSelection(fixture) {
  const cases = new Map(fixture.cases.map((entry) => [entry.id, entry]));
  for (const id of manifest.required_track_cases) {
    assert(cases.has(id), `missing track selection case ${id}`);
  }
  assert.equal(cases.get('supported_audio_and_subtitle').expect.restart_required, true);
  assert.equal(cases.get('unsupported_audio_downmix').expect.playback_type, 'transcode_hls');
  assert.equal(cases.get('unsupported_image_subtitle_burn_in').expect.subtitle_decision, 'burn_in');
  assert.equal(cases.get('unavailable_track_rejected').expect_problem.title, 'VALID_001');
}

function assertStreamPaths(fixture) {
  const decisions = new Set(fixture.cases.map((entry) => entry.decision));
  for (const decision of manifest.required_stream_decisions) {
    assert(decisions.has(decision), `missing stream decision ${decision}`);
  }
  for (const item of fixture.cases) {
    assert.equal(item.requires_bearer_header, true, `${item.id} must require bearer header`);
    const mediaUrl = item.stream_url ?? item.manifest_url;
    assert(mediaUrl.startsWith('/api/v1/'), `${item.id} media URL must be API-relative`);
    if (item.decision === 'transcode_hls') {
      assert.equal(item.expect.transcode_session, true);
      assert.equal(item.expect.webvtt_timestamp_map_required, true);
    }
  }
}

function assertMediaSession(fixture) {
  const actions = new Set(fixture.actions.map((entry) => entry.action));
  for (const action of manifest.required_remote_actions) {
    assert(actions.has(action), `missing media session action ${action}`);
  }
  for (const item of fixture.actions) {
    assert(item.duskcue_request.startsWith('POST /api/v1/playback/'), `${item.action} must map to playback API`);
  }
}

function assertQoe(fixture) {
  for (const sample of fixture.samples) {
    for (const field of manifest.required_qoe_fields) {
      if (sample.id === 'startup-and-buffering' && field === 'playback_failure_code') continue;
      assert(Object.hasOwn(sample.body, field), `${sample.id} missing QoE field ${field}`);
    }
    assertPlaybackRequest(fixture.request, `qoe.${sample.id}`);
  }
}

function assertCrossDeviceResume(fixture) {
  const steps = new Map(fixture.scenario.map((entry) => [entry.step, entry]));
  assert.equal(steps.get('tv_resolve_refreshes_resume').expect.resume_position_ms, 3000000);
  assert.equal(steps.get('tv_resolve_refreshes_resume').expect.ignore_launcher_cached_resume_ms, 1800000);
  assert.equal(steps.get('mobile_start_uses_latest_resume').expect.start_position_ms, 3000000);
  assert(steps.get('tv_surface_changed_event').event.data.debounce_until, 'resume event must include debounce guidance');
}

function assertErrorReporting(fixture) {
  const cases = new Map(fixture.cases.map((entry) => [entry.id, entry]));
  for (const id of ['transcode-unavailable', 'expired-media-url', 'unsupported-track']) {
    assert(cases.has(id), `missing playback error case ${id}`);
  }
  for (const item of fixture.cases) {
    assert(item.status >= 400);
    assert.equal(item.problem.status, item.status);
    assert.match(item.problem.title, /^[A-Z0-9_]+$/);
    assert(item.expected_client_action);
  }
  assert(cases.get('transcode-unavailable').qoe.playback_failure_code);
}

function assertPlaybackRequest(request, context) {
  assert(['GET', 'POST', 'PUT', 'PATCH', 'DELETE'].includes(request.method), `${context} invalid method`);
  assert(request.path.startsWith('/api/v1/'), `${context} path must be API-relative`);
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
      assertNoPrivateValues(nested, `${context}.${key}`);
    }
  }
}

function assertDateTimes(value, context) {
  visit(value, (key, nested, nestedContext) => {
    if (nested === null) return;
    if (['at', 'recorded_at', 'debounce_until'].includes(key)) {
      assert.equal(typeof nested, 'string', `${nestedContext} must be a string date-time`);
      assert.match(nested, dateTimePattern, `${nestedContext} must be RFC3339 UTC`);
    }
  }, context);
}

function assertUuidFields(value, context) {
  visit(value, (key, nested, nestedContext) => {
    if (nested === null || typeof nested !== 'string') return;
    if (key.endsWith('_id') && !['event_id', 'trace_id'].includes(key)) {
      assert.match(nested, uuidPattern, `${nestedContext} must be a stable UUID`);
    }
  }, context);
}

function assertInOrder(events, ordered) {
  let cursor = -1;
  for (const event of ordered) {
    const index = events.indexOf(event);
    assert(index > cursor, `event ${event} must appear after ${events[cursor]}`);
    cursor = index;
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
