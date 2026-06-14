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

import { get, put, del } from './core.js';

export async function listUsers(params = {}) {
    return get('/users', params);
}

export async function getUser(userId) {
    return get(`/users/${userId}`);
}

export async function updateUser(userId, data) {
    return put(`/users/${userId}`, data);
}

export async function deleteUser(userId) {
    return del(`/users/${userId}`);
}

export async function getUserCapabilities(userId) {
    return get(`/users/${userId}/capabilities`);
}

export async function updateUserCapabilities(userId, data) {
    return put(`/users/${userId}/capabilities`, data);
}

