// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { constants } from 'node:fs';
import { lstat, open, rename, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

const uuid = /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/i;
const fields = ['resourceId', 'sessionId', 'transcodeId', 'positionMs', 'paused'];

export function createNativeStallControl(resourceId, { enabled = false, directory = '/tmp' } = {}) {
    if (!uuid.test(resourceId || '')) throw new Error('Owned native control identity is invalid.');
    let requested = false;
    const pending = join(directory, 'direct-native-stall.pending');
    const request = join(directory, 'direct-native-stall.json');
    const acknowledgement = join(directory, 'direct-native-stall-armed.json');
    return async (report) => {
        if (!enabled || requested || !report || Object.keys(report).some((key) => !fields.includes(key))
            || report.resourceId !== resourceId || !uuid.test(report.sessionId || '') || !uuid.test(report.transcodeId || '')
            || report.positionMs !== 4000 || report.paused !== true) throw new Error('Native stall request is outside its owned scope.');
        requested = true;
        for (const path of [pending, request, acknowledgement]) {
            try { await lstat(path); throw new Error('Native stall control has stale evidence.'); }
            catch (error) { if (error.code !== 'ENOENT') throw error; }
        }
        await writeFile(pending, JSON.stringify(report), { flag: 'wx', mode: 0o600 });
        await rename(pending, request);
        const deadline = Date.now() + 5000;
        while (Date.now() < deadline) {
            let handle;
            try {
                const entry = await lstat(acknowledgement);
                if (!entry.isFile() || entry.isSymbolicLink() || entry.size > 65536) throw new Error('Native stall acknowledgement is not bounded regular evidence.');
                handle = await open(acknowledgement, constants.O_RDONLY | (constants.O_NOFOLLOW || 0));
                const actual = await handle.stat();
                if (!actual.isFile() || actual.size > 65536) throw new Error('Native stall acknowledgement changed identity.');
                const buffer = Buffer.alloc(65537);
                const { bytesRead } = await handle.read(buffer, 0, buffer.length, 0);
                if (bytesRead > 65536) throw new Error('Native stall acknowledgement exceeded its bound.');
                const value = JSON.parse(buffer.subarray(0, bytesRead).toString('utf8'));
                const identity = value.encoderIdentity;
                if (fields.some((key) => value[key] !== report[key]) || value.armed !== true
                    || !['pid', 'parent_pid', 'started_ticks'].every((key) => Number.isSafeInteger(identity?.[key]) && identity[key] > 0)) throw new Error('Native stall acknowledgement does not match the owned worker.');
                return { ...report, armed: true, encoderIdentity: Object.fromEntries(['pid', 'parent_pid', 'started_ticks'].map((key) => [key, identity[key]])) };
            } catch (error) { if (error.code !== 'ENOENT') throw error; }
            finally { await handle?.close(); }
            await new Promise((resolve) => setTimeout(resolve, Math.min(50, Math.max(0, deadline - Date.now()))));
        }
        throw new Error('Native stall acknowledgement exceeded its five-second deadline.');
    };
}
