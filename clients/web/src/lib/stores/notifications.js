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

import { writable, derived } from 'svelte/store';

const DEFAULT_DURATION = 5000;
const MAX_NOTIFICATIONS = 5;

let counter = 0;

function generateId() {
    counter += 1;
    return `notification-${Date.now()}-${counter}`;
}

function createNotificationsStore() {
    const { subscribe, update } = writable([]);

    const timers = new Map();

    function scheduleRemoval(id, duration) {
        if (typeof setTimeout === 'undefined') return;
        const timer = setTimeout(() => {
            removeById(id);
        }, duration);
        timers.set(id, timer);
    }

    function removeById(id) {
        const timer = timers.get(id);
        if (timer !== undefined) {
            clearTimeout(timer);
            timers.delete(id);
        }
        update((items) => items.filter((n) => n.id !== id));
    }

    function add(notification) {
        const item = {
            id: generateId(),
            type: notification.type || 'info',
            title: notification.title || null,
            message: notification.message || '',
            duration: notification.duration ?? DEFAULT_DURATION,
            dismissible: notification.dismissible !== false,
        };

        update((items) => {
            const updated = [...items, item];
            if (updated.length > MAX_NOTIFICATIONS) {
                const removed = updated.slice(0, updated.length - MAX_NOTIFICATIONS);
                for (const r of removed) {
                    const t = timers.get(r.id);
                    if (t !== undefined) {
                        clearTimeout(t);
                        timers.delete(r.id);
                    }
                }
                return updated.slice(updated.length - MAX_NOTIFICATIONS);
            }
            return updated;
        });

        if (item.duration > 0) {
            scheduleRemoval(item.id, item.duration);
        }

        return item.id;
    }

    return {
        subscribe,

        success(message, options = {}) {
            return add({ ...options, type: 'success', message });
        },

        error(message, options = {}) {
            return add({
                ...options,
                type: 'error',
                message,
                duration: options.duration ?? 8000,
            });
        },

        warning(message, options = {}) {
            return add({ ...options, type: 'warning', message });
        },

        info(message, options = {}) {
            return add({ ...options, type: 'info', message });
        },

        dismiss(id) {
            removeById(id);
        },

        clear() {
            for (const timer of timers.values()) {
                clearTimeout(timer);
            }
            timers.clear();
            update(() => []);
        },

        add,
    };
}

export const notifications = createNotificationsStore();

export const notificationList = derived(notifications, ($notifications) => $notifications);
