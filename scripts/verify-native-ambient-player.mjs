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

const flutterService = read('clients/mobile/lib/services/ambient_playback_service.dart');
const flutterScreen = read('clients/mobile/lib/screens/ambient_player_screen.dart');
const profileScreen = read('clients/mobile/lib/screens/profile_selection_screen.dart');
const authService = read('clients/mobile/lib/services/auth_service.dart');
const androidService = read('clients/mobile/android/app/src/main/kotlin/com/duskcue/mobile/AmbientPlaybackService.kt');
const androidBridge = read('clients/mobile/android/app/src/main/kotlin/com/duskcue/mobile/AmbientPlaybackBridge.kt');
const androidManifest = read('clients/mobile/android/app/src/main/AndroidManifest.xml');
const androidGradleProperties = read('clients/mobile/android/gradle.properties');
const iosBridge = read('clients/mobile/ios/Runner/AmbientPlaybackBridge.swift');
const iosInfo = read('clients/mobile/ios/Runner/Info.plist');
const contracts = read('docs/api/CLIENT_CONTRACTS.md');
const fixture = read('docs/api/fixtures/playback/v1/ambient-channel-revision.json');

assert.match(flutterService, /MethodChannel\('duskcue\/ambient_player'\)/);
assert.match(flutterService, /'server_origin': origin\.toString\(\)/);
assert.match(flutterService, /'bearer_token': token/);
assert.match(flutterService, /invokeMapMethod<Object\?, Object\?>\('status'\)/);
assert.match(flutterScreen, /AndroidView\(viewType: 'duskcue\/ambient_player_view'\)/);
assert.match(flutterScreen, /UiKitView\(viewType: 'duskcue\/ambient_player_view'\)/);
assert.match(profileScreen, /ambientPlaybackServiceProvider\)\.clear\(\)/);
assert.match(authService, /_ambientPlayback\.clear\(\)/);

assert.match(androidService, /class AmbientPlaybackService : MediaSessionService\(\)/);
assert.match(androidGradleProperties, /org\.gradle\.jvmargs=-Xmx8G/);
assert.match(androidGradleProperties, /android\.useAndroidX=true/);
assert.match(androidGradleProperties, /android\.enableJetifier=true/);
assert.match(androidService, /\/api\/v1\/ambient-channels\/\$\{activeRuntime\.channelId\}\/next/);
assert.match(androidService, /"playback_mode", "ambient"/);
assert.match(androidService, /"ambient_channel_updated_at", selection\.channelUpdatedAt/);
assert.match(androidService, /MAX_STALE_RETRIES = 1/);
assert.match(androidService, /private fun advanceToNext\(\)/);
assert.match(androidService, /"\/api\/v1\/playback\/stop"/);
assert.match(androidService, /runtime !== activeRuntime\) \{\s+networkExecutor\.execute \{[\s\S]*?"\/api\/v1\/playback\/stop"/);
assert.match(androidService, /context\.startService\(intent\)/);
assert.doesNotMatch(androidService, /startForegroundService/);
assert.doesNotMatch(androidService, /SharedPreferences|DataStore|SharedPreferences/);
assert.match(androidBridge, /duskcue\/ambient_player_view/);
assert.match(androidManifest, /FOREGROUND_SERVICE_MEDIA_PLAYBACK/);
assert.match(androidManifest, /android:foregroundServiceType="mediaPlayback"/);

assert.match(iosBridge, /private let player = AVQueuePlayer\(\)/);
assert.match(iosBridge, /session\.setCategory\(\.playback, mode: \.moviePlayback\)/);
assert.match(iosBridge, /"ambient_channel_updated_at": selection\.channelUpdatedAt/);
assert.match(iosBridge, /private func advanceToNext\(afterMediaItemId: String\?\)/);
assert.match(iosBridge, /"after_media_item_id": afterMediaItemId \?\? NSNull\(\)/);
assert.match(iosBridge, /AVPlayerItemFailedToPlayToEndTime/);
assert.match(iosBridge, /guard self\.runtime\?\.id == currentRuntime\.id else \{\s+if case \.success\(let streamURL\) = startResult \{[\s\S]*?"\/api\/v1\/playback\/stop"/);
assert.doesNotMatch(iosBridge, /UserDefaults|Keychain|NSUserDefaults/);
assert.match(iosInfo, /<key>UIBackgroundModes<\/key>\s*<array>\s*<string>audio<\/string>/s);

assert.match(contracts, /native player as the only queue owner/);
assert.match(contracts, /PLAY_019/);
assert.match(contracts, /must never persist a stream URL, bearer\/signed token/);
assert.match(fixture, /"client_action": "discard pending selection and call next again"/);

console.log('Verified Flutter/native ambient queue ownership, revision safety, lifecycle cleanup, and restoration boundaries.');
