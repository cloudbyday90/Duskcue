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
const fireRoot = path.join(root, 'clients', 'tv', 'fire');
const buildFile = readFile('clients/tv/fire/app/build.gradle.kts');
const manifest = readFile('clients/tv/fire/app/src/main/AndroidManifest.xml');
const fireSource = readTree(path.join(fireRoot, 'app', 'src', 'main'), '.kt');
const design = readFile('docs/design/FIRE_TV.md');
const readme = readFile('clients/tv/fire/README.md');
const sdkSink = readFile('clients/tv/fire/app/src/main/java/com/duskcue/fire/personalization/FireSdkPlaybackSink.kt');
const playbackService = readFile('clients/tv/android/app/src/main/java/com/duskcue/tv/playback/TvPlaybackService.kt');
const runtime = readFile('clients/tv/fire/app/src/main/java/com/duskcue/tv/TvApplicationRuntime.kt');
const watchFixture = JSON.parse(readFile('docs/api/fixtures/fire/v1/watch-activity.json'));
const catalogFixture = JSON.parse(readFile('docs/api/fixtures/fire/v1/catalog-authorization.json'));
const fireRoutes = readFile('server/src/domains/tv/fire/mod.rs');
const registryMigration = readFile('server/migrations/20261003010000_create_fire_catalog_registry.sql');
const launchPlayer = readFile('clients/tv/android/app/src/main/java/com/duskcue/tv/playback/TvAuthenticatedDeepLinkPlayer.kt');
const appController = readFile('clients/tv/android/app/src/main/java/com/duskcue/tv/ui/TvAppController.kt');

for (const file of [
  'clients/tv/fire/build.gradle.kts',
  'clients/tv/fire/settings.gradle.kts',
  'clients/tv/fire/gradle.properties',
  'clients/tv/fire/gradlew',
  'clients/tv/fire/gradlew.bat',
  'clients/tv/fire/app/build.gradle.kts',
  'clients/tv/fire/app/src/main/AndroidManifest.xml',
  'clients/tv/fire/README.md',
]) {
  assert(fs.existsSync(path.join(root, file)), `missing Fire TV target file ${file}`);
}

for (const token of [
  'applicationId = "com.duskcue.firetv"',
  'minSdk = 28',
  'compileSdk = 36',
  'targetSdk = 36',
  '../../android/app/src/main/java/com/duskcue/tv/api',
  '../../android/app/src/main/java/com/duskcue/tv/ui',
]) {
  assert(buildFile.includes(token), `Fire TV build target missing ${token}`);
}
assert(!buildFile.includes('androidx.tvprovider'), 'Fire TV build must not depend on AndroidX tvprovider');
assert(!buildFile.includes('watchnext'), 'Fire TV build must not include Android TV Watch Next sources');
assert(buildFile.includes('compileOnly(files(prepareFireContentSdk'), 'Amazon SDK must be compile-only');
assert(buildFile.includes('f3094973bbb18b5a58807ad043d31055ade6a2f6646bcae54a4d1022b9cdf593'), 'Amazon SDK archive checksum must be pinned');
assert(buildFile.includes('.orElse("false")'), 'Watch Activity must default to disabled');
assert(manifest.includes('com.amazon.tv.developer.sdk.content.USE_SDK'), 'Amazon SDK permission missing');
assert(/<uses-library\s+android:name="com.amazon.tv.developer.sdk.content"\s+android:required="false"\s*\/>/.test(manifest), 'Amazon system library must be optional');
assert(sdkSink.includes('hasSystemFeature('), 'Amazon SDK requires a device library check');
assert(sdkSink.includes('if (!available()) return false'), 'Every SDK send requires a fresh library check');
for (const token of ['NAMESPACE_CDF_ID', 'NAMESPACE_APP_INTERNAL', 'buildActiveEvent()', 'addPlaybackEvent(payload)']) {
  assert(sdkSink.includes(token), `Amazon SDK adapter missing ${token}`);
}
for (const signal of ['Loaded', 'StateChanged', 'Seek', 'Tick']) {
  assert(playbackService.includes(`reportObserverSample(TvPlaybackSignal.${signal})`), `Media3 observer missing ${signal}`);
}
assert(playbackService.indexOf('it.exited(finalSample)') < playbackService.indexOf('player?.clearMediaItems()'), 'Exit must sample Media3 before clearing content');
for (const cleanup of ['clearProfileScope', 'clearIdentityScope']) {
  const body = runtime.slice(runtime.indexOf(`override suspend fun ${cleanup}`));
  assert(body.indexOf('fireAuthorization.clear()') >= 0 && body.indexOf('fireAuthorization.clear()') < body.indexOf('TvPlaybackService.stop('), `${cleanup} must revoke reporting before stopping playback`);
}
assert.equal(watchFixture.version, 1);
assert.equal(watchFixture.event_type, 'active');
assert.equal(watchFixture.content_namespace, 'cdf_id');
assert.equal(watchFixture.profile_namespace, 'app_internal');
assert(watchFixture.samples.some((sample) => sample.trigger === 'Loaded' && sample.position_ms > 0), 'Fixture must cover loaded resume position');
assert(watchFixture.samples.some((sample) => sample.trigger === 'Tick' && sample.playing === false && sample.expected_state === 'PAUSED'), 'Fixture must cover paused cadence');
for (const state of ['PLAYING', 'PAUSED', 'EXIT']) {
  assert(watchFixture.samples.some((sample) => sample.expected_state === state), `Fixture missing ${state}`);
}
assert(fs.existsSync(path.join(fireRoot, 'app/src/test/java/com/duskcue/fire/personalization/FireWatchActivityReporterTest.kt')), 'Watch Activity regression coverage missing');
assert(fireRoutes.includes('"/api/v1/tv/fire/playback/{session_id}/authorization"'), 'Fire authorization route missing');
assert(registryMigration.includes('UNIQUE (catalog_reference, amazon_content_id)'), 'Accepted IDs must remain uniquely registered');
assert(registryMigration.includes('ON DELETE SET NULL'), 'Deleted media must retain a catalog tombstone');
assert(!readFile('server/src/domains/tv/types.rs').includes('AmazonCatalog'), 'Synthetic Amazon content-ID encoding must remain removed');
assert.equal(
  readFile('clients/tv/fire/app/src/main/java/com/duskcue/tv/TvDeepLink.kt').trim(),
  readFile('clients/tv/android/app/src/main/java/com/duskcue/tv/TvDeepLink.kt').trim(),
  'Fire and Android canonical launch parsing must agree',
);
assert(appController.includes('deepLinkPlayer.launch(deepLink)'), 'UI must use the verified authenticated launch coordinator');
for (const token of ['scopeIsCurrent()', 'discard(session, playback, resolved.resume_position_ms)', 'resolved.resume_position_ms < 0', 'resolved.access_revalidated']) {
  assert(launchPlayer.includes(token), `Launch coordinator missing ${token}`);
}
for (const token of ['expectedSession: ActiveTvSession?', 'requestIsCurrent()', 'coordinator.sessionSnapshot()']) {
  assert(runtime.includes(token), `Fire runtime missing launch scope guard ${token}`);
}
assert(buildFile.includes('../../android/app/src/test/java/com/duskcue/tv/playback'), 'Fire must run shared suspended-launch regression tests');
assert.equal(catalogFixture.cache_control, 'private, no-store');
assert.equal(catalogFixture.accepted.opaque_profile_key.length, 43);
assert.equal(catalogFixture.denied.eligible, false);
for (const field of ['catalog_content_id', 'opaque_profile_key', 'mapping_revision', 'expires_at']) {
  assert.equal(catalogFixture.denied[field], null, `Denied authorization leaks ${field}`);
}

