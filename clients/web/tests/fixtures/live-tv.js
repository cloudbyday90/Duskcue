/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

export const liveTvId = (n) => `019a0123-4567-7000-8000-${String(n).padStart(12, '0')}`;

export function createLiveTvFixture() {
    return { status: 200, retryAfter: null, snapshot: liveTvId(1), revision: '9007199254740993', channelCount: 42, programmeCount: 25, malformed: false };
}

export function liveTvResponse(scenario, url) {
    const fixture = scenario.liveTv;
    const status = scenario.profiles.find((entry) => entry.id === scenario.activeProfileId)?.profile_type === 'kids' ? 403 : fixture.status;
    if (status !== 200) return { status, body: { status, title: 'FIXTURE_LIVE_TV', detail: 'PRIVATE_UPSTREAM_ERROR' }, headers: fixture.retryAfter == null ? {} : { 'Retry-After': String(fixture.retryAfter) } };
    const context = {
        profile_id: scenario.activeProfileId, profile_context_revision: fixture.revision, snapshot_id: fixture.snapshot,
        imported_at: '2026-10-26T12:00:00.123456Z', observed_at: '2026-10-26T12:01:00Z', channel_count: fixture.channelCount,
        latest_refresh: { state: 'succeeded', category: null, attempted_at: '2026-10-26T11:59:00Z', finished_at: '2026-10-26T12:00:00.123456Z' },
    };
    const guide = { undeclared: false, ambiguous: true, declarations: [{ display_names: [{ language: 'en', text: 'World guide' }, { language: 'ar', text: 'دليل العالم' }] }] };
    const channelLane = url.pathname.endsWith('/channels');
    const view = url.pathname.endsWith('/unpositionable') ? 'unpositionable' : 'window';
    const offset = Number(url.searchParams.get('cursor')?.split(':')[1] || 0);
    const count = channelLane ? fixture.channelCount : view === 'unpositionable' ? 3 : fixture.programmeCount;
    const items = Array.from({ length: Math.min(20, Math.max(0, count - offset)) }, (_, index) => {
        const n = offset + index;
        if (channelLane) return { id: liveTvId(100 + n), name: n === 0 ? 'World & Culture' : `Channel ${n + 1}`, tvg_name: n === 0 ? 'World guide' : null, group: n === 0 ? 'News · أخبار' : 'General', number: String(n + 1), guide, stream_url: 'https://private-provider.invalid/secret', logo_url: 'https://private-provider.invalid/logo' };
        return {
            id: liveTvId(1000 + n), start_raw: view === 'window' ? '20261026120000 +0000' : '202610', stop_raw: null, clump_index_raw: '0/2',
            starts_at: view === 'window' ? new Date(Date.parse(url.searchParams.get('from')) + n * 60_000).toISOString().replace(/\.\d{3}Z$/, '.123456Z') : null, stops_at: null,
            titles: [{ language: 'fr', text: `Bulletin ${n + 1}` }, { language: 'en', text: n === 0 ? 'Evening bulletin <script>' : `Programme ${n + 1}` }, { language: 'ar', text: 'نشرة المساء' }],
            subtitles: [], descriptions: [{ language: null, text: 'Supplied description. No end time is inferred.' }], categories: [{ language: 'en', text: 'News' }],
            ratings: [{ system: 'provider-custom', value: 'Unmapped-7' }], quality: { uncertain: true, conflict: false, overlap: true, clump: true },
        };
    });
    const has_more = offset + items.length < count;
    const body = { context, items, has_more, ...(has_more ? { cursor: `fixture-${channelLane ? 'channels' : view}:${offset + items.length}` } : {}) };
    if (!channelLane) Object.assign(body, { channel_id: url.pathname.split('/channels/')[1].split('/')[0], guide, view, window: view === 'window' ? { from: url.searchParams.get('from'), to: url.searchParams.get('to') } : null });
    if (fixture.malformed) body.context.profile_context_revision = 9007199254740993;
    return { status: 200, body, headers: { 'Cache-Control': 'no-store' } };
}
