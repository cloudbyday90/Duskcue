// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { access, lstat, open, realpath, stat } from 'node:fs/promises';
import { constants } from 'node:fs';
import { delimiter, isAbsolute, join, resolve } from 'node:path';

export async function boundedMedia(program, args, { timeoutMs = 30000, outputLimit = 1048576 } = {}) {
    if (!Number.isInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 120000 || !Number.isInteger(outputLimit) || outputLimit < 1 || outputLimit > 2097152) throw new Error('Invalid format process bounds.');
    const child = spawn(program, args, { stdio: ['ignore', 'pipe', 'pipe'], detached: process.platform === 'linux', windowsHide: true });
    const chunks = [[], []];
    let total = 0;
    let failure;
    let force;
    const signal = (kind) => {
        if (!child.pid) return;
        try { if (process.platform === 'linux') process.kill(-child.pid, kind); else child.kill(kind); }
        catch (error) { if (error.code !== 'ESRCH') failure ||= 'Format process termination failed.'; }
    };
    const stop = (message) => {
        failure ||= message;
        signal('SIGTERM');
        force ||= setTimeout(() => signal('SIGKILL'), 1000);
    };
    const closed = new Promise((done) => child.once('close', (code, signal) => done({ code, signal })));
    child.once('error', () => { failure ||= 'Format process could not start.'; });
    for (const [index, stream] of [child.stdout, child.stderr].entries()) {
        stream.on('error', () => stop('Format process output could not be read.'));
        stream.on('data', (bytes) => {
            total += bytes.length;
            if (total > outputLimit) stop('Format process exceeded its output bound.');
            else chunks[index].push(bytes);
        });
    }
    const timer = setTimeout(() => stop('Format process exceeded its deadline.'), timeoutMs);
    const interrupted = () => stop('Format process was interrupted.');
    process.on('SIGTERM', interrupted);
    process.on('SIGINT', interrupted);
    try {
        const outcome = await closed;
        if (failure) throw new Error(failure);
        if (outcome.code !== 0 || outcome.signal) throw new Error(`Format process failed (code ${outcome.code}, signal ${outcome.signal || 'none'}): ${Buffer.concat(chunks[1]).toString('utf8').slice(-4096)}`);
        return Buffer.concat(chunks[0]);
    } finally {
        clearTimeout(timer);
        clearTimeout(force);
        process.off('SIGTERM', interrupted);
        process.off('SIGINT', interrupted);
        if (child.pid && child.exitCode === null && child.signalCode === null) { signal('SIGKILL'); await closed; }
    }
}

export async function fileIdentity(path, limit) {
    const before = await lstat(path);
    if (!before.isFile() || before.isSymbolicLink() || before.size < 1 || before.size > limit) throw new Error('Format file exceeded its regular-file bound.');
    const file = await open(path, constants.O_RDONLY | (constants.O_NOFOLLOW || 0));
    const matches = (metadata) => metadata.isFile() && !metadata.isSymbolicLink() && before.dev === metadata.dev && before.ino === metadata.ino && before.size === metadata.size && before.mtimeMs === metadata.mtimeMs && before.ctimeMs === metadata.ctimeMs;
    try {
        if (!matches(await file.stat())) throw new Error('Format file changed before hashing.');
        const hash = createHash('sha256');
        let bytes = 0;
        const buffer = Buffer.alloc(32768);
        while (true) {
            const read = await file.read(buffer, 0, Math.min(buffer.length, limit - bytes + 1));
            if (!read.bytesRead) break;
            bytes += read.bytesRead;
            if (bytes > limit) throw new Error('Format file grew past its bound.');
            hash.update(buffer.subarray(0, read.bytesRead));
        }
        if (bytes !== before.size || !matches(await file.stat()) || !matches(await lstat(path))) throw new Error('Format file changed while hashing.');
        return { bytes, sha256: hash.digest('hex') };
    } finally { await file.close(); }
}

export async function toolIdentity(program) {
    const candidates = isAbsolute(program) || program.includes('/') || program.includes('\\') ? [resolve(program)] : (process.env.PATH || '').split(delimiter).map((directory) => join(directory, program));
    let path;
    for (const candidate of candidates) {
        try { await access(candidate, constants.X_OK); if ((await stat(candidate)).isFile()) { path = await realpath(candidate); break; } }
        catch {}
    }
    if (!path) throw new Error('Format tool is unavailable.');
    const identity = await fileIdentity(path, 67108864);
    const version = (await boundedMedia(path, ['-version'])).toString('utf8').trim();
    return { path, ...identity, version };
}

export const mediaPrelude = ['-hide_banner', '-loglevel', 'error', '-nostdin', '-threads', '1', '-filter_threads', '1', '-filter_complex_threads', '1'];
