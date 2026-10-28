// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { createHash, randomUUID } from 'node:crypto';
import { createWriteStream } from 'node:fs';
import { access, mkdir, readFile, realpath, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Readable, Transform } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { runBounded } from '../qualification/commands.mjs';
import { hashFile } from '../qualification/source.mjs';

const workspace = fileURLToPath(new URL('../../', import.meta.url));
const directory = join(workspace, '.cache/android-playback/sdk');
const expected = JSON.parse(await readFile(new URL('./flutter-toolchain.json', import.meta.url), 'utf8'));

async function main() {
    await mkdir(directory, { recursive: true });
    const response = await fetch(expected.metadata, { redirect: 'error', signal: AbortSignal.timeout(30000) });
    if (!response.ok || Number(response.headers.get('content-length')) > 4194304) throw new Error('Official Flutter release metadata was unavailable.');
    const chunks = []; let bytes = 0;
    for await (const chunk of response.body) {
        bytes += chunk.length;
        if (bytes > 4194304) throw new Error('Flutter release metadata exceeded its bound.');
        chunks.push(chunk);
    }
    const metadata = JSON.parse(Buffer.concat(chunks).toString('utf8'));
    const release = metadata.releases.find((entry) => entry.hash === metadata.current_release.stable && entry.dart_sdk_arch === 'x64');
    if (metadata.base_url !== 'https://storage.googleapis.com/flutter_infra_release/releases'
        || release?.version !== expected.flutter || release.dart_sdk_version !== expected.dart || release.hash !== expected.revision
        || release.sha256 !== expected.archiveSha256 || release.archive !== `stable/windows/flutter_windows_${expected.flutter}-stable.zip`) {
        throw new Error('The official stable release differs from the reviewed toolchain.');
    }
    const archive = join(directory, `flutter-${expected.flutter}.zip`);
    let cached = false;
    try { cached = await hashFile(archive) === expected.archiveSha256; }
    catch (error) { if (error.code !== 'ENOENT') throw error; }
    if (!cached) {
        try { await access(archive); throw new Error('Existing SDK archive failed verification; preserve it for review.'); }
        catch (error) { if (error.code !== 'ENOENT') throw error; }
        const download = await fetch(`${metadata.base_url}/${release.archive}`, { redirect: 'error', signal: AbortSignal.timeout(900000) });
        if (!download.ok || Number(download.headers.get('content-length')) > 3 * 1024 ** 3) throw new Error('Official SDK download failed its size/status check.');
        let total = 0, milestone = 0;
        const digest = createHash('sha256');
        const bounded = new Transform({ transform(chunk, _, callback) {
            total += chunk.length; digest.update(chunk);
            if (total > 3 * 1024 ** 3) return callback(new Error('SDK archive exceeded its bound.'));
            if (total - milestone >= 128 * 1024 ** 2) { milestone = total; process.stdout.write(`SDK downloaded: ${Math.floor(total / 1024 ** 2)} MiB\n`); }
            callback(null, chunk);
        } });
        await pipeline(Readable.fromWeb(download.body), bounded, createWriteStream(archive, { flags: 'wx' }));
        if (digest.digest('hex') !== expected.archiveSha256) throw new Error('SDK archive checksum failed.');
    }
    const propertiesPath = join(workspace, 'clients/mobile/android/local.properties');
    const properties = await readFile(propertiesPath, 'utf8');
    const old = await realpath(properties.match(/^flutter\.sdk=(.+)$/m)[1].trim().replaceAll('\\\\', '\\'));
    const parent = await realpath(dirname(old));
    const oldVersion = JSON.parse(await readFile(join(old, 'bin/cache/flutter.version.json'), 'utf8'));
    const selected = oldVersion.frameworkRevision === expected.revision && oldVersion.dartSdkVersion === expected.dart;
    const destination = selected ? parent : resolve(parent, `flutter-${expected.flutter}`);
    if (!selected && (dirname(destination) !== parent || destination === old)) throw new Error('New SDK destination is outside its designated parent.');
    const sdk = selected ? old : join(destination, 'flutter');
    let exists = false;
    try { await access(destination); exists = true; } catch (error) { if (error.code !== 'ENOENT') throw error; }
    if (!exists) {
        await mkdir(destination);
        const unpack = await runBounded('tar.exe', ['-xf', archive, '-C', destination], {
            cwd: workspace, timeoutMs: 900000, bytes: 1048576, log: join(directory, `extract-${randomUUID()}.log`) });
        if (unpack.code !== 0 || unpack.signal) throw new Error('SDK extraction did not complete.');
    }
    const version = JSON.parse(await readFile(join(sdk, 'bin/cache/flutter.version.json'), 'utf8'));
    if (version.frameworkVersion !== expected.flutter || version.dartSdkVersion !== expected.dart || version.frameworkRevision !== expected.revision) throw new Error('Extracted Flutter identity did not match the official release.');
    const dart = join(sdk, 'bin/cache/dart-sdk/bin/dart.exe');
    const result = await runBounded(dart, ['--old_gen_heap_size=1536', '--version'], {
        timeoutMs: 10000, bytes: 32768, env: { ...process.env, DART_SUPPRESS_ANALYTICS: 'true' } });
    if (result.code !== 0 || !`${result.stdout}${result.stderr}`.includes(`Dart SDK version: ${expected.dart} (stable)`)) throw new Error('Actual Dart executable identity failed.');
    const settingsPath = join(workspace, '.vscode/settings.json');
    let settings = {};
    try { settings = JSON.parse(await readFile(settingsPath, 'utf8')); }
    catch (error) { if (error.code !== 'ENOENT') throw error; }
    settings['dart.flutterSdkPath'] = sdk.replaceAll('\\', '/');
    await writeFile(propertiesPath, properties.replace(/^flutter\.sdk=.+$/m, `flutter.sdk=${sdk.replaceAll('\\', '/')}`));
    await mkdir(dirname(settingsPath), { recursive: true });
    await writeFile(settingsPath, `${JSON.stringify(settings, null, 2)}\n`);
    await writeFile(join(directory, 'installed.json'), JSON.stringify({ ...expected, dartSha256: await hashFile(dart), installedAt: new Date().toISOString() }, null, 2));
    process.stdout.write(`Project toolchain selected: Flutter ${expected.flutter}, Dart ${expected.dart}.\n`);
}

main().catch((error) => { process.stderr.write(`${error.message}\n`); process.exitCode = 1; });
