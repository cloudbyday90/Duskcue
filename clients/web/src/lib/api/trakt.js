/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 */

import { del, get, post, put } from './core.js';

export async function getTraktAccount() {
    return get('/trakt/account');
}

export async function startTraktLink() {
    return post('/trakt/account/link');
}

export async function pollTraktLink(deviceCode) {
    return post('/trakt/account/poll', { device_code: deviceCode });
}

export async function unlinkTraktAccount() {
    return del('/trakt/account');
}

export async function getTraktSyncSettings() {
    return get('/trakt/settings');
}

export async function updateTraktSyncSettings(settings) {
    return put('/trakt/settings', settings);
}

export async function triggerTraktSync() {
    return post('/trakt/sync');
}

export async function getTraktSyncStatus() {
    return get('/trakt/sync/status');
}

export async function listTraktHistory(params = {}) {
    return get('/trakt/history', params);
}

export async function listTraktRatings(params = {}) {
    return get('/trakt/ratings', params);
}

export async function getTraktIntegrationSettings() {
    return get('/settings/trakt');
}

export async function updateTraktIntegrationSettings(settings) {
    return put('/settings/trakt', settings);
}
