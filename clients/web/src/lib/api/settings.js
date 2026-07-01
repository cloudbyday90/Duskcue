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

import { get, post, put, rootGet } from './core.js';

export async function validateProviderKey(data) {
    return post('/settings/providers/validate', data);
}

export async function getHealth() {
    return rootGet('/health/ready');
}

export async function getServerConfig() {
    return get('/server/config');
}

export async function getConfigGroup(group) {
    return get(`/server/config/${group}`);
}

export async function updateConfigGroup(group, value) {
    return put(`/server/config/${group}`, { value });
}

export async function listScheduledTasks() {
    return get('/scheduled-tasks');
}

export async function getScheduledTask(taskId) {
    return get(`/scheduled-tasks/${taskId}`);
}

export async function triggerScheduledTask(taskId) {
    return post(`/scheduled-tasks/${taskId}/trigger`);
}

export async function cancelScheduledTask(taskId) {
    return post(`/scheduled-tasks/${taskId}/cancel`);
}

export async function listScheduledTaskRuns(taskId, params = {}) {
    return get(`/scheduled-tasks/${taskId}/runs`, params);
}

export async function listDownloadAdminInventory(params = {}) {
    return get('/downloads/admin/inventory', params);
}
