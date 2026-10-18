// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { PassThrough, Writable } from 'node:stream';
import test from 'node:test';
import { createDirectBridgeSession } from './direct-bridge-session.mjs';

function fixture(options) {
    const child = new EventEmitter();
    const messages = [];
    child.stdout = new PassThrough();
    child.stderr = new PassThrough();
    child.stdin = new Writable({ write(chunk, _encoding, callback) { messages.push(JSON.parse(chunk.toString())); callback(); } });
    child.stdin.once('finish', () => queueMicrotask(() => child.emit('close', 0)));
    child.kill = () => { queueMicrotask(() => child.emit('close', 0)); return true; };
    const session = createDirectBridgeSession(child, options);
    const reply = (message, value) => child.stdout.write(`${JSON.stringify({ id: message.id, ...value })}\n`);
    return { child, messages, session, reply };
}

test('saturated data slots retain independent idempotent stream cancellation and pending responses', async () => {
    const { messages, session, reply } = fixture();
    try {
        const stream = session.stream({ path: '/api/v1/events', method: 'GET' }, () => {});
        const requests = Array.from({ length: 7 }, () => session.request({ operation: 'fetch' }));
        assert.equal(messages.length, 8);
        await assert.rejects(session.request({ operation: 'fetch' }), /unavailable/);
        let canceled;
        assert.doesNotThrow(() => { canceled = stream.cancel(); });
        assert.equal(stream.cancel(), canceled);
        await Promise.resolve();
        assert.equal(messages.length, 9);
        const control = messages.at(-1);
        assert.equal(control.operation, 'cancel');
        assert.equal(control.target, messages[0].id);
        reply(control, { cancelled: true });
        reply(messages[0], { type: 'end' });
        for (const message of messages.slice(1, 8)) reply(message, { status: 200, body: '' });
        assert.equal((await canceled).cancelled, true);
        assert.equal((await stream.done).type, 'end');
        assert.equal((await Promise.all(requests)).length, 7);
        assert.equal(stream.cancel(), canceled);
    } finally { await session.close(); }
});

test('two stream control slots remain available when all eight data slots are occupied', async () => {
    const { messages, session, reply } = fixture();
    try {
        const first = session.stream({}, () => {});
        const second = session.stream({}, () => {});
        assert.throws(() => session.stream({}, () => {}), /unavailable/);
        const requests = Array.from({ length: 6 }, () => session.request({ operation: 'fetch' }));
        assert.throws(() => session.stream({}, () => {}), /unavailable/);
        const canceled = [first.cancel(), second.cancel()];
        await Promise.resolve();
        assert.equal(messages.length, 10);
        await assert.rejects(session.request({ operation: 'cancel', target: messages[0].id }), /unavailable/);
        for (const control of messages.slice(8)) reply(control, { cancelled: true });
        for (const stream of messages.slice(0, 2)) reply(stream, { type: 'end' });
        for (const message of messages.slice(2, 8)) reply(message, { status: 200, body: '' });
        await Promise.all([...canceled, first.done, second.done, ...requests]);
    } finally { await session.close(); }
});

test('aborted cancellation returns a rejected promise and never throws synchronously', async () => {
    const controller = new AbortController();
    const { session } = fixture({ abortSignal: controller.signal });
    const stream = session.stream({}, () => {});
    const stopped = assert.rejects(stream.done, /terminated/);
    controller.abort();
    let canceled;
    assert.doesNotThrow(() => { canceled = stream.cancel(); });
    await assert.rejects(canceled, /unavailable/);
    assert.equal(stream.cancel(), canceled);
    await stopped;
    await session.close();
});
