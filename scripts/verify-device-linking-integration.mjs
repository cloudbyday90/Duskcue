import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const read = (relativePath) => fs.readFileSync(path.join(root, relativePath), 'utf8');
const contract = JSON.parse(read('docs/api/client-contracts.v1.json'));
const fixture = JSON.parse(read('docs/api/fixtures/auth/v1/auth-flow-matrix.json'));
const handlers = read('server/src/domains/auth/handlers.rs');
const service = read('server/src/domains/auth/service.rs');
const migration = read('server/migrations/20260718100000_harden_device_linking.sql');
const authRouter = read('server/src/domains/auth/mod.rs');
const linkPage = read('clients/web/src/routes/auth/link/+page.svelte');
const loginPage = read('clients/web/src/routes/auth/login/+page.svelte');
const layout = read('clients/web/src/routes/+layout.svelte');

const authDomain = contract.domains.find((domain) => domain.name === 'auth');
assert(authDomain, 'missing auth domain contract');
const routes = new Map(authDomain.routes.map((route) => [`${route.method} ${route.path}`, route]));

assert.equal(routes.get('POST /api/v1/device/code')?.response, 'DeviceCodeResponse');
assert.equal(routes.get('GET /api/v1/device/verify')?.response, 'DeviceLinkingRequestResponse');
assert.equal(routes.get('POST /api/v1/device/verify')?.request, 'DeviceVerifyRequest');
assert.deepEqual(
  routes.get('POST /api/v1/device/token')?.contract.errors.problem_codes,
  ['VALID_001', 'AUTH_013', 'AUTH_014', 'AUTH_023', 'AUTH_024', 'INTERNAL'],
);

const deviceFlow = fixture.flows.find((flow) => flow.id === 'device_linking');
assert(deviceFlow, 'missing device-linking fixture');
assert.equal(deviceFlow.steps[0].expect.verification_uri, 'https://duskcue.example.test/auth/link');
assert.equal(
  deviceFlow.steps[0].expect.verification_uri_complete,
  'https://duskcue.example.test/auth/link?code=ABCD-EFGH',
);
assert.equal(deviceFlow.steps[1].expect_pending.title, 'AUTH_023');

assert(handlers.includes('format!("{}/auth/link", base.trim_end_matches(\'/\'))'));
assert(!handlers.slice(handlers.indexOf('pub async fn device_code'), handlers.indexOf('pub async fn device_token')).includes('get("host")'));
assert(service.includes('FOR UPDATE'));
assert(service.includes('DeviceLinkingSlowDown'));
assert(service.includes('is_denied'));
assert(migration.includes('last_polled_at'));
assert(migration.includes('poll_interval_seconds'));
assert(authRouter.includes('get(handlers::device_linking_request).post(handlers::device_verify)'));
assert(linkPage.includes('getDeviceLinkingRequest'));
assert(linkPage.includes('handleDecision(false)'));
assert(layout.includes('return_to='));
assert(loginPage.includes('postLoginDestination'));

console.log('Verified device-linking server, client, migration, contract, and fixture integration.');
