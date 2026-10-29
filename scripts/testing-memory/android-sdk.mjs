// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { mkdir, readFile, realpath, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { runBounded } from '../qualification/commands.mjs';
import { hashFile } from '../qualification/source.mjs';

const workspace = fileURLToPath(new URL('../../', import.meta.url));

async function main() {
    if (!process.env.DUSKCUE_TEST_RESOURCE_ID || process.platform !== 'win32') throw new Error('Use the managed Windows resource workflow.');
    const directory = join(workspace, '.cache/android-playback/sdk');
    await mkdir(directory, { recursive: true });
    const properties = await readFile(join(workspace, 'clients/mobile/android/local.properties'), 'utf8');
    const sdk = await realpath(properties.match(/^sdk\.dir=(.+)$/m)[1].trim().replaceAll('\\\\', '\\').replaceAll('\\:', ':'));
    const java = await realpath('C:/Program Files/Eclipse Adoptium/jdk-17.0.19.10-hotspot');
    const executable = join(java, 'bin/java.exe');
    if (!(await readFile(join(java, 'release'), 'utf8')).includes('JAVA_VERSION="17.0.19"')) throw new Error('Reviewed JDK changed.');
    const licenses = await readFile(join(sdk, 'licenses/android-sdk-license'), 'utf8');
    const before = await hashFile(join(sdk, 'licenses/android-sdk-license'));
    if (!licenses.split(/\s+/).includes('24333f8a63b6825ea9c5514f83c2829b004d1fee')) throw new Error('Existing accepted SDK terms are required.');
    const manager = join(sdk, 'cmdline-tools/latest');
    let needsInstall = false;
    for (const [path, revision] of [['emulator/source.properties', '37.2.12'], ['system-images/android-36/default/x86_64/source.properties', '2']]) {
        try {
            const existing = await readFile(join(sdk, path), 'utf8');
            if (existing.match(/^Pkg\.Revision=(.+)$/m)?.[1]?.trim() !== revision) throw new Error('An installed Android package differs from the reviewed version; no automatic upgrade was requested.');
        } catch (error) { if (error.code === 'ENOENT') needsInstall = true; else throw error; }
    }
    const result = needsInstall ? await runBounded(executable, ['-Xmx384m', `-Dcom.android.sdklib.toolsdir=${manager}`, '-classpath',
        join(manager, 'lib/sdkmanager-classpath.jar'), 'com.android.sdklib.tool.sdkmanager.SdkManagerCli',
        `--sdk_root=${sdk}`, '--channel=0', '--install', 'emulator', 'system-images;android-36;default;x86_64'], {
        cwd: workspace, timeoutMs: 900000, bytes: 16777216, log: join(directory, `android-install-${Date.now()}.log`),
        env: { ...process.env, JAVA_HOME: java, ANDROID_HOME: sdk, ANDROID_SDK_ROOT: sdk },
    }) : { code: 0 };
    if (await hashFile(join(sdk, 'licenses/android-sdk-license')) !== before) throw new Error('SDK license acceptance changed unexpectedly.');
    if (result.code !== 0 || result.signal) throw new Error('SDK installation failed; no license response was supplied. Inspect the owned log.');
    const emulator = join(sdk, 'emulator/emulator.exe');
    const image = join(sdk, 'system-images/android-36/default/x86_64');
    const emuProperties = await readFile(join(sdk, 'emulator/source.properties'), 'utf8');
    const imageProperties = await readFile(join(image, 'source.properties'), 'utf8');
    if (!/^Pkg\.Revision=37\.2\.12\s*$/m.test(emuProperties) || !/^Pkg\.Revision=2\s*$/m.test(imageProperties)) throw new Error('Installed packages differ from reviewed official stable metadata.');
    const acceleration = await runBounded(emulator, ['-accel-check'], { timeoutMs: 15000, bytes: 65536 });
    if (acceleration.code !== 0 || !/WHPX.*installed and usable/.test(acceleration.stdout + acceleration.stderr)) throw new Error('Actual emulator cannot use WHPX.');
    const receipt = { sdk, image, emulator, emulatorRevision: '37.2.12', imageRevision: 2, api: 36, abi: 'x86_64',
        emulatorSha256: await hashFile(emulator), adbSha256: await hashFile(join(sdk, 'platform-tools/adb.exe')),
        licenseFileSha256: before, acceleration: (acceleration.stdout + acceleration.stderr).trim(), installedAt: new Date().toISOString() };
    await writeFile(join(directory, 'android-installed.json'), JSON.stringify(receipt, null, 2));
    process.stdout.write(`Verified emulator 37.2.12/API 36 r2; ${receipt.acceleration}\n`);
}

main().catch((error) => { process.stderr.write(`${error.message}\n`); process.exitCode = 1; });
