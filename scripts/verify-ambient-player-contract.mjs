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
const read = (relativePath) => fs.readFileSync(path.join(root, relativePath), 'utf8');
const readJson = (relativePath) => JSON.parse(read(relativePath));

const migration = read('server/migrations/20260718130000_harden_ambient_playback_contract.sql');
const profileTypes = read('server/src/domains/profiles/types.rs');
const profileService = read('server/src/domains/profiles/service.rs');
const playbackTypes = read('server/src/domains/playback/types.rs');
const playbackService = read('server/src/domains/playback/service.rs');
const playbackError = read('server/src/domains/playback/error.rs');
const appError = read('server/src/error.rs');
const profileRoutes = read('server/src/domains/profiles/mod.rs');
const contracts = readJson('docs/api/client-contracts.v1.json');
const fixture = readJson('docs/api/fixtures/playback/v1/ambient-channel-revision.json');

assert.match(migration, /ADD COLUMN IF NOT EXISTS ambient_channel_id UUID REFERENCES ambient_channels\(id\) ON DELETE SET NULL/);
assert.match(migration, /idx_play_sessions_ambient_channel_id/);
assert.match(migration, /WHERE playback_mode = 'ambient' AND ambient_channel_id IS NOT NULL/);
assert.match(migration, /jsonb_typeof\(ps\.metadata -> 'ambient_channel_id'\) = 'string'/);
assert.match(profileTypes, /pub channel_updated_at: DateTime<Utc>/);
assert.match(profileService, /FOR UPDATE/);
assert.match(profileService, /UPDATE ambient_channels SET updated_at = now\(\)/);
assert.match(profileService, /channel_updated_at: channel\.updated_at/);
assert.match(playbackTypes, /pub ambient_channel_updated_at: Option<DateTime<Utc>>/);
assert.match(playbackTypes, /pub ambient_channel_id: Option<Uuid>/);
assert.match(playbackService, /ambient playback requires ambient_channel_updated_at/);
assert.match(playbackService, /interactive playback cannot include ambient channel fields/);
assert.match(playbackService, /JOIN ambient_channel_items i ON i\.channel_id = c\.id AND i\.media_item_id = \$4/);
assert.match(playbackService, /c\.updated_at = \$9/);
assert.match(playbackError, /AmbientChannelStale/);
assert.match(appError, /"PLAY_019"/);
assert.match(profileRoutes, /"\/api\/v1\/ambient-channels\/\{id\}\/next"/);

const ambientDomain = contracts.domains.find((domain) => domain.name === 'ambient_channels');
assert(ambientDomain, 'missing ambient_channels client contract domain');
assert(ambientDomain.routes.some((route) => route.path === '/api/v1/ambient-channels/{id}/next'));
assert(contracts.phase16d.required_domains.includes('ambient_channels'));

const cases = new Map(fixture.cases.map((entry) => [entry.id, entry]));
const standard = cases.get('standard_channel_start');
assert(standard, 'missing ambient standard-start fixture');
assert.equal(standard.start_request.body.ambient_channel_id, standard.next_response.channel_id);
assert.equal(standard.start_request.body.ambient_channel_updated_at, standard.next_response.channel_updated_at);
const stale = cases.get('stale_channel_revision');
assert.equal(stale.response.status, 409);
assert.equal(stale.response.problem.title, 'PLAY_019');
assert.equal(stale.expect.play_session_created, false);
assert.equal(stale.expect.stream_url_returned, false);

console.log('Verified ambient queue revision, diagnostics, stale-start rejection, and native handoff contract.');
