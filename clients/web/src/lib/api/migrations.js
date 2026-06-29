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
 * but WITHOUT ANY WARRANTY; without even the implied
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 */

import { buildApiUrl, get, post, del } from './core.js';

export async function createMigrationSource(data) {
    return post('/migrations', data);
}

export async function listMigrationSources(params = {}) {
    return get('/migrations', params);
}

export async function getMigrationSource(migrationSourceId) {
    return get(`/migrations/${migrationSourceId}`);
}

export async function deleteMigrationSource(migrationSourceId) {
    return del(`/migrations/${migrationSourceId}`);
}

export async function testMigrationConnection(migrationSourceId, data = {}) {
    return post(`/migrations/${migrationSourceId}/connect`, data);
}

export async function discoverMigrationSource(migrationSourceId, data = {}) {
    return post(`/migrations/${migrationSourceId}/discover`, data);
}

export async function matchMigrationItems(migrationSourceId) {
    return post(`/migrations/${migrationSourceId}/match`);
}

export async function uploadPlexMigrationDatabase(migrationSourceId, file) {
    const data = new FormData();
    data.append('file', file);
    return post(`/migrations/${migrationSourceId}/upload`, data);
}

export async function getMigrationUserMappingOptions(migrationSourceId) {
    return get(`/migrations/${migrationSourceId}/map-users`);
}

export async function saveMigrationUserMappings(migrationSourceId, data) {
    return post(`/migrations/${migrationSourceId}/map-users`, data);
}

export async function startMigration(migrationSourceId, data = {}) {
    return post(`/migrations/${migrationSourceId}/start`, data);
}

export async function runMigrationPreflight(migrationSourceId) {
    return post(`/migrations/${migrationSourceId}/preflight`);
}

export async function getMigrationProgress(migrationSourceId) {
    return get(`/migrations/${migrationSourceId}/progress`);
}

export async function getMigrationReviewItems(migrationSourceId, params = {}) {
    return get(`/migrations/${migrationSourceId}/review`, params);
}

export async function resolveMigrationReviewItem(migrationSourceId, itemId, data) {
    return post(`/migrations/${migrationSourceId}/review/${itemId}`, data);
}

export function getMigrationReviewCsvUrl(migrationSourceId, params = {}) {
    return buildApiUrl(`/migrations/${migrationSourceId}/review.csv`, params);
}

export async function getUnmatchedMigrationItems(migrationSourceId, params = {}) {
    return get(`/migrations/${migrationSourceId}/unmatched`, params);
}

export async function cancelMigration(migrationSourceId) {
    return post(`/migrations/${migrationSourceId}/cancel`);
}
