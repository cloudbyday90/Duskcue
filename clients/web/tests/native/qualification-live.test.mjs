// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import test from 'node:test';
import { assertNativeLiveMedia, nativeLiveFailure, nativeLiveScenario } from './qualification-live-policy.mjs';

const item = '00000000-0000-4000-8000-000000000001';
const file = '00000000-0000-4000-8000-000000000002';
const origin = 'http://127.103.21.42:48027';
const manifest = { origins: [origin] };
const fixture = () => ({ resourceId: item, token: 'x'.repeat(48), user: { id: item, username: 'owned',
    active_profile_id: file, profile_selection_required: false, session_token: 'never cache' }, candidate: { item, file } });

test('native fixture accepts only its isolated public origin and canonical user scope', () => {
    const scenario = nativeLiveScenario(manifest, fixture(), origin);
    assert.equal(scenario.origin, origin);
    assert.equal(scenario.user.session_token, undefined);
    for (const target of ['http://127.0.0.1:48027', 'http://127.103.21.42:48028', `${origin}/`, `${origin}?token=secret`]) {
        assert.throws(() => nativeLiveScenario(manifest, fixture(), target));
    }
    const wrong = fixture(); wrong.user.profile_selection_required = true;
    assert.throws(() => nativeLiveScenario(manifest, wrong, origin));
});

test('native proof requires real bearer HLS and rejects cookie or direct-play substitutions', () => {
    const records = [
        { method: 'POST', path: '/api/v1/playback/start', status: 200, bearer: true, cookie: false, request: { force_hls: true }, reply: { session_id: item, transcode_session_id: file, stream_decision: 'direct_stream' } },
        { method: 'GET', path: `/api/v1/transcode/${file}/manifest.m3u8`, status: 200, bearer: true, cookie: false },
        { method: 'GET', path: `/api/v1/transcode/${file}/segment-00001.m4s`, status: 200, bearer: true, cookie: false },
    ];
    assert.equal(assertNativeLiveMedia(records).media.length, 2);
    for (const mutate of [(copy) => { copy[1].bearer = false; }, (copy) => { copy[2].cookie = true; },
        (copy) => { copy[0].request.force_hls = false; }, (copy) => { copy[0].reply.stream_decision = 'direct_play'; }]) {
        const copy = structuredClone(records); mutate(copy); assert.throws(() => assertNativeLiveMedia(copy));
    }
    assert.throws(() => assertNativeLiveMedia(records.slice(0, 2)));
});

test('native error evidence is bounded and excludes known credentials and isolation lookup values', () => {
    const token = fixture().token;
    const message = nativeLiveFailure(new Error(`failed ${token} Bearer other-secret ${'x'.repeat(2000)}`), token, 'connect');
    assert.equal(message.includes(token), false);
    assert.equal(message.includes('other-secret'), false);
    assert.ok(message.length <= 1200);
    assert.equal(nativeLiveFailure(new Error('unknown existing keyring secret'), token, 'isolation'), 'Native isolation check failed.');
});
