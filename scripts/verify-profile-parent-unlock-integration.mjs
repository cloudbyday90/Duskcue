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

const migration = read('server/migrations/20260718120000_add_kids_parent_unlock.sql');
const cargo = read('Cargo.toml');
const profileService = read('server/src/domains/profiles/service.rs');
const profileTypes = read('server/src/domains/profiles/types.rs');
const profileRoutes = read('server/src/domains/profiles/mod.rs');
const profileErrors = read('server/src/domains/profiles/error.rs');
const webProfiles = read('clients/web/src/lib/api/profiles.js');
const webLayout = read('clients/web/src/routes/+layout.svelte');
const webSettings = read('clients/web/src/routes/settings/profiles/+page.svelte');
const fixture = JSON.parse(read('docs/api/fixtures/auth/v1/auth-flow-matrix.json'));
const storage = JSON.parse(read('docs/api/fixtures/auth/v1/secure-storage-policy.json'));

assert.match(migration, /parent_pin_hash TEXT/);
assert.match(migration, /parent_pin_failed_attempts SMALLINT NOT NULL DEFAULT 0/);
assert.match(migration, /parent_pin_locked_until TIMESTAMPTZ/);
assert.match(migration, /parent_unlock_profile_id UUID/);
assert.match(migration, /parent_unlock_expires_at TIMESTAMPTZ/);
assert.match(cargo, /argon2 = "0\.5"/);
assert.match(profileService, /Algorithm::Argon2id/);
assert.match(profileService, /Params::new\(19 \* 1024, 2, 1, Some\(32\)\)/);
assert.match(profileService, /PARENT_PIN_MAX_ATTEMPTS: i16 = 5/);
assert.match(profileService, /PARENT_PIN_LOCKOUT_MINUTES: i64 = 15/);
assert.match(profileService, /PARENT_UNLOCK_MINUTES: i64 = 10/);
assert.match(profileService, /FOR UPDATE/);
assert.match(profileService, /ParentUnlockRequired/);
assert.match(profileService, /parent_unlock_profile_id = NULL/);
assert.match(profileTypes, /pub parent_pin_configured: bool/);
assert.doesNotMatch(profileTypes, /pub struct ProfileResponse\s*\{[^}]*parent_pin_hash/s);
assert.match(profileRoutes, /"\/api\/v1\/profiles\/parent-unlock"/);
assert.match(profileErrors, /ParentPinLocked/);
assert.match(webProfiles, /function unlockParentProfile/);
assert.match(webLayout, /parent-unlock-dialog/);
assert.match(webSettings, /parent_pin/);

const parentUnlock = fixture.flows.find((flow) => flow.id === 'parent_unlock');
assert(parentUnlock, 'missing parent unlock auth fixture');
assert.equal(parentUnlock.steps[0].expect.parent_unlock_required, true);
assert.equal(parentUnlock.steps[1].expect_problem.title, 'PROFILE_010');
assert.equal(parentUnlock.steps[2].expect_success.unlocked_until, '2026-07-18T16:10:00Z');
assert.equal(parentUnlock.steps[3].expect_success.parent_unlock_required, false);

const parentPin = storage.items.find((item) => item.id === 'parent_pin');
assert(parentPin, 'missing parent PIN storage policy');
assert.equal(parentPin.classification, 'secret');
assert(parentPin.forbidden_storage.includes('local_storage'));
assert(parentPin.forbidden_storage.includes('profile_change_signal'));

console.log('Verified Kids parent-PIN storage, locking, session unlock, and web integration contract.');
