// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { androidPackage, androidActivity, androidPid, androidProcessIdentity, androidPausedIsolate, androidCommandIdentity } from './android-vm-identity.mjs';
import { androidTcpMapping } from './android-evidence.mjs';
import { ownAndroidAppLogs } from './android-app-logs.mjs';
import { androidVmRead } from './android-vm-probe.mjs';
import { listening } from './android-owner.mjs';

export async function bootstrapAndroidDriver(owner, { fixture, directory, port, budget, onPhase }, { ownLogs = ownAndroidAppLogs, readVm = androidVmRead, isListening = listening } = {}) {
    const command = async (args) => {
        try { return await owner.command(args, { timeoutMs: Math.min(5000, budget.remaining(true)), bytes: 65536, outputBudget: budget, abortSignal: budget.signal }); }
        catch (error) { throw new Error(`Android bootstrap command refused: ${args.join(' ')}`, { cause: error }); }
    };
    let logs, forwarded = false, forwardAttempted = false, identity, ready = false, abortReady, expectedIsolate;
    const readiness = { polls: 0, last: [] };
    const verify = async () => {
        if (await command(['shell', 'cat', '/proc/sys/kernel/random/boot_id']) !== owner.bootId) throw new Error('Android guest rebooted during bootstrap.');
        const pid = androidPid(await command(['shell', 'pidof', androidPackage]));
        const cmdline = await command(['shell', 'run-as', androidPackage, 'cat', `/proc/${pid}/cmdline`]);
        androidCommandIdentity(cmdline);
        const current = androidProcessIdentity(await command(['shell', 'run-as', androidPackage, 'cat', `/proc/${pid}/stat`]), pid);
        if (identity && JSON.stringify(identity) !== JSON.stringify(current)) throw new Error('Android package process was replaced.');
        identity ||= current;
        if (forwarded && !androidTcpMapping(await command(['forward', '--list']), port, owner.serial)) throw new Error('Owned Android VM forward disappeared.');
    };
    const close = async () => {
        const errors = [];
        try { await logs?.close(); } catch (error) { errors.push(error); }
        if (forwardAttempted) try {
            const before = await owner.command(['forward', '--list']);
            const present = androidTcpMapping(before, port, owner.serial);
            if (forwarded && !present) throw new Error('Owned Android VM forward disappeared before cleanup.');
            if (present) await owner.command(['forward', '--remove', `tcp:${port}`]);
            const after = await owner.command(['forward', '--list']);
            const deadline = Date.now() + 5000;
            while (await isListening(port) && Date.now() < deadline) await delay(50);
            if (androidTcpMapping(after, port, owner.serial) || await isListening(port)) throw new Error('Owned Android VM forward remains.');
            await writeFile(join(directory, 'debug-mappings.json'), JSON.stringify({ before, after, port }));
            forwarded = false;
            forwardAttempted = false;
        } catch (error) { errors.push(error); }
        if (errors.length) throw new AggregateError(errors, 'Owned Android driver bootstrap cleanup is unconfirmed.');
    };
    const timer = setTimeout(() => { if (!ready) budget.fail(new Error('Android bootstrap exceeded thirty seconds.')); }, budget.remaining(true));
    try {
        if (await command(['shell', `pidof ${androidPackage} || true`])) throw new Error('Android package was already running before paused launch.');
        await command(['logcat', '-b', 'main', '-c']);
        if (androidTcpMapping(await command(['forward', '--list']), port, owner.serial)) throw new Error('Android VM forward already exists.');
        forwardAttempted = true;
        await command(['forward', '--no-rebind', `tcp:${port}`, `tcp:${port}`]); forwarded = true;
        await command(['shell', 'am', 'start', '-a', 'android.intent.action.MAIN', '-c', 'android.intent.category.LAUNCHER',
            '-f', '0x20000000', '--ez', 'enable-dart-profiling', 'true', '--ez', 'enable-checked-mode', 'true',
            '--ez', 'verify-entry-points', 'true', '--ez', 'start-paused', 'true', '-n', androidActivity]);
        while (true) {
            const observed = await command(['shell', `pidof ${androidPackage} || true`]);
            if (observed) { androidPid(observed); break; }
            await delay(Math.min(100, budget.remaining(true)), undefined, { signal: budget.signal });
        }
        await verify();
        logs = ownLogs(owner, { pid: identity.pid, port, directory, fixture, budget, onPhase });
        const uri = await Promise.race([logs.ready, new Promise((_, reject) => {
            abortReady = () => reject(budget.failure || new Error('Android bootstrap was cancelled.'));
            if (budget.signal.aborted) abortReady();
            else budget.signal.addEventListener('abort', abortReady, { once: true });
        })]);
        let isolateId;
        while (true) {
            const vm = await readVm(uri, 'getVM', {}, budget);
            isolateId = androidPausedIsolate(vm);
            readiness.polls++;
            if (expectedIsolate && expectedIsolate !== isolateId) throw new Error('Android pending main isolate was replaced.');
            if (isolateId) {
                expectedIsolate ||= isolateId;
                const isolate = await readVm(uri, 'getIsolate', { isolateId }, budget);
                const kind = isolate.pauseEvent?.kind;
                readiness.last.push({ isolateId, runnable: isolate.runnable === true, pause: ['None', 'PauseStart', 'Resume', 'PauseExit', 'PauseBreakpoint', 'PauseException'].includes(kind) ? kind : 'unexpected' });
                if (readiness.last.length > 8) readiness.last.shift();
                if (androidPausedIsolate(vm, isolate)) break;
            }
            await delay(Math.min(100, budget.remaining(true)), undefined, { signal: budget.signal });
        }
        await verify();
        ready = true;
        await writeFile(join(directory, 'bootstrap-identity.json'), JSON.stringify({ ...identity, port, bootId: owner.bootId, isolateId, paused: true }));
        return { uri, logs, close };
    } catch (error) {
        try { await close(); } catch (cleanup) { throw new AggregateError([error, cleanup]); }
        throw error;
    } finally {
        clearTimeout(timer); if (abortReady) budget.signal.removeEventListener('abort', abortReady);
        await writeFile(join(directory, 'vm-readiness.json'), JSON.stringify(readiness));
    }
}
