/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

export function instant(value) {
    if (typeof value !== 'string' || value.length > 64) throw new Error('Invalid instant');
    const parts = /^([+-]?\d{4,6})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.(\d{1,6}))?Z$/.exec(value);
    if (!parts) throw new Error('Invalid instant');
    const [year, month, day, hour, minute, second] = parts.slice(1, 7).map(Number);
    const date = new Date(0);
    date.setUTCFullYear(year, month - 1, day);
    date.setUTCHours(hour, minute, second, 0);
    if (!Number.isFinite(date.getTime()) || date.getUTCFullYear() !== year || date.getUTCMonth() !== month - 1
        || date.getUTCDate() !== day || date.getUTCHours() !== hour || date.getUTCMinutes() !== minute || date.getUTCSeconds() !== second) throw new Error('Invalid instant');
    const fraction = Number((parts[7] || '').padEnd(6, '0'));
    return { micros: BigInt(date.getTime()) * 1000n + BigInt(fraction), milliseconds: date.getTime() + Math.floor(fraction / 1000) };
}
