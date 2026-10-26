/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

const days = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
const longDays = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
const months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

function dateParts(value, now) {
    let parts = /^([A-Z][a-z]{2}), (\d{2}) ([A-Z][a-z]{2}) (\d{4}) (\d{2}):(\d{2}):(\d{2}) GMT$/.exec(value);
    if (parts) return { weekday: parts[1], day: Number(parts[2]), month: months.indexOf(parts[3]), year: Number(parts[4]), hour: Number(parts[5]), minute: Number(parts[6]), second: Number(parts[7]), oldYear: false };
    parts = /^([A-Z][a-z]+), (\d{2})-([A-Z][a-z]{2})-(\d{2}) (\d{2}):(\d{2}):(\d{2}) GMT$/.exec(value);
    if (parts && longDays.includes(parts[1])) return { weekday: parts[1].slice(0, 3), day: Number(parts[2]), month: months.indexOf(parts[3]), year: Math.floor((new Date(now).getUTCFullYear() + 50) / 100) * 100 + Number(parts[4]), hour: Number(parts[5]), minute: Number(parts[6]), second: Number(parts[7]), oldYear: true };
    parts = /^([A-Z][a-z]{2}) ([A-Z][a-z]{2}) ( \d|\d{2}) (\d{2}):(\d{2}):(\d{2}) (\d{4})$/.exec(value);
    if (parts) return { weekday: parts[1], day: Number(parts[3]), month: months.indexOf(parts[2]), year: Number(parts[7]), hour: Number(parts[4]), minute: Number(parts[5]), second: Number(parts[6]), oldYear: false };
    return null;
}

function httpDate(value, now) {
    const parts = dateParts(value, now);
    if (!parts || parts.month < 0 || parts.year < 1900 || parts.hour > 23 || parts.minute > 59 || parts.second > 60) return null;
    const { day, month, hour, minute, second } = parts;
    const date = new Date(0);
    date.setUTCFullYear(parts.year, month, day);
    date.setUTCHours(hour, minute, Math.min(second, 59), 0);
    if (parts.oldYear) {
        const threshold = new Date(now);
        threshold.setUTCFullYear(threshold.getUTCFullYear() + 50);
        if (date.getTime() > threshold.getTime()) { parts.year -= 100; date.setUTCFullYear(parts.year, month, day); }
    }
    if (date.getUTCFullYear() !== parts.year || date.getUTCMonth() !== month || date.getUTCDate() !== day || days[date.getUTCDay()] !== parts.weekday) return null;
    return date.getTime() + (second === 60 ? 1000 : 0);
}

export function retryAfterSeconds(value, now = Date.now()) {
    if (typeof value !== 'string' || !value || value.length > 128) return null;
    if (/^\d+$/.test(value)) {
        const seconds = Number(value);
        return Number.isSafeInteger(seconds) ? seconds : null;
    }
    const instant = httpDate(value, now);
    return instant === null ? null : Math.max(0, Math.ceil((instant - now) / 1000));
}
