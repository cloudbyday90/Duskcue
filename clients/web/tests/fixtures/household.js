/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */
import { createHash } from 'node:crypto';

export function createHouseholdFixture() {
    return { sessionId: '019a4567-1234-7000-8000-000000000701', revision: '0', profileRevision: '1', enabled: false,
        trustedPassword: false, trustedKeys: 0, grant: null, password: 'household-fixture-password', handoff: 'owner-handoff-fixture', refusal: null };
}
export function fixtureSessionContext(scenario, transport = 'cookie') {
    const state = scenario.household;
    const context = { user_id: scenario.user.id, session_id: state.sessionId, profile_id: scenario.activeProfileId,
        credential_revision: state.revision, profile_context_revision: state.profileRevision, profile_selection_required: scenario.selectionRequired,
        transport, session_expires_at: '2030-01-01T00:00:00Z', observed_at: new Date().toISOString() };
    return { ...context, context_id: `sc1.${createHash('sha256').update(JSON.stringify([context.user_id, context.session_id, context.profile_id, state.revision, state.profileRevision, scenario.selectionRequired])).digest('base64url')}` };
}
export async function handleHouseholdFixture(route, scenario, { authenticated = scenario.authenticated, transport = 'cookie', rotateToken = null } = {}) {
    const request = route.request();
    const path = new URL(request.url()).pathname.replace('/api/v1', '');
    if (path !== '/auth/session-context' && !path.startsWith('/household-management/')) return false;
    const json = async (body, status = 200) => { await route.fulfill({ status, contentType: 'application/json', headers: { 'Cache-Control': 'no-store' }, body: JSON.stringify(body) }); return true; };
    if (!authenticated) return json({ status: 401, title: 'AUTH_005' }, 401);
    const context = fixtureSessionContext(scenario, transport);
    if (path === '/auth/session-context') return json(context);
    if (scenario.profiles.find((entry) => entry.id === scenario.activeProfileId)?.profile_type !== 'standard') return json({ status: 403, title: 'OWNER_001' }, 403);
    const headers = await request.allHeaders();
    if (headers['x-session-context'] && headers['x-session-context'] !== context.context_id) return json({ status: 409, title: 'OWNER_007' }, 409);
    const state = scenario.household;
    if (state.refusal) return json(state.refusal, state.refusal.status);
    if (path.endsWith('/status')) return json({ context, protection_enabled: state.enabled, expires_at: state.grant, password_available: state.trustedPassword, trusted_passkeys: state.trustedKeys });
    if (path.endsWith('/grant') && request.method() === 'DELETE') { state.grant = null; await route.fulfill({ status: 204, headers: { 'Cache-Control': 'no-store' } }); return true; }
    if (path.endsWith('/password')) {
        const body = request.postDataJSON();
        if (body.password !== state.password || (body.handoff ? body.handoff !== state.handoff : !state.trustedPassword)) return json({ status: 401, title: 'OWNER_002', detail: 'PRIVATE_OWNER_REFUSAL' }, 401);
        state.enabled = true; state.trustedPassword = true; state.handoff = null;
        state.revision = String(BigInt(state.revision) + 1n);
        state.grant = new Date(Date.now() + 300_000).toISOString();
        const token = rotateToken ? rotateToken() : null;
        return json({ context: fixtureSessionContext(scenario, transport), expires_at: state.grant, ...(transport === 'bearer' ? { session_token: token } : {}) });
    }
    return json({ status: 400, title: 'OWNER_005' }, 400);
}
