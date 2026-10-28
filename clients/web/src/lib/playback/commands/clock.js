// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { instant, valid } from './schema.js';

export function createServerClock({ now = () => performance.now(), wallNow = () => Date.now(), crypto = { getRandomValues: (bytes) => globalThis.crypto.getRandomValues(bytes) } } = {}) {
    let observation = null;
    function observe(value, began = now()) {
        observation = { time: instant(value), began, wall: wallNow() - (now() - began) };
    }
    function elapsed() {
        const value = observation ? now() - observation.began : Infinity;
        valid(Number.isFinite(value) && value >= 0);
        valid(Math.abs(wallNow() - observation.wall - value) <= 2000);
        return value;
    }
    function id() {
        valid(observation && elapsed() <= 30_000 && typeof crypto?.getRandomValues === 'function');
        const time = Math.floor(observation.time + elapsed());
        valid(time >= 0 && time < 2 ** 48);
        const bytes = crypto.getRandomValues(new Uint8Array(16));
        let timestamp = BigInt(time);
        for (let index = 5; index >= 0; index--) { bytes[index] = Number(timestamp & 255n); timestamp >>= 8n; }
        bytes[6] = 0x70 | bytes[6] & 15;
        bytes[8] = 0x80 | bytes[8] & 63;
        const hex = [...bytes].map((byte) => byte.toString(16).padStart(2, '0')).join('');
        return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
    }
    function remaining(deadline) { return observation ? instant(deadline) - observation.time - elapsed() : -1; }
    return { observe, id, remaining, stale: () => !observation || elapsed() > 30_000, invalidate: () => { observation = null; } };
}
