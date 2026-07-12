/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 *
 * This program is free software: licensed under AGPL-3.0
 * See LICENSE file for details.
 */

export function cloneConfig(value) {
    return JSON.parse(JSON.stringify(value ?? {}));
}

export function getConfigPath(root, path) {
    return path.split('.').reduce((value, part) => value?.[part], root);
}

export function setConfigPath(root, path, value) {
    const parts = path.split('.');
    let target = root;
    for (let index = 0; index < parts.length - 1; index += 1) {
        const part = parts[index];
        if (!target[part] || typeof target[part] !== 'object' || Array.isArray(target[part])) {
            target[part] = {};
        }
        target = target[part];
    }
    target[parts[parts.length - 1]] = value;
}

export function hydrateConfigGroup(value, fields) {
    const next = cloneConfig(value && typeof value === 'object' && !Array.isArray(value) ? value : {});
    for (const field of fields) {
        const current = getConfigPath(next, field.path);
        if (field.type === 'list' && Array.isArray(current)) {
            setConfigPath(next, field.path, current.join(', '));
        } else if (field.type === 'json') {
            setConfigPath(next, field.path, JSON.stringify(current ?? {}, null, 2));
        }
    }
    return next;
}

export function serializeConfigGroup(value, fields) {
    const next = cloneConfig(value);
    for (const field of fields) {
        const current = getConfigPath(next, field.path);
        if (field.type === 'list') {
            setConfigPath(next, field.path, parseList(current));
        } else if (field.type === 'number') {
            setConfigPath(next, field.path, parseNumber(current, field));
        } else if (field.type === 'json') {
            setConfigPath(next, field.path, parseJson(current));
        } else if (field.type === 'text' || field.type === 'password') {
            setConfigPath(next, field.path, current === '' && field.nullable ? null : current);
        }
    }
    return next;
}

export function isConfigGroupDirty(value, original, fields) {
    return JSON.stringify(serializeConfigGroup(value, fields)) !== JSON.stringify(serializeConfigGroup(original, fields));
}

export function updateConfigField(value, field, nextValue) {
    const next = cloneConfig(value);
    setConfigPath(next, field.path, nextValue);
    return next;
}

function parseList(value) {
    if (Array.isArray(value)) return value;
    return String(value || '')
        .split(',')
        .map((item) => item.trim())
        .filter((item) => item.length > 0);
}

function parseNumber(value, field) {
    if ((value === '' || value === null || value === undefined) && field.nullable) return null;
    const parsed = Number(value);
    if (Number.isNaN(parsed)) return field.nullable ? null : 0;
    if (Number.isInteger(field.step)) return Math.trunc(parsed);
    return parsed;
}

function parseJson(value) {
    if (!value || String(value).trim() === '') return {};
    try {
        const parsed = JSON.parse(value);
        return parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? parsed : {};
    } catch {
        return {};
    }
}
