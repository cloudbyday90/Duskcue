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

import { get, put } from './core.js';

export async function getTvSurface(params = {}) {
    return get('/users/me/tv-surface', params);
}

export async function resolveTvContent(platformContentId) {
    return get(`/tv/resolve/${encodeURIComponent(platformContentId)}`);
}

export async function getTvSettings() {
    return get('/tv/settings');
}

export async function updateTvSettings(data) {
    return put('/tv/settings', data);
}

export async function getTvDiagnostics(params = {}) {
    return get('/tv/diagnostics', params);
}
