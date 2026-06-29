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

import { get, post, put, del } from './core.js';

export async function listNotifications(params = {}) {
    return get('/notifications', params);
}

export async function getUnreadCount() {
    return get('/notifications/unread-count');
}

export async function markNotificationRead(notificationId) {
    return post(`/notifications/${notificationId}/read`);
}

export async function markAllRead() {
    return post('/notifications/read-all');
}

export async function deleteNotification(notificationId) {
    return del(`/notifications/${notificationId}`);
}

export async function deleteReadNotifications() {
    return del('/notifications/read');
}

export async function listNotificationTypes() {
    return get('/notification-types');
}

export async function listNotificationPreferences() {
    return get('/user/notification-preferences');
}

export async function updateNotificationPreference(typeId, data) {
    return put(`/user/notification-preferences/${typeId}`, data);
}

export async function sendTestNotification(data = {}) {
    return post('/notifications/test', data);
}
