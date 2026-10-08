// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026 Duskcue Contributors
//
// This program is free software: licensed under AGPL-3.0
// See LICENSE file for details.

import { win32 } from 'node:path';
import { hostedContext } from './qualification-hosted.mjs';

export function requireHostedWindowContext(environment, repository, manifest, platform = process.platform) {
    const context = hostedContext(environment, repository, platform);
    if (context.sourceCommit !== manifest.hostedCI?.context?.sourceCommit || context.runId !== manifest.hostedCI.context.runId || context.attempt !== manifest.hostedCI.context.attempt || context.job !== manifest.hostedCI.context.job) throw new Error('Owned window control requires this actual hosted native manifest/run.');
    return context;
}

export function assertOwnedNativeWindow(expected, actual, handles) {
    if (!Number.isSafeInteger(expected?.pid) || expected.pid <= 0 || !Number.isSafeInteger(expected.parentPid) || expected.parentPid <= 0
        || expected.name !== 'duskcue-tonight-qualification.exe' || !win32.isAbsolute(expected.path || '') || win32.basename(expected.path).toLowerCase() !== expected.name
        || !Number.isFinite(Date.parse(expected.createdAt)) || !/^[a-f0-9]{64}$/.test(expected.sha256 || '')
        || !actual || actual.pid !== expected.pid || actual.parentPid !== expected.parentPid || actual.name?.toLowerCase() !== expected.name
        || win32.normalize(actual.path || '').toLowerCase() !== win32.normalize(expected.path).toLowerCase() || actual.createdAt !== expected.createdAt || actual.sha256 !== expected.sha256) throw new Error('The native window process/executable/creation identity changed; no window action is permitted.');
    if (!Array.isArray(handles) || handles.length !== 1 || !/^[1-9][0-9]*$/.test(handles[0]) || expected.handle && expected.handle !== handles[0]) throw new Error('The exact owned native HWND is missing, ambiguous or changed.');
    return handles[0];
}
