/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

export function createCatalogPaging() {
    let history = [null];
    let index = 0;
    let nextCursor = null;
    return {
        get index() { return index; },
        get canNext() { return Boolean(nextCursor) && index < 63; },
        get atLimit() { return Boolean(nextCursor) && index === 63; },
        cursor(action = 'current') {
            if (action === 'first') { history = [null]; index = 0; nextCursor = null; }
            if (action === 'previous' && index > 0) index -= 1;
            if (action === 'next' && this.canNext) { history = history.slice(0, index + 1); history.push(nextCursor); index += 1; }
            return history[index];
        },
        accept(cursor) {
            if (cursor && history.slice(0, index + 1).includes(cursor)) throw new Error('Repeated catalog cursor');
            nextCursor = cursor;
        },
    };
}
