// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { spawn } from 'node:child_process';
import { createWriteStream } from 'node:fs';
import { mkdir, writeFile } from 'node:fs/promises';
import { dirname } from 'node:path';
import { createInterface } from 'node:readline';

export async function runBounded(executable, args, { cwd, env = process.env, log, bytes = 16777216, timeoutMs = 300000, onLine, redact = [], forward = false, captureBytes = 65536 } = {}) {
    if (log) await mkdir(dirname(log), { recursive: true });
    const buffered = [];
    const writer = log && !redact.length ? createWriteStream(log, { flags: 'wx' }) : null;
    const written = writer ? new Promise((resolve, reject) => { writer.once('finish', resolve); writer.once('error', reject); }) : null;
    written?.catch(() => {});
    const child = spawn(executable, args, { cwd, env, stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true, detached: process.platform === 'linux' });
    let total = 0;
    let stdout = '';
    let stderr = '';
    let failure;
    let force;
    const paused = new Set();
    const signal = (kind) => {
        if (!child.pid) return;
        try {
            if (process.platform === 'linux') process.kill(-child.pid, kind);
            else if (child.exitCode === null && child.signalCode === null) child.kill(kind);
        } catch (error) { if (error.code !== 'ESRCH') failure ||= 'Owned process-group termination could not be requested.'; }
    };
    const stop = (reason) => {
        failure ||= reason;
        signal('SIGTERM');
        for (const stream of paused) stream.resume();
        paused.clear();
        force ||= setTimeout(() => signal('SIGKILL'), 1000);
    };
    const outcome = new Promise((resolve, reject) => {
        child.once('error', reject);
        child.once('close', (code, signal) => resolve({ code, signal }));
    });
    outcome.catch(() => {});
    const timer = setTimeout(() => stop('Owned qualification command exceeded its deadline.'), timeoutMs);
    const interrupt = () => stop('Owned qualification command was interrupted.');
    process.on('SIGINT', interrupt);
    process.on('SIGTERM', interrupt);
    const lines = onLine ? createInterface({ input: child.stdout }) : null;
    lines?.on('line', (line) => { try { onLine(line); } catch (error) { stop(error.message); } });
    writer?.on('error', () => stop('Qualification evidence could not be written.'));
    const sinkError = () => stop('Qualification output could not be forwarded.');
    if (forward) { process.stdout.on('error', sinkError); process.stderr.on('error', sinkError); }
    for (const [stream, channel] of [[child.stdout, 'stdout'], [child.stderr, 'stderr']]) {
        stream.on('data', (chunk) => {
            total += chunk.length;
            if (total > bytes) { stop('Owned qualification command exceeded its log bound.'); return; }
            const drains = [];
            if (redact.length) buffered.push(Buffer.from(chunk));
            else if (writer && !writer.write(chunk)) drains.push(writer);
            if (forward && !redact.length) {
                const target = channel === 'stdout' ? process.stdout : process.stderr;
                if (!target.write(chunk)) drains.push(target);
            }
            if (drains.length && !failure) {
                stream.pause();
                paused.add(stream);
                let remaining = drains.length;
                for (const drain of drains) drain.once('drain', () => {
                    remaining -= 1;
                    if (!remaining) { paused.delete(stream); stream.resume(); }
                });
            }
            if (channel === 'stdout') stdout = (stdout + chunk.toString('utf8')).slice(-captureBytes);
            else stderr = (stderr + chunk.toString('utf8')).slice(-captureBytes);
        });
    }
    const clean = (text) => {
        for (const value of redact) text = text.split(value).join('[redacted]');
        return text.replace(/postgres(?:ql)?:\/\/[^\s'"<>]+/g, '[redacted-database-url]');
    };
    try {
        const result = await outcome;
        if (writer) { writer.end(); await written; }
        if (log && redact.length) await writeFile(log, clean(Buffer.concat(buffered).toString('utf8')), { flag: 'wx' });
        if (failure) throw new Error(failure);
        return { ...result, stdout: clean(stdout), stderr: clean(stderr), bytes: total };
    } finally {
        clearTimeout(timer);
        clearTimeout(force);
        lines?.close();
        process.off('SIGINT', interrupt);
        process.off('SIGTERM', interrupt);
        if (forward) { process.stdout.off('error', sinkError); process.stderr.off('error', sinkError); }
        if (failure || child.pid && child.exitCode === null && child.signalCode === null) { signal('SIGKILL'); await outcome; }
        writer?.destroy();
    }
}
