// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { spawn } from 'node:child_process';
import { createServer, connect } from 'node:net';
import { mkdir, readFile, writeFile, appendFile } from 'node:fs/promises';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { randomUUID } from 'node:crypto';
import { runBounded } from '../qualification/commands.mjs';
import { hashFile } from '../qualification/source.mjs';
import { stopOwnedProcess } from './owned-process.mjs';
import { validateAndroidHardware, validateAndroidAvd } from './android-evidence.mjs';

const guestMiB = 2560;

export async function portAvailable(port = 0) {
    const server = createServer();
    await new Promise((resolve, reject) => { server.once('error', reject); server.listen({ host: '127.0.0.1', port, exclusive: true }, resolve); });
    const selected = server.address().port;
    await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
    return selected;
}

export async function listening(port) {
    return new Promise((resolve) => {
        const socket = connect({ host: '127.0.0.1', port });
        let done = false;
        const finish = (value) => { if (!done) { done = true; socket.destroy(); resolve(value); } };
        socket.once('connect', () => finish(true)); socket.once('error', () => finish(false));
        socket.setTimeout(1000, () => finish(true));
    });
}

function own(executable, args, env, log) {
    const child = spawn(executable, args, { env, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
    child.startedAt = Date.now();
    let total = 0, writes = Promise.resolve(), failure;
    const completion = new Promise((resolve, reject) => { child.once('error', reject); child.once('close', (code, signal) => resolve({ code, signal })); });
    completion.catch(() => {});
    for (const stream of [child.stdout, child.stderr]) stream.on('data', (chunk) => {
        total += chunk.length;
        if (total > 16777216) { failure ||= new Error('Owned Android process log exceeded its bound.'); child.kill(); return; }
        stream.pause();
        writes = writes.then(() => appendFile(log, chunk)).catch((error) => { failure ||= error; child.kill(); }).finally(() => stream.resume());
    });
    return { child, completion, async observe(deadline) {
        let timer;
        try { await Promise.race([completion, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('Owned Android cleanup was not confirmed within thirty seconds.')), Math.max(1, deadline - Date.now())); })]); }
        finally { clearTimeout(timer); await writes; }
        if (failure) throw failure;
    } };
}

