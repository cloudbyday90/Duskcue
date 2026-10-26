/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */
import { createHash } from 'node:crypto';
import { fixtureSessionContext } from './household.js';

const uuid = (n) => `019a4567-1234-7000-8000-${String(n).padStart(12, '0')}`;
export const managementIds = { target: uuid(801), control: uuid(802), assignment: uuid(803), attempt: uuid(804), snapshot: uuid(805) };
const tag = (value) => `"${createHash('sha256').update(JSON.stringify(value)).digest('hex')}"`;
export function createManagementFixture() {
    return { assignments: new Map(), profiles: new Map(), attempts: new Map(), control: { id: managementIds.control, revision: 1, enabled: false }, writes: [], starts: 0, users: [], counter: 900, finishState: 'succeeded', refusal: null };
}
export function fixtureAssignment(user, extra = {}) {
    return { assignment_id: managementIds.assignment, household_user_id: user, revision: 1, source_generation: 1, playlist_destination_approved: false, guide_destination_approved: false, playlist_address_count: 0, guide_address_count: 0, configured: true, playlist_transport: 'http', guide_transport: 'http', enabled: false, max_live_streams: null, owner_max_live_streams: null, configured_live_tv_cap: null, allow_owner_profile_access: false, allow_owner_live_limit: false, network_checked: false, playback_available: false, ...extra };
}
export async function handleManagementFixture(route, scenario, { authenticated = scenario.authenticated, transport = 'cookie' } = {}) {
    const request = route.request(); const url = new URL(request.url()); const path = url.pathname.replace('/api/v1', ''); const method = request.method();
    const owner = path === '/household-management/live-tv' || path === '/household-management/live-tv/profiles';
    const operator = path === '/live-tv/manual-refresh-control';
    const match = /^\/live-tv\/households\/([^/]+)\/(assignment|refresh)(\/status)?$/.exec(path);
    if (!owner && !operator && !match && path !== '/users') return false;
    const state = scenario.management;
    const json = async (body, status = 200, headers = {}) => { await route.fulfill({ status, contentType: status >= 400 ? 'application/problem+json' : 'application/json', headers: { 'Cache-Control': 'no-store', 'Access-Control-Expose-Headers': 'ETag, Location, Retry-After', ...headers }, body: JSON.stringify(body) }); return true; };
    const problem = (status, title) => json({ status, title, detail: 'PRIVATE_MANAGEMENT_REFUSAL' }, status);
    if (!authenticated) return problem(401, 'AUTH_005');
    if (scenario.profiles.find((p) => p.id === scenario.activeProfileId)?.profile_type !== 'standard') return problem(403, 'OWNER_001');
    const permission = operator ? 'can_manage_server' : 'can_manage_users';
    if (!owner && scenario.user.role !== 'owner' && !scenario.user.capabilities?.includes(permission)) return problem(403, 'IPTV_001');
    if (path === '/users') {
        const all = [scenario.user, { ...scenario.user, id: managementIds.target, username: 'second-household', display_name: 'Second household' }, ...state.users];
        const page = Number(url.searchParams.get('page') || 1); return json({ items: all.slice((page - 1) * 20, page * 20), total: all.length, page, page_size: 20, total_pages: Math.ceil(all.length / 20) });
    }
    const headers = await request.allHeaders(); const context = fixtureSessionContext(scenario, transport);
    if (headers['x-session-context'] && headers['x-session-context'] !== context.context_id) return problem(409, 'OWNER_007');
    const grant = scenario.household.grant && Date.parse(scenario.household.grant) > Date.now();
    if ((owner || method !== 'GET') && !grant) return problem(403, 'OWNER_001');
    if (state.refusal) return problem(state.refusal.status, state.refusal.title);
    const household = owner ? scenario.user.id : match?.[1];
    let saved = state.assignments.get(household);
    const view = () => ({ ...saved, management_scope: owner ? 'household_owner' : 'administrator' });
    const resource = () => ({ assignment_id: saved.assignment_id, revision: saved.revision, source_generation: saved.source_generation, assignment_enabled: saved.enabled, playlist_address_count: saved.playlist_address_count, guide_address_count: saved.guide_address_count, control: state.control, latest_attempt_id: state.attempts.get(household)?.attempt_id || null, latest_attempt_sequence: state.attempts.get(household)?.attempt_sequence || null });
    if (operator) {
        if (method === 'GET') return json(state.control, 200, { ETag: tag(state.control) });
        if (headers['if-match'] !== tag(state.control)) return problem(412, 'IPTV_003');
        const body = request.postDataJSON(); state.writes.push({ path, method, operation: 'control' });
        if (state.control.enabled !== body.enabled) state.control = { ...state.control, revision: state.control.revision + 1, enabled: body.enabled };
        return json(state.control, 200, { ETag: tag(state.control) });
    }
    if (!saved && method !== 'PUT') return problem(404, 'IPTV_002');
    if (match?.[2] === 'refresh') {
        if (match[3]) {
            const latest = state.attempts.get(household) || null; const expected = url.searchParams.get('attempt_id');
            return json({ latest, expected_attempt_id: expected, observation: !latest ? 'never_imported' : expected && expected !== latest.attempt_id ? 'not_current' : 'current', locally_owned: latest?.state === 'running', service_available: true });
        }
        if (method === 'GET') return json(resource(), 200, { ETag: tag(resource()) });
        if (headers['if-match'] !== tag(resource())) return problem(412, 'IPTV_003');
        if (!state.control.enabled) return problem(503, 'IPTV_REFRESH_004');
        const body = request.postDataJSON(); if (body.playlist_pin >= saved.playlist_address_count || body.guide_pin >= saved.guide_address_count) return problem(400, 'IPTV_REFRESH_001');
        state.starts += 1; state.writes.push({ path, method, operation: 'refresh' });
        const attempt = { assignment_id: saved.assignment_id, attempt_id: uuid(++state.counter), attempt_sequence: state.starts, state: state.finishState, category: state.finishState === 'failed' ? 'format' : null,
            attempted_at: new Date().toISOString(), finished_at: state.finishState === 'running' ? null : new Date().toISOString(), snapshot_id: managementIds.snapshot, imported_at: new Date().toISOString(), channel_count: 1, guide_count: 1, programme_count: 1, current_source: true };
        state.attempts.set(household, attempt); const status_url = `/api/v1/live-tv/households/${household}/refresh/status?attempt_id=${attempt.attempt_id}`;
        return json({ attempt_id: attempt.attempt_id, attempt_sequence: attempt.attempt_sequence, status_url }, 202, { Location: status_url });
    }
    if (path.endsWith('/profiles')) {
        const all = scenario.profiles.map((p) => ({ id: p.id, name: p.name, profile_type: p.profile_type, enabled: state.profiles.get(p.id) || false, eligible: p.profile_type === 'standard' }));
        return json({ assignment_revision: saved.revision, items: all, next_cursor: null }, 200, { ETag: tag(all) });
    }
    if (method === 'GET') return json(view(), 200, { ETag: tag(view()) });
    if (method === 'PUT') {
        if (saved || headers['if-none-match'] !== '*') return problem(412, 'IPTV_003');
        const body = request.postDataJSON(); saved = fixtureAssignment(household, { assignment_id: uuid(++state.counter), enabled: body.enabled ?? false }); state.assignments.set(household, saved);
    } else {
        if (headers['if-match'] !== tag(view())) return problem(412, 'IPTV_003');
        if (method === 'DELETE') { state.assignments.delete(household); state.attempts.delete(household); if (household === scenario.user.id) scenario.household.grant = null; state.writes.push({ path, method, operation: 'delete' }); await route.fulfill({ status: 204, headers: { 'Cache-Control': 'no-store' } }); return true; }
        const body = request.postDataJSON();
        if (owner) {
            if (!saved.enabled || (body.operation === 'set_profile_access' ? !saved.allow_owner_profile_access : !saved.allow_owner_live_limit)) return problem(403, 'IPTV_001');
            if (body.operation === 'set_profile_access') { if (scenario.profiles.find((p) => p.id === body.profile_id)?.profile_type !== 'standard') return problem(403, 'IPTV_001'); state.profiles.set(body.profile_id, body.enabled); }
            else saved.owner_max_live_streams = body.max_live_streams;
        } else {
            if (body.operation === 'configure') Object.assign(saved, { enabled: body.enabled, max_live_streams: body.max_live_streams, allow_owner_profile_access: body.allow_owner_profile_access, allow_owner_live_limit: body.allow_owner_live_limit });
            if (body.operation === 'approve_destination' || body.operation === 'revoke_destination') { saved[`${body.feed}_address_count`] = body.operation === 'approve_destination' ? body.addresses.length : 0; saved[`${body.feed}_destination_approved`] = body.operation === 'approve_destination'; }
            if (body.operation === 'replace_assignment') { saved.assignment_id = uuid(++state.counter); saved.owner_max_live_streams = null; for (const feed of ['playlist', 'guide']) { saved[`${feed}_address_count`] = 0; saved[`${feed}_destination_approved`] = false; } state.profiles.clear(); state.attempts.delete(household); }
            saved.source_generation += 1;
        }
        saved.revision += 1; saved.configured_live_tv_cap = saved.max_live_streams === null ? saved.owner_max_live_streams : saved.owner_max_live_streams === null ? saved.max_live_streams : Math.min(saved.max_live_streams, saved.owner_max_live_streams);
    }
    const body = request.postDataJSON(); state.writes.push({ path, method, operation: body.operation || 'create' });
    if (!owner && household === scenario.user.id) scenario.household.grant = null;
    return json(view(), method === 'PUT' ? 201 : 200, { ETag: tag(view()) });
}
