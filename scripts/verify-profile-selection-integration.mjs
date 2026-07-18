import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const read = (relativePath) => fs.readFileSync(path.join(root, relativePath), 'utf8');

const migration = read('server/migrations/20260718110000_add_profile_selection_state.sql');
const authService = read('server/src/domains/auth/service.rs');
const profileService = read('server/src/domains/profiles/service.rs');
const profileTypes = read('server/src/domains/profiles/types.rs');
const profileRoutes = read('server/src/domains/profiles/mod.rs');
const webProfiles = read('clients/web/src/lib/api/profiles.js');
const webCore = read('clients/web/src/lib/api/core.js');
const webScope = read('clients/web/src/lib/profiles/scope.js');
const webLayout = read('clients/web/src/routes/+layout.svelte');
const fixture = JSON.parse(read('docs/api/fixtures/auth/v1/auth-flow-matrix.json'));

assert.match(migration, /profile_selection_required BOOLEAN NOT NULL DEFAULT false/);
assert.match(authService, /requires_profile_selection\(remembered_profile_id\.is_some\(\), profile_count\)/);
assert.match(authService, /profile_selection_required: should_require_profile_selection/);
assert.match(profileService, /profile_selection_required = false/);
assert.match(profileService, /let mut transaction = pool\.begin\(\)\.await\?/);
assert.match(profileTypes, /pub profile_selection_required: bool/);
assert.match(profileRoutes, /"\/api\/v1\/profiles"/);
assert.match(webProfiles, /profileScoped: false/);
assert.match(webCore, /invalidateProfileScopedRequests/);
assert.match(webCore, /ensureProfileScopeCurrent/);
assert.match(webScope, /BroadcastChannel/);
assert.match(webScope, /storage/);
assert.match(webLayout, /profile-gate/);
assert.match(webLayout, /profileScopeReady/);
assert.match(webLayout, /publishProfileScopeChange/);
assert.match(webLayout, /resetProfileScope/);

const profileSelection = fixture.flows.find((flow) => flow.id === 'profile_selection');
assert(profileSelection, 'missing profile selection auth fixture');
assert.equal(profileSelection.steps[0].expect_success.profile_selection_required, true);
assert.equal(profileSelection.steps[2].expect_success.profile_selection_required, false);

console.log('Verified shared-TV profile selection server, client, migration, and fixture integration.');
