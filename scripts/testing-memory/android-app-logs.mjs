// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { join } from 'node:path';
import { runBounded } from '../qualification/commands.mjs';
import { redactAndroidOutput } from './android-evidence.mjs';
import { androidLogFrames } from './android-stage-budget.mjs';
import { androidFlutterMessage, androidVmUri } from './android-vm-identity.mjs';

export function ownAndroidAppLogs(owner, { pid, port, directory, fixture, budget, onPhase }, runner = runBounded) {
    const cancellation = new AbortController();
    const stopForBudget = () => cancellation.abort();
    budget.signal.addEventListener('abort', stopForBudget, { once: true });
    let stopping = false, uri, resolveUri, rejectUri;
    const phases = new Set();
    const ready = new Promise((resolve, reject) => { resolveUri = resolve; rejectUri = reject; });
    ready.catch(() => {});
    const frames = androidLogFrames((line) => {
        const message = androidFlutterMessage(line, pid);
        if (message === null) return;
        const found = androidVmUri(message, port);
        if (found) {
            if (uri) throw new Error('Duplicate Android VM-service announcement.');
            uri = found; resolveUri(found);
        }
        const phase = message.match(/^ANDROID_PHASE (background|renew-loss)$/)?.[1];
        if (phase) {
            if (phases.has(phase) || phase === 'background' && fixture.scenario !== 'baseline'
                || phase === 'renew-loss' && fixture.scenario !== 'lost-outcomes') throw new Error('Unexpected or repeated Android lifecycle phase.');
            phases.add(phase); onPhase(message);
        }
    });
    const completed = runner(owner.adb, ['-P', String(owner.adbPort), '-s', owner.serial, 'logcat',
        '-b', 'main', '-v', 'brief', `--pid=${pid}`, 'flutter:I', '*:S'], {
        env: owner.env, timeoutMs: budget.remaining(), outputBudget: budget, abortSignal: cancellation.signal,
        log: join(directory, 'app.log'), redact: [fixture.token, fixture.peer_token, fixture.other_token].filter(Boolean),
        sanitize: redactAndroidOutput, onChunk(chunk, channel) { if (channel === 'stdout') frames.write(chunk); },
    }).then(() => { throw new Error('Owned Android log producer exited unexpectedly.'); }).catch((error) => {
        if (stopping && error.message === 'Owned qualification command was cancelled by resource admission.') return;
        budget.fail(error); rejectUri(error); throw error;
    }).finally(() => budget.signal.removeEventListener('abort', stopForBudget));
    completed.catch(() => {});
    return {
        ready, phases, completed,
        async close() {
            stopping = true; cancellation.abort();
            await completed;
            frames.finish();
        },
    };
}
