// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026 Duskcue Contributors
//
// This program is free software: licensed under AGPL-3.0
// See LICENSE file for details.

import { randomBytes, randomUUID } from 'node:crypto';
import { mkdir, readFile, stat, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { inspectQualificationBinary } from './qualification-binary.mjs';
import { hostedPreparationProvenance, validateHostedManifest } from './qualification-hosted.mjs';

export const repository = fileURLToPath(new URL('../../../../', import.meta.url));

export function qualificationCapabilities() {
    return ['default', {
        identifier: 'tonight-qualification-zoom', windows: ['main'],
        permissions: ['core:webview:allow-set-webview-zoom'],
    }];
}

export async function prepareQualification() {
    if (process.platform !== 'win32') throw new Error('Native qualification requires Windows.');
    const id = randomUUID().replaceAll('-', '');
    const directory = join(repository, '.cache', 'tonight-desktop', id);
    const identifier = `com.duskcue.tonightqualification.t${id}`;
    const bytes = randomBytes(4);
    const hosts = [`127.103.${1 + bytes[0] % 253}.${1 + bytes[1] % 253}`, `127.104.${1 + bytes[2] % 253}.${1 + bytes[3] % 253}`];
    const configPath = join(directory, 'tauri.qualification.json');
    const manifestPath = join(directory, 'manifest.json');
    const manifest = {
        version: 1, preparedAt: new Date().toISOString(), identifier, directory, configPath, manifestPath,
        executable: join(repository, 'target', 'debug', 'duskcue-tonight-qualification.exe'),
        webviewData: join(directory, 'webview-data'),
        expectedAppData: join(process.env.APPDATA, identifier),
        origins: hosts.map((host) => `http://${host}:48027`),
        hostedCI: await hostedPreparationProvenance(),
    };
    await mkdir(directory, { recursive: true });
    await writeFile(configPath, `${JSON.stringify({
        identifier, productName: 'Duskcue Tonight Qualification', mainBinaryName: 'duskcue-tonight-qualification',
        build: { beforeBuildCommand: '' }, bundle: { active: false },
        app: { security: { capabilities: qualificationCapabilities() } },
    }, null, 2)}\n`, 'utf8');
    await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, 'utf8');
    return manifest;
}

export async function readQualification(path) {
    const manifest = JSON.parse(await readFile(resolve(path), 'utf8'));
    if (manifest.version !== 1 || !/^com\.duskcue\.tonightqualification\.t[a-f0-9]{32}$/.test(manifest.identifier)) throw new Error('Use a generated native qualification manifest.');
    if (resolve(manifest.directory) !== resolve(repository, '.cache', 'tonight-desktop', manifest.identifier.split('.t').at(-1))) throw new Error('Qualification artifacts must stay in the dedicated workspace directory.');
    const config = JSON.parse(await readFile(manifest.configPath, 'utf8'));
    if (config.identifier !== manifest.identifier || config.mainBinaryName !== 'duskcue-tonight-qualification') throw new Error('Qualification configuration does not match its manifest.');
    if (!config.app?.security?.capabilities?.some((capability) => capability?.identifier === 'tonight-qualification-zoom'
        && capability.permissions?.includes('core:webview:allow-set-webview-zoom'))) throw new Error('Generate the current QA configuration with its isolated zoom capability before building.');
    const binary = await stat(manifest.executable);
    if (binary.mtimeMs < Date.parse(manifest.preparedAt)) throw new Error('Build the current qualification executable using the generated --config before running.');
    const artifact = await inspectQualificationBinary(manifest.executable, manifest.identifier);
    if (!artifact.containsIdentifier) throw new Error('The executable does not contain this isolated application identifier. Rebuild with the generated --config.');
    await validateHostedManifest(manifest);
    return { ...manifest, executableSha256: artifact.sha256 };
}
