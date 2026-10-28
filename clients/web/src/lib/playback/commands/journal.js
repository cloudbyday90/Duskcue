// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { uuid, entityId, valid } from './schema.js';

const KEY = 'duskcue_vod_command_v2';
const fields = ['version', 'scope', 'start', 'operation', 'output', 'phase'];
const scopeFields = ['origin', 'user', 'session', 'credential', 'transport', 'profile', 'revision', 'context'];
function record(value) {
    valid(value && Object.keys(value).length === fields.length && Object.keys(value).every((key) => fields.includes(key))
        && value.version === 1 && uuid(value.start) && (value.operation === null || uuid(value.operation))
        && (value.output === null || uuid(value.output)) && ['active', 'uncertain', 'cleanup'].includes(value.phase));
    const scope = value.scope;
    valid(scope && Object.keys(scope).length === scopeFields.length && Object.keys(scope).every((key) => scopeFields.includes(key))
        && typeof scope.origin === 'string' && scope.origin.length <= 2048 && new URL(scope.origin).origin === scope.origin
        && ['user', 'session', 'profile'].every((key) => entityId(scope[key]))
        && ['credential', 'revision'].every((key) => typeof scope[key] === 'string' && /^[1-9]\d{0,18}$/.test(scope[key]))
        && ['cookie', 'bearer'].includes(scope.transport) && typeof scope.context === 'string' && /^sc1\.[A-Za-z0-9_-]{43}$/.test(scope.context));
    return value;
}
export function createJournal({ storage = () => globalThis.sessionStorage } = {}) {
    function read() {
        const raw = storage()?.getItem(KEY);
        if (raw === null || raw === undefined) return null;
        valid(new TextEncoder().encode(raw).byteLength <= 4096);
        return record(JSON.parse(raw));
    }
    function write(value) {
        const raw = JSON.stringify(record(value));
        valid(new TextEncoder().encode(raw).byteLength <= 4096);
        const target = storage();
        valid(target);
        target.setItem(KEY, raw);
        valid(target.getItem(KEY) === raw);
    }
    return { read, write, clear: () => storage()?.removeItem(KEY) };
}
