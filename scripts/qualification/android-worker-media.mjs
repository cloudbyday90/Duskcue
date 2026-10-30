// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { lstat, realpath } from 'node:fs/promises';
import { join, relative, isAbsolute } from 'node:path';
import { runBounded } from './commands.mjs';
import { hashFile } from './source.mjs';

export async function createAndroidWorkerMedia(root, reference, ffmpeg, ffprobe) {
    root = await realpath(root);
    const scoped = relative(root, await realpath(reference));
    if (!scoped || scoped.startsWith('..') || isAbsolute(scoped) || !(await lstat(reference)).isFile()) throw new Error('Worker reference must be an owned regular fixture.');
    const path = join(root, 'worker-source.mkv');
    const made = await runBounded(ffmpeg, ['-nostdin', '-hide_banner', '-loglevel', 'error', '-n', '-stream_loop', '4', '-i', reference,
        '-t', '40', '-map', '0', '-c', 'copy', path], { cwd: root, timeoutMs: 60000, bytes: 1048576 });
    if (made.code !== 0 || made.signal) throw new Error('Bounded worker source authoring failed.');
    const info = await lstat(path);
    if (!info.isFile() || info.isSymbolicLink() || info.size < 1 || info.size > 16777216) throw new Error('Worker source exceeded its unchanged byte budget.');
    const measured = await runBounded(ffprobe, ['-v', 'error', '-show_entries', 'format=duration', '-of', 'json', path], { cwd: root, timeoutMs: 15000, bytes: 32768 });
    if (measured.code !== 0 || measured.signal) throw new Error('Worker source duration probe failed.');
    const seconds = Number(JSON.parse(measured.stdout).format?.duration);
    if (!Number.isFinite(seconds) || seconds < 39.5 || seconds > 41) throw new Error('Actual worker source duration differs from its bounded intent.');
    return { path, seconds, bytes: info.size, sha256: await hashFile(path) };
}
