// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { createInterface } from 'node:readline';
import { readFile, writeFile, rename } from 'node:fs/promises';

const resourceId = process.env.DUSKCUE_TEST_RESOURCE_ID;
if (!/^[a-f0-9-]{36}$/.test(resourceId || '') || process.env.DUSKCUE_TEST_DIRECT_BROWSER !== 'live') throw new Error('An owned direct-play fixture is required.');
let active = 0;
let completed = false;
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
    if (message.operation !== 'fetch' || !['GET', 'POST'].includes(message.method)
        || !/^\/api\/v1\/[A-Za-z0-9/_-]+(?:\.m3u8|\.m4s|\.mp4)?$/.test(message.path || '')
        || typeof message.body !== 'string' || message.body.length > 65536) throw new Error('Invalid fixture HTTP request.');
    const headers = {};
    for (const [key, value] of Object.entries(message.headers || {})) {
        if (!['cookie', 'content-type', 'range', 'authorization'].includes(key) || typeof value !== 'string' || value.length > 4096 || /[\r\n]/.test(value)) throw new Error('Invalid fixture header.');
        headers[key] = value;
    }
    const response = await fetch(`http://127.0.0.1:38081${message.path}`, {
        method: message.method, headers, ...(message.method === 'POST' ? { body: message.body } : {}),
        signal: AbortSignal.timeout(30000), redirect: 'error',
    });
    let bytes = 0;
    const chunks = [];
    for await (const chunk of response.body || []) {
        bytes += chunk.length;
        if (bytes > 8388608) throw new Error('Fixture HTTP response exceeded its bound.');
        chunks.push(Buffer.from(chunk));
    }
    return { status: response.status, headers: Object.fromEntries([...response.headers].filter(([key]) => ['content-type', 'content-range', 'accept-ranges', 'cache-control'].includes(key))), body: Buffer.concat(chunks).toString('base64') };
}
const lines = createInterface({ input: process.stdin, crlfDelay: Infinity });
lines.on('line', (line) => {
    if (line.length > 131072 || active >= 8) { emit({ id: 0, error: 'Bridge request bound exceeded.' }); return; }
    active += 1;
    let message;
    try { message = JSON.parse(line); } catch { emit({ id: 0, error: 'Invalid bridge JSON.' }); active -= 1; return; }
    void handle(message).then((result) => emit({ id: message.id, ...result }), () => emit({ id: message.id, error: 'Fixture HTTP bridge request failed.' })).finally(() => { active -= 1; });
});