for (const token of [
  'amazon.hardware.fire_tv',
  'android.software.leanback',
  'androidx.media3.session.MediaButtonReceiver',
  'android.intent.action.MEDIA_BUTTON',
  'android.intent.category.LEANBACK_LAUNCHER',
  'android:scheme="duskcue"',
  'android:usesCleartextTraffic="false"',
]) {
  assert(manifest.includes(token), `Fire TV manifest missing ${token}`);
}
for (const forbidden of ['WatchNextProgramReceiver', 'WatchNextArtworkProvider', 'androidx.tvprovider']) {
  assert(!manifest.includes(forbidden), `Fire TV manifest must not include ${forbidden}`);
}
for (const forbidden of ['WatchNext', 'watchnext', 'tvprovider', 'Google Play']) {
  assert(!fireSource.includes(forbidden), `Fire TV source must not include ${forbidden}`);
}
for (const token of ['TvDeviceProfile.fireTv()', 'TvPlatform.FireTv', 'platform = "fire_tv"', 'integrationState = "app_local_only"']) {
  assert(fireSource.includes(token), `Fire TV source missing ${token}`);
}
for (const token of ['Fire OS 7/API 28', 'Kids profile never sends personalization activity', 'exact, currently accepted Amazon catalog content ID', 'Vega is a separate']) {
  assert(design.includes(token), `Fire TV design missing ${token}`);
}
for (const token of ['com.duskcue.firetv', 'AndroidX Watch Next', 'Fire TV device is required']) {
  assert(readme.includes(token), `Fire TV README missing ${token}`);
}

console.log('Verified Fire TV app target, SDK isolation, Watch Activity fixtures, playback wiring, and integration gates.');

function readFile(relativePath) {
  return fs.readFileSync(path.join(root, relativePath), 'utf8');
}

function readTree(directory, extension) {
  return fs.readdirSync(directory, { withFileTypes: true })
    .flatMap((entry) => {
      const entryPath = path.join(directory, entry.name);
      return entry.isDirectory() ? readTree(entryPath, extension) : entry.name.endsWith(extension) ? [fs.readFileSync(entryPath, 'utf8')] : [];
    })
    .join('\n');
}
