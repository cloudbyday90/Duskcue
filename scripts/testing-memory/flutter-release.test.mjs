// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import test from 'node:test';
import assert from 'node:assert/strict';
import { selectFlutterRelease } from '../ci/flutter-release.mjs';

const expected = { flutter: '3.47.8', dart: '3.13.7', revision: 'a'.repeat(40) };
const release = { channel: 'stable', version: expected.flutter, hash: expected.revision,
    dart_sdk_version: expected.dart, dart_sdk_arch: 'x64', archive: 'stable/linux/flutter_linux_3.47.8-stable.tar.xz', sha256: 'b'.repeat(64) };
const manifest = (rows) => ({ base_url: 'https://storage.googleapis.com/flutter_infra_release/releases', releases: rows });

test('CI selects reviewed toolchain even when a newer stable release exists', () => {
    const newer = { ...release, version: '3.50.0', hash: 'c'.repeat(40) };
    assert.deepEqual(selectFlutterRelease(manifest([newer, release]), expected, 'x64'), { archive: release.archive, sha256: release.sha256 });
});
test('version, compiler, revision and architecture must all match', () => {
    for (const change of [{ version: '3.47.7' }, { dart_sdk_version: '3.12.2' }, { hash: 'c'.repeat(40) }, { dart_sdk_arch: 'arm64' }, { channel: 'beta' }]) {
        assert.throws(() => selectFlutterRelease(manifest([{ ...release, ...change }]), expected, 'x64'));
    }
});
test('ambiguous archives and untrusted paths/checksums are refused', () => {
    assert.throws(() => selectFlutterRelease(manifest([release, release]), expected, 'x64'));
    for (const change of [{ archive: '../../outside.zip' }, { archive: 'https://unapproved.invalid/sdk.zip' }, { archive: 'stable/linux/flutter_linux_3.50.0-stable.tar.xz' }, { sha256: 'missing' }]) {
        assert.throws(() => selectFlutterRelease(manifest([{ ...release, ...change }]), expected, 'x64'));
    }
});
