/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { instant } from '../time/instant.js';
export { instant } from '../time/instant.js';

export function sixHourWindow(now = Date.now()) {
    return { from: new Date(now).toISOString(), to: new Date(now + 6 * 60 * 60 * 1000).toISOString() };
}

export function moveWindow(window, direction) {
    return sixHourWindow(instant(window.from).milliseconds + direction * 6 * 60 * 60 * 1000);
}