export async function createAndroidOwner(workspace, directory) {
    if (process.platform !== 'win32' || !process.env.DUSKCUE_TEST_RESOURCE_ID) throw new Error('Use the managed Windows native workflow.');
    if (guestMiB < 2560) throw new Error('Configured Android guest RAM is below the reviewed emulator minimum.');
    const installed = JSON.parse(await readFile(join(workspace, '.cache/android-playback/sdk/android-installed.json'), 'utf8'));
    const adb = join(installed.sdk, 'platform-tools/adb.exe');
    if (await hashFile(installed.emulator) !== installed.emulatorSha256 || await hashFile(adb) !== installed.adbSha256) throw new Error('Android executable identities changed.');
    const acceleration = await runBounded(installed.emulator, ['-accel-check'], { timeoutMs: 15000, bytes: 65536 });
    if (acceleration.code !== 0 || !/WHPX.*installed and usable/.test(acceleration.stdout + acceleration.stderr)) throw new Error('WHPX is not usable.');
    await mkdir(directory, { recursive: true });
    const avdRoot = join(directory, 'avd'), user = join(directory, 'user');
    await mkdir(avdRoot); await mkdir(user);
    const name = `duskcue_${randomUUID().replaceAll('-', '')}`;
    const avd = join(avdRoot, `${name}.avd`);
    await mkdir(avd);
    const slash = (path) => path.replaceAll('\\', '/');
    await writeFile(join(avdRoot, `${name}.ini`), `avd.ini.encoding=UTF-8\npath=${slash(avd)}\ntarget=android-36\n`);
    await writeFile(join(avd, 'config.ini'), [
        'avd.ini.encoding=UTF-8', `AvdId=${name}`, `avd.ini.displayname=${name}`, 'abi.type=x86_64',
        `image.sysdir.1=${slash(installed.image)}/`, 'hw.cpu.arch=x86_64', 'hw.cpu.ncore=2', `hw.ramSize=${guestMiB}`,
        'hw.lcd.width=720', 'hw.lcd.height=1280', 'hw.lcd.density=320', 'hw.gpu.enabled=yes', 'hw.gpu.mode=swiftshader',
        'hw.keyboard=yes', 'hw.audioInput=no', 'hw.audioOutput=no', 'hw.mainKeys=no', 'disk.dataPartition.size=2048M',
        'showDeviceFrame=no', 'fastboot.forceColdBoot=yes', 'PlayStore.enabled=false', 'tag.id=default', 'tag.display=Default',
        'vm.heapSize=256', 'runtime.network.latency=none', 'runtime.network.speed=full', '',
    ].join('\n'));
    const adbPort = await portAvailable();
    let consolePort;
    for (let port = 5580; port < 5680; port += 2) {
        try { await portAvailable(port); await portAvailable(port + 1); consolePort = port; break; } catch {}
    }
    if (!consolePort) throw new Error('No isolated emulator port pair is available.');
    const serial = `emulator-${consolePort}`;
    const env = { ...process.env, ANDROID_HOME: installed.sdk, ANDROID_SDK_ROOT: installed.sdk,
        ANDROID_USER_HOME: user, ANDROID_EMULATOR_HOME: user, ANDROID_AVD_HOME: avdRoot,
        ANDROID_ADB_SERVER_PORT: String(adbPort), ADB_SERVER_SOCKET: `tcp:127.0.0.1:${adbPort}`, ANDROID_SERIAL: serial };
    const server = own(adb, ['-L', `tcp:${adbPort}`, 'server', 'nodaemon'], env, join(directory, 'adb-server.log'));
    let emulator;
    let closed = false;
    const ownership = { name, serial, adbPort, consolePort, adbPid: server.child.pid, status: 'starting', cleanup: false };
    const persistOwner = () => writeFile(join(directory, 'owner.json'), JSON.stringify(ownership, null, 2));
    const command = async (args, options = {}) => {
        const result = await runBounded(adb, ['-P', String(adbPort), '-s', serial, ...args], { env, timeoutMs: 15000, bytes: 1048576, ...options });
        if (result.code !== 0 || result.signal) throw new Error(`Owned ADB ${args[0]} failed.`);
        return result.stdout.trim();
    };
    const close = async () => {
        if (closed) return;
        const errors = [];
        const deadline = Date.now() + 30000;
        if (emulator) {
            if (emulator.child.exitCode === null && emulator.child.signalCode === null) {
                try { await command(['emu', 'kill'], { timeoutMs: Math.max(1, Math.min(5000, deadline - Date.now())) }); } catch (error) { errors.push(error); }
            }
            try { await emulator.observe(deadline); } catch (error) { errors.push(error); await stopOwnedProcess(emulator.child); await emulator.completion; }
        }
        try {
            const result = await runBounded(adb, ['-P', String(adbPort), 'kill-server'], { env, timeoutMs: Math.max(1, Math.min(5000, deadline - Date.now())), bytes: 65536 });
            if (result.code !== 0) throw new Error('Owned ADB server did not accept shutdown.');
            await server.observe(deadline);
        } catch (error) { errors.push(error); await stopOwnedProcess(server.child); await server.completion; }
        for (const port of [consolePort, consolePort + 1, adbPort]) if (await listening(port)) errors.push(new Error('Owned Android listener remains after cleanup.'));
        try {
            const processes = await runBounded('pwsh.exe', ['-NoProfile', '-Command',
                `$owned = @(Get-CimInstance Win32_Process -Filter "Name LIKE 'qemu%' OR Name = 'emulator.exe'" | Where-Object { $_.CommandLine -like '*${name}*' }); $owned.Count`],
            { timeoutMs: 10000, bytes: 65536 });
            if (processes.code !== 0 || processes.stdout.trim() !== '0') throw new Error('Owned Android process exit remains unconfirmed.');
        } catch (error) { errors.push(error); }
        if (Date.now() > deadline) errors.push(new Error('Android cleanup exceeded its thirty-second observation deadline.'));
        closed = errors.length === 0;
        ownership.cleanup = closed;
        ownership.status = closed ? 'closed' : 'cleanup_uncertain';
        await persistOwner();
        if (errors.length) throw new AggregateError(errors, 'Android cleanup is unconfirmed; retain this owner.');
    };
    try {
        await persistOwner();
        const serverDeadline = Date.now() + 15000;
        while (!await listening(adbPort)) {
            if (server.child.exitCode !== null || Date.now() >= serverDeadline) throw new Error('Owned ADB server was not ready.');
            await delay(100);
        }
        emulator = own(installed.emulator, ['-avd', name, '-ports', `${consolePort},${consolePort + 1}`,
            '-no-window', '-no-snapshot', '-no-boot-anim', '-no-audio', '-cores', '2', '-memory', String(guestMiB),
            '-gpu', 'swiftshader', '-feature', '-Vulkan', '-accel', 'on', '-no-metrics'], env, join(directory, 'emulator.log'));
        ownership.emulatorPid = emulator.child.pid;
        await persistOwner();
        const bootDeadline = Date.now() + 600000;
        while (true) {
            if (emulator.child.exitCode !== null || emulator.child.signalCode !== null) throw new Error('Owned emulator exited before boot.');
            try {
                const actual = await readFile(join(avd, 'hardware-qemu.ini'), 'utf8');
                validateAndroidHardware(actual, guestMiB);
            } catch (error) { if (error.code !== 'ENOENT') throw error; }
            try { if (await command(['shell', 'getprop', 'sys.boot_completed'], { timeoutMs: 5000 }) === '1') break; } catch {}
            if (Date.now() >= bootDeadline) throw new Error('Owned Android boot deadline expired.');
            await delay(2000);
        }
        const identity = await command(['emu', 'avd', 'name']);
        await writeFile(join(directory, 'avd-identity.json'), JSON.stringify({ expected: name, response: identity }));
        validateAndroidAvd(identity, name);
        const properties = { api: await command(['shell', 'getprop', 'ro.build.version.sdk']), abi: await command(['shell', 'getprop', 'ro.product.cpu.abi']) };
        const bootId = await command(['shell', 'cat', '/proc/sys/kernel/random/boot_id']);
        if (!/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/.test(bootId)) throw new Error('Owned Android boot identity is invalid.');
        if (properties.api !== '36' || properties.abi !== 'x86_64') throw new Error('Booted Android platform differs from reviewed image.');
        await writeFile(join(directory, 'device.json'), JSON.stringify({ name, serial, adbPort, consolePort, properties,
            emulatorPid: emulator.child.pid, adbPid: server.child.pid, acceleration: installed.acceleration, graphics: 'swiftshader', requestedFeatureFlags: ['-Vulkan'], guestMiB, cores: 2, bootId }, null, 2));
        ownership.status = 'booted';
        await persistOwner();
        return { command, env, serial, installed, adb, directory, close, bootId };
    } catch (error) { try { await close(); } catch (cleanup) { throw new AggregateError([error, cleanup]); } throw error; }
}
