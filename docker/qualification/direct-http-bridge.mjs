// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { createInterface } from 'node:readline';
import { readFile, writeFile, rename } from 'node:fs/promises';
import { fixtureResponseHeaders, validateFixtureRequest } from './direct-http-policy.mjs';
import { createNativeStallControl } from './direct-native-control.mjs';
import { createFormatControl } from './direct-format-control.mjs';

const resourceId = process.env.DUSKCUE_TEST_RESOURCE_ID;
if (!/^[a-f0-9-]{36}$/.test(resourceId || '') || process.env.DUSKCUE_TEST_DIRECT_BROWSER !== 'live') throw new Error('An owned direct-play fixture is required.');
let active = 0;
let controls = 0;
let completed = false;
const streams = new Map();
const verified = new Set();
const nativeRecovery = process.env.DUSKCUE_TEST_NATIVE_RECOVERY === 'live';
const armNativeStall = createNativeStallControl(resourceId, { enabled: nativeRecovery });
const verifyFormat = createFormatControl(resourceId, { enabled: nativeRecovery && process.env.DUSKCUE_TEST_FORMAT_CORPUS === 'live' });
const emit = (value) => process.stdout.write(`${JSON.stringify(value)}\n`);
async function handle(message) {
    if (!Number.isSafeInteger(message.id) || message.id < 1) throw new Error('Invalid bridge request identity.');
    if (message.operation === 'ready') {
        try { return { ready: JSON.parse(await readFile('/tmp/direct-ready.json', 'utf8')) }; }
        catch (error) { if (error.code === 'ENOENT') return { ready: null }; throw error; }
    }
    if (message.operation === 'complete') {
        if (completed || message.report?.resourceId !== resourceId || JSON.stringify(message.report).length > 65536) throw new Error('Invalid fixture completion.');
        completed = true;
        await writeFile('/tmp/direct-complete.pending', JSON.stringify(message.report), { flag: 'wx', mode: 0o600 });
        await rename('/tmp/direct-complete.pending', '/tmp/direct-complete.json');
        return { completed: true };
    }
    if (message.operation === 'verify') {
        const phase = message.report?.phase;
        if (!(nativeRecovery ? ['browser', 'native-recovery', 'native'] : ['browser', 'native']).includes(phase) || verified.has(phase) || message.report.resourceId !== resourceId || Buffer.byteLength(JSON.stringify(message.report)) > 65536) throw new Error('Invalid live verification.');
        verified.add(phase);
        await writeFile(`/tmp/direct-observed-${phase}.pending`, JSON.stringify(message.report), { flag: 'wx', mode: 0o600 });
        await rename(`/tmp/direct-observed-${phase}.pending`, `/tmp/direct-observed-${phase}.json`);
        const deadline = Date.now() + 30000;
        while (Date.now() < deadline) {
            try {
                const bytes = await readFile(`/tmp/direct-verified-${phase}.json`);
                if (bytes.length > 65536) throw new Error('Verification response exceeded its bound.');
                return { verified: JSON.parse(bytes) };
            } catch (error) { if (error.code !== 'ENOENT') throw error; }
            await new Promise((resolve) => setTimeout(resolve, 100));
        }
        throw new Error('Live verification exceeded its deadline.');
    }
    if (message.operation === 'arm-native-stall') return { armed: await armNativeStall(message.report) };
    if (message.operation === 'verify-format') return verifyFormat(message.report);
    if (message.operation === 'cancel') {
        if (!Number.isSafeInteger(message.target) || message.target < 1) throw new Error('Invalid event stream.');
        streams.get(message.target)?.abort();
        return { cancelled: true };
    }
    if (!['fetch', 'stream'].includes(message.operation)) throw new Error('Invalid fixture operation.');
    const request = validateFixtureRequest(message);
    const stream = message.operation === 'stream';
    if (stream && (request.path !== '/api/v1/events' || request.method !== 'GET' || streams.size >= 2)) throw new Error('Invalid event stream.');
    const controller = new AbortController();
    if (stream) streams.set(message.id, controller);
    const response = await fetch(`http://127.0.0.1:38081${request.path}`, {
        ...request, signal: AbortSignal.any([controller.signal, AbortSignal.timeout(stream ? 120000 : 30000)]), redirect: 'error',
    });
    const headers = fixtureResponseHeaders(response.headers);
    if (stream) emit({ id: message.id, type: 'headers', status: response.status, headers });
    let bytes = 0;
    const chunks = [];
    for await (const chunk of response.body || []) {
        bytes += chunk.length;
        if (bytes > (stream ? 1048576 : 8388608)) throw new Error('Fixture HTTP response exceeded its bound.');
        if (stream) {
            for (let offset = 0; offset < chunk.length; offset += 16384) emit({ id: message.id, type: 'chunk', body: Buffer.from(chunk.subarray(offset, offset + 16384)).toString('base64') });
        } else chunks.push(Buffer.from(chunk));
    }
    return stream ? { type: 'end' } : { status: response.status, headers, body: Buffer.concat(chunks).toString('base64') };
}
const lines = createInterface({ input: process.stdin, crlfDelay: Infinity });
lines.on('line', (line) => {
    if (line.length > 131072) { emit({ id: 0, error: 'Bridge request bound exceeded.' }); return; }
    let message;
    try { message = JSON.parse(line); } catch { emit({ id: 0, error: 'Invalid bridge JSON.' }); return; }
    if (!message || typeof message !== 'object' || Array.isArray(message) || !Number.isSafeInteger(message.id) || message.id < 1) { emit({ id: 0, error: 'Invalid bridge request identity.' }); return; }
    const control = message.operation === 'cancel';
    if (control ? controls >= 2 : active >= 8) { emit({ id: message.id, error: 'Bridge request bound exceeded.' }); return; }
    if (control) controls += 1; else active += 1;
    void handle(message).then((result) => emit({ id: message.id, ...result }), () => {
        if (message.operation === 'stream' && streams.get(message.id)?.signal.aborted) emit({ id: message.id, type: 'end' });
        else emit({ id: message.id, error: 'Fixture HTTP bridge request failed.' });
    }).finally(() => { streams.get(message.id)?.abort(); streams.delete(message.id); if (control) controls -= 1; else active -= 1; });
});
lines.on('close', () => { for (const controller of streams.values()) controller.abort(); });
