// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import test from 'node:test';
import assert from 'node:assert/strict';
import { MetadataReadError, readLinuxSample } from './linux-engine-headroom.mjs';

const engine = { context: 'desktop-linux', engineId: 'owned-engine', kernel: '5.15.167.4-microsoft-standard-WSL2' };
const info = { id: engine.engineId, kernel: engine.kernel, osType: 'linux', name: 'docker-desktop', operatingSystem: 'Docker Desktop', architecture: 'x86_64' };
const memory = `MemTotal: 8388608 kB\nMemAvailable: 6291456 kB\naaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee\nLinux version ${engine.kernel} owned-fixture\n`;

test('metadata read diagnostics retain bounded termination context without raw process material', () => {
    const error = new MetadataReadError('wsl.exe', { code: 'ETIMEDOUT', signal: 'SIGTERM', killed: true, elapsedMs: 5000.7, stderr: 'private stderr', message: 'private message', args: ['private argument'], env: { PRIVATE: 'private environment' } });
    assert.deepEqual(error.context, { code: 'ETIMEDOUT', signal: 'SIGTERM', killed: true, elapsedMs: 5001 });
    assert.match(error.message, /Read-only wsl\.exe metadata was unavailable/);
    assert.match(error.message, /"killed":true/);
    assert.equal(error.message.includes('private'), false);
    const unsafe = new MetadataReadError('/private/path/tool.exe', { code: 'private\n'.repeat(100), signal: 'private signal', elapsedMs: Number.POSITIVE_INFINITY });
    assert.deepEqual(unsafe.context, { code: null, signal: null, killed: false, elapsedMs: null });
    assert.equal(unsafe.message.includes('private'), false);
    assert.ok(unsafe.message.length < 256);
    for (const code of [-2147483648, 1, 4294967295]) assert.equal(new MetadataReadError('wsl.exe', { code }).context.code, code);
    for (const code of [-2147483649, 4294967296, 1.5]) assert.equal(new MetadataReadError('wsl.exe', { code }).context.code, null);
});

test('running-distribution and engine reads overlap and both validate before reading memory', async () => {
    const running = Promise.withResolvers();
    const current = Promise.withResolvers();
    const calls = [];
    const read = (executable, args) => {
        calls.push([executable, args]);
        if (args[0] === '--list') return running.promise;
        if (executable === 'docker.exe') return current.promise;
        assert.equal(calls.length, 3);
        return Promise.resolve(memory);
    };
    const pending = readLinuxSample(engine, read);
    assert.equal(calls.length, 2);
    current.resolve(JSON.stringify(info));
    await Promise.resolve();
    assert.equal(calls.length, 2);
    running.resolve('docker-desktop\n');
    const sample = await pending;
    assert.equal(sample.engineId, engine.engineId);
    assert.equal(sample.availableBytes, 6291456 * 1024);
    assert.deepEqual(calls[2][1].slice(0, 4), ['--distribution', 'docker-desktop', '--exec', 'cat']);
});

test('parallel transport failure cannot mask observed engine or JSON identity failures', async () => {
    for (const current of [JSON.stringify({ ...info, id: 'foreign-engine' }), '{malformed']) {
        let memoryReads = 0;
        const read = async (executable, args) => {
            if (args[0] === '--list') throw new MetadataReadError('wsl.exe', { code: 'ETIMEDOUT', killed: true });
            if (executable === 'docker.exe') return current;
            memoryReads += 1;
            return memory;
        };
        await assert.rejects(readLinuxSample(engine, read), (error) => !(error instanceof MetadataReadError));
        assert.equal(memoryReads, 0);
    }
});
