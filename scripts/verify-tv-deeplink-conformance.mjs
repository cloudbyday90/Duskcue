import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const fixtureDir = path.join(root, 'docs', 'api', 'fixtures', 'tv', 'v1');
const manifest = readJson(path.join(fixtureDir, 'manifest.json'));

const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const platformContentIdPattern = /^duskcue:(movie|episode):[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const dateTimePattern = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/;
const forbiddenPatterns = [
  /bearer\s+[A-Za-z0-9._-]+/i,
  /token=/i,
  /signature=/i,
  /X-Amz-Signature=/i,
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
  assert(fixtures.has(id), `missing TV conformance fixture ${id}`);
}

assertSurfaceContract(fixtures.get('tv-surface-contract'));
assertDeepLinkResolve(fixtures.get('tv-deep-link-resolve'));
assertPlatformAdapters(fixtures.get('tv-platform-adapter-mappings'));
assertAccessRevalidation(fixtures.get('tv-access-revalidation'));

console.log(`Verified ${fixtures.size} TV/deep-link conformance fixtures.`);

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function assertSurfaceContract(fixture) {
  assertRequest(fixture.request, 'surface.request');
  assert.equal(fixture.headers['cache-control'], 'private, max-age=60, stale-while-revalidate=300');
  assert.match(fixture.headers.etag, /^".+"$/);
  assert.equal(fixture.body.sections.length, manifest.required_sections.length);
  assert.deepEqual(fixture.body.sections.map((section) => section.section_type), manifest.required_sections);

  const totalItems = fixture.body.sections.reduce((sum, section) => sum + section.items.length, 0);
  assert(totalItems <= fixture.body.limit, 'TV surface exceeds declared limit');

  for (const section of fixture.body.sections) {
    if (section.items.length === 0) {
      assert(section.empty_reason, `${section.section_type} empty section needs empty_reason`);
    }
    for (const item of section.items) {
      assert.equal(item.section_type, section.section_type);
      assert.match(item.media_item_id, uuidPattern);
      assert.match(item.platform_content_id, platformContentIdPattern);
      assert(item.deep_link.startsWith(`duskcue://play/${item.media_type}/`));
      assert(item.deep_link.endsWith(item.media_item_id));
      assert(item.web_url.endsWith(item.media_item_id));
      assert(item.poster_url.startsWith('/api/v1/'));
      assert(['playable', 'missing_file', 'metadata_incomplete'].includes(item.availability));
      assert(item.progress_percent >= 0 && item.progress_percent <= 100);
    }
  }

  assert.equal(fixture.body.access_filtering.library_access_checked, true);
  assert.equal(fixture.body.access_filtering.revoked_items_omitted, true);
}

function assertDeepLinkResolve(fixture) {
  const cases = new Map(fixture.cases.map((item) => [item.id, item]));
  for (const id of manifest.required_resolve_cases) {
    assert(cases.has(id), `missing resolve case ${id}`);
  }

  for (const item of fixture.cases) {
    assertRequest(item.request, `${item.id}.request`);
    assert(item.request.path.startsWith('/api/v1/tv/resolve/'), `${item.id} must resolve through TV API`);
    if (item.status === 200) {
      assert.match(item.body.platform_content_id, platformContentIdPattern);
      assert.match(item.body.media_item_id, uuidPattern);
      assert.equal(item.body.requires_auth, true);
      assert.equal(item.body.access_revalidated, true);
      assert.equal(item.body.playback_action, 'start_playback');
      assertRequest(item.body.playback_start, `${item.id}.playback_start`);
      assert.equal(item.body.playback_start.path, '/api/v1/playback/start');
      assert.equal(item.body.playback_start.media_item_id, item.body.media_item_id);
    } else {
      assertProblem(item.problem, item.id);
      assert(item.expected_client_action, `${item.id} missing client action`);
    }
  }
}

function assertPlatformAdapters(fixture) {
  assert(fixture.source_surface_endpoint.startsWith('/api/v1/'));
  const adapters = new Map(fixture.adapters.map((item) => [item.id, item]));
  for (const id of manifest.required_platform_adapters) {
    assert(adapters.has(id), `missing platform adapter ${id}`);
  }
  for (const adapter of fixture.adapters) {
    assert(adapter.source_section_types.every((section) => manifest.required_sections.includes(section)));
    assert.equal(adapter.requirements.revalidate_before_playback, true, `${adapter.id} must revalidate before playback`);
    assertPlatformIds(adapter.mapping, adapter.id);
  }
  assert.equal(adapters.get('android_tv_watch_next').mapping.watch_next_type, 'continue');
  assert.equal(adapters.get('roku_search_direct_to_play').mapping.launch_behavior, 'direct_to_play');
  assert.equal(adapters.get('lg_webos_launch_params').requirements.handle_webOSRelaunch, true);
  assert.equal(adapters.get('apple_tvos_top_shelf_universal_links').mapping.universal_link_required, true);
  assert.equal(adapters.get('xbox_uri_activation').requirements.handle_protocol_activation, true);
}

function assertAccessRevalidation(fixture) {
  const cases = new Map(fixture.cases.map((item) => [item.id, item]));
  for (const id of manifest.required_revalidation_cases) {
    assert(cases.has(id), `missing access revalidation case ${id}`);
  }
  assert.equal(cases.get('launcher_cached_resume_stale').expect.ignore_cached_resume, true);
  assert.equal(cases.get('session_revoked_after_publication').expect.clear_bearer_token, true);
  assert.equal(cases.get('library_access_revoked_after_publication').expect.do_not_start_playback, true);
  assert.equal(cases.get('different_duskcue_user_selected').expect.do_not_send_previous_user_token, true);
  assert.equal(cases.get('platform_id_deleted_or_replaced').expect.do_not_retry_without_user_action, true);

  for (const item of fixture.cases) {
    if (item.entry?.platform_content_id) {
      assert.match(item.entry.platform_content_id, platformContentIdPattern);
    }
    if (item.resolve_request) {
      assertRequest(item.resolve_request, `${item.id}.resolve_request`);
    }
    if (item.resolve_response?.problem) {
      assertProblem(item.resolve_response.problem, item.id);
    }
  }
}

function assertPlatformIds(value, context) {
  visit(value, (key, nested, nestedContext) => {
    if (typeof nested !== 'string') return;
    if (['content_id', 'contentId', 'preview_id', 'top_shelf_identifier', 'platform_content_id'].includes(key)) {
      assert.match(nested, platformContentIdPattern, `${nestedContext} must be a Duskcue platform content ID`);
    }
    if (['intent_uri', 'deep_link_intent_uri', 'deeplink_data', 'uri'].includes(key)) {
      assert(nested.startsWith('duskcue://play/'), `${nestedContext} must be Duskcue playback URI`);
    }
  }, context);
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
      assertNoPrivateValues(nested, `${context}.${key}`);
    }
  }
}

function assertDateTimes(value, context) {
  visit(value, (key, nested, nestedContext) => {
    if (nested === null) return;
    if (['generated_at', 'last_engaged_at', 'published_at'].includes(key)) {
      assert.equal(typeof nested, 'string', `${nestedContext} must be a string date-time`);
      assert.match(nested, dateTimePattern, `${nestedContext} must be RFC3339 UTC`);
    }
  }, context);
}

function assertUuidFields(value, context) {
  visit(value, (key, nested, nestedContext) => {
    if (nested === null || typeof nested !== 'string') return;
    if (key.endsWith('_id') && !['app_id', 'content_id', 'platform_content_id', 'preview_id', 'surface_item_id', 'trace_id'].includes(key)) {
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
