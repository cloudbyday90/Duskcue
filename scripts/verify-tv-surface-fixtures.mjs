import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const fixtureDir = path.join(root, 'docs', 'api', 'fixtures', 'tv');

const sectionOrder = ['continue', 'next_up', 'new_episodes', 'recommended'];
const sectionLabels = {
    continue: 'Continue Watching',
    next_up: 'Next Up',
    new_episodes: 'New Episodes',
    recommended: 'Recommended'
};
const canonicalIdPattern = /^duskcue:(movie|episode):[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const forbiddenValuePatterns = [
    /[A-Za-z]:\\/,
    /\\\\[^\\]+\\/,
    /\/mnt\//,
    /\/home\//,
    /\/var\//,
    /bearer\s+[A-Za-z0-9._-]+/i,
    /token=/i,
    /signature=/i
];

function readFixture(name) {
    return JSON.parse(fs.readFileSync(path.join(fixtureDir, name), 'utf8'));
}

function renderRows(surfaceBody) {
    return surfaceBody.sections.map((section) => ({
        section_type: section.section_type,
        label: sectionLabels[section.section_type] ?? section.title,
        item_count: section.items.length,
        items: section.items.map((item) => ({
            platform_content_id: item.platform_content_id,
            title: item.title,
            subtitle: item.subtitle,
            progress_percent: Number(item.progress_percent)
        }))
    }));
}

function assertNoPrivateValues(value, context = 'fixture') {
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

function assertSurfaceFixture(fixture) {
    assert.equal(fixture.request.method, 'GET');
    assert(fixture.request.path.startsWith('/api/v1/users/me/tv-surface'));
    assert.equal(
        fixture.headers['cache-control'],
        'private, max-age=60, stale-while-revalidate=300',
        `${fixture.fixture} must document private TV cache policy`
    );
    assert.match(fixture.headers.etag, /^".+"$/, `${fixture.fixture} must document quoted ETag`);

    const body = fixture.body;
    assert.equal(body.sections.length, sectionOrder.length, `${fixture.fixture} section count`);
    assert.deepEqual(
        body.sections.map((section) => section.section_type),
        sectionOrder,
        `${fixture.fixture} section order`
    );

    const totalItems = body.sections.reduce((sum, section) => sum + section.items.length, 0);
    assert(totalItems <= body.limit, `${fixture.fixture} exceeds declared limit`);

    for (const section of body.sections) {
        assert.equal(section.title, sectionLabels[section.section_type]);
        if (section.items.length === 0) {
            assert(section.empty_reason, `${fixture.fixture}.${section.section_type} needs empty_reason`);
        }
        for (const item of section.items) {
            assert.equal(item.section_type, section.section_type);
            assert.match(item.media_item_id, uuidPattern);
            assert.match(item.platform_content_id, canonicalIdPattern);
            assert(item.deep_link.includes(item.media_item_id));
            assert(item.web_url.endsWith(item.media_item_id));
            assert(['playable', 'missing_file', 'metadata_incomplete'].includes(item.availability));
            assert(item.progress_percent >= 0 && item.progress_percent <= 100);
        }
    }

    assertNoPrivateValues(body, fixture.fixture);
}

function assertResolvePlayable(fixture) {
    const body = fixture.body;
    assert.match(body.platform_content_id, canonicalIdPattern);
    assert.equal(body.playback_action, 'start_playback');
    assert.equal(body.playback_start.method, 'POST');
    assert.equal(body.playback_start.path, '/api/v1/playback/start');
    assert.equal(body.playback_start.media_item_id, body.media_item_id);
    assert.equal(body.playback_start.start_position_ms, body.resume_position_ms);
    assert.equal(body.requires_auth, true);
    assertNoPrivateValues(body, fixture.fixture);
}

function assertResolveUnavailable(fixture) {
    assert.equal(fixture.status, 404);
    assert.equal(fixture.body.title, 'TV_002');
    assert.equal(fixture.body.status, 404);
    assertNoPrivateValues(fixture.body, fixture.fixture);
}

function assertDiagnosticsAccessRevoked(fixture) {
    const body = fixture.body;
    assert.equal(body.included_count, 0);
    assert(body.reason_counts.some((entry) => entry.reason === 'access_revoked' && entry.count === 1));
    assert(body.excluded.some((entry) => entry.reason === 'access_revoked'));
    assertNoPrivateValues(body, fixture.fixture);
}

const surfaceFull = readFixture('surface-full.json');
const surfaceEmpty = readFixture('surface-empty.json');
const surfaceAccessRevoked = readFixture('surface-access-revoked.json');
const resolvePlayable = readFixture('resolve-playable.json');
const resolveUnavailable = readFixture('resolve-unavailable.json');
const diagnosticsAccessRevoked = readFixture('diagnostics-access-revoked.json');
const goldenRender = readFixture('golden-render.json');

for (const fixture of [surfaceFull, surfaceEmpty, surfaceAccessRevoked]) {
    assertSurfaceFixture(fixture);
}
assertResolvePlayable(resolvePlayable);
assertResolveUnavailable(resolveUnavailable);
assertDiagnosticsAccessRevoked(diagnosticsAccessRevoked);

assert.equal(goldenRender.source, 'surface-full.json');
assert.deepEqual(renderRows(surfaceFull.body), goldenRender.rows);

console.log('Verified 7 TV surface fixtures and golden renderer output.');
