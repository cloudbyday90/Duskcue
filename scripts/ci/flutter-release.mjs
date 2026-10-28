// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { readFile, stat } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export function selectFlutterRelease(metadata, expected, architecture) {
    if (metadata?.base_url !== 'https://storage.googleapis.com/flutter_infra_release/releases'
        || !Array.isArray(metadata.releases) || metadata.releases.length > 10000
        || !['x64', 'arm64'].includes(architecture)) throw new Error('Invalid official Flutter release manifest.');
    const matches = metadata.releases.filter((item) => item.channel === 'stable' && item.version === expected.flutter
        && item.hash === expected.revision && item.dart_sdk_version === expected.dart && item.dart_sdk_arch === architecture);
    if (matches.length !== 1) throw new Error('The reviewed Flutter/Dart release and architecture are unavailable.');
    const release = matches[0];
    if (!/^[a-f0-9]{64}$/.test(release.sha256 || '') || !/^stable\/(?:linux|macos|windows)\/[A-Za-z0-9_.-]+$/.test(release.archive || '')) {
        throw new Error('Invalid official SDK archive identity.');
    }
    const platform = release.archive.split('/')[1];
    const filename = `flutter_${platform}${architecture === 'arm64' ? '_arm64' : ''}_${expected.flutter}-stable.${platform === 'linux' ? 'tar.xz' : 'zip'}`;
    if (release.archive !== `stable/${platform}/${filename}`) throw new Error('The SDK archive filename differs from the reviewed release.');
    return { archive: release.archive, sha256: release.sha256 };
}

async function main() {
    if (process.argv.length !== 3 || (await stat(process.argv[2])).size > 4194304) throw new Error('Provide one bounded official release manifest.');
    const metadata = JSON.parse(await readFile(process.argv[2], 'utf8'));
    const expected = JSON.parse(await readFile(new URL('../testing-memory/flutter-toolchain.json', import.meta.url), 'utf8'));
    const release = selectFlutterRelease(metadata, expected, process.arch);
    process.stdout.write(`${release.archive}\n${release.sha256}\n`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
    main().catch((error) => { process.stderr.write(`${error.message}\n`); process.exitCode = 1; });
}
