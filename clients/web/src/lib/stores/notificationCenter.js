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
import {
    listNotifications as apiListNotifications,
    getUnreadCount as apiGetUnreadCount,
    markNotificationRead as apiMarkRead,
    markAllRead as apiMarkAllRead,
    deleteNotification as apiDeleteNotification,
    deleteReadNotifications as apiDeleteRead,
} from '../api/notifications.js';
import { events } from './events.js';

const PAGE_SIZE = 20;
const UNREAD_POLL_MS = 60000;

function extractItems(response) {
    if (Array.isArray(response)) return response;
    if (response && Array.isArray(response.items)) return response.items;
    return [];
}

function createNotificationCenterStore() {
    const { subscribe, update, set } = writable({
        items: [],
        unreadCount: 0,
        cursor: null,
        hasMore: false,
        loading: false,
        loadingMore: false,
        error: null,
        initialized: false,
    });

    let pollTimer = null;
    let sseUnsub = null;
    let started = false;

    async function fetchUnreadCount() {
        try {
            const resp = await apiGetUnreadCount();
            update((s) => ({ ...s, unreadCount: resp?.unread_count ?? 0 }));
        } catch {
            // best-effort; SSE remains the primary source of truth
        }
    }

    function startPolling() {
        if (pollTimer !== null) return;
        if (typeof setInterval === 'undefined') return;
        pollTimer = setInterval(() => {
            fetchUnreadCount();
        }, UNREAD_POLL_MS);
    }

    function stopPolling() {
        if (pollTimer !== null) {
            clearInterval(pollTimer);
            pollTimer = null;
        }
    }

    function startSse() {
        if (sseUnsub !== null) return;
        if (typeof window === 'undefined') return;
        sseUnsub = events.on('notification', (payload) => {
            if (!payload || !payload.id) return;
            update((s) => {
                if (s.items.some((n) => n.id === payload.id)) return s;
                const item = {
                    id: payload.id,
                    notification_type: payload.notification_type,
                    category: payload.category,
                    priority: payload.priority,
                    title: payload.title || '',
                    body: payload.body || '',
                    link: payload.link || null,
                    related_item_type: payload.related_item_type || null,
                    related_item_id: payload.related_item_id || null,
                    is_read: false,
                    read_at: null,
                    created_at: payload.created_at || new Date().toISOString(),
                };
                return {
                    ...s,
                    items: [item, ...s.items],
                    unreadCount: s.unreadCount + 1,
                };
            });
        });
    }

    function stopSse() {
        if (sseUnsub !== null) {
            sseUnsub();
            sseUnsub = null;
        }
    }

    return {
        subscribe,

        async init() {
            if (started) return;
            started = true;
            startSse();
            startPolling();
            await this.refresh();
        },

        shutdown() {
            started = false;
            stopSse();
            stopPolling();
        },

        async refresh() {
            update((s) => ({ ...s, loading: true, error: null }));
            try {
                const [listResp, countResp] = await Promise.all([
                    apiListNotifications({ limit: PAGE_SIZE }),
                    apiGetUnreadCount(),
                ]);
                const items = extractItems(listResp);
                update((s) => ({
                    ...s,
                    items,
                    unreadCount: countResp?.unread_count ?? 0,
                    cursor: listResp?.cursor ?? null,
                    hasMore: listResp?.has_more ?? false,
                    loading: false,
                    initialized: true,
                    error: null,
                }));
            } catch (err) {
                update((s) => ({
                    ...s,
                    loading: false,
                    initialized: true,
                    error: err.detail || err.message || 'Failed to load notifications',
                }));
            }
        },

        async refreshUnreadCount() {
            await fetchUnreadCount();
        },

        async loadMore() {
            let cursor;
            let canLoad;
            const unsub = subscribe((s) => {
                cursor = s.cursor;
                canLoad = s.hasMore && !s.loadingMore;
            });
            unsub();
            if (!canLoad || !cursor) return;
            update((s) => ({ ...s, loadingMore: true }));
            try {
                const resp = await apiListNotifications({ limit: PAGE_SIZE, cursor });
                const items = extractItems(resp);
                update((s) => ({
                    ...s,
                    items: [...s.items, ...items],
                    cursor: resp?.cursor ?? null,
                    hasMore: resp?.has_more ?? false,
                    loadingMore: false,
                }));
            } catch (err) {
                update((s) => ({
                    ...s,
                    loadingMore: false,
                    error: err.detail || err.message || 'Failed to load more notifications',
                }));
            }
        },

        async markRead(notificationId) {
            try {
                await apiMarkRead(notificationId);
                update((s) => {
                    let decremented = false;
                    const items = s.items.map((n) => {
                        if (n.id === notificationId && !n.is_read) {
                            decremented = true;
                            return { ...n, is_read: true, read_at: new Date().toISOString() };
                        }
                        return n;
                    });
                    return {
                        ...s,
                        items,
                        unreadCount: decremented ? Math.max(0, s.unreadCount - 1) : s.unreadCount,
                    };
                });
            } catch (err) {
                update((s) => ({
                    ...s,
                    error: err.detail || err.message || 'Failed to mark notification as read',
                }));
                throw err;
            }
        },

        async markAllRead() {
            try {
                const resp = await apiMarkAllRead();
                update((s) => ({
                    ...s,
                    items: s.items.map((n) => ({
                        ...n,
                        is_read: true,
                        read_at: n.read_at || new Date().toISOString(),
                    })),
                    unreadCount: 0,
                }));
                return resp?.marked_read ?? 0;
            } catch (err) {
                update((s) => ({
                    ...s,
                    error: err.detail || err.message || 'Failed to mark all notifications as read',
                }));
                throw err;
            }
        },

        async remove(notificationId) {
            try {
                await apiDeleteNotification(notificationId);
                update((s) => {
                    const removed = s.items.find((n) => n.id === notificationId);
                    const wasUnread = removed && !removed.is_read;
                    return {
                        ...s,
                        items: s.items.filter((n) => n.id !== notificationId),
                        unreadCount: wasUnread ? Math.max(0, s.unreadCount - 1) : s.unreadCount,
                    };
                });
            } catch (err) {
                update((s) => ({
                    ...s,
                    error: err.detail || err.message || 'Failed to delete notification',
                }));
                throw err;
            }
        },

        async deleteRead() {
            try {
                const resp = await apiDeleteRead();
                update((s) => ({
                    ...s,
                    items: s.items.filter((n) => !n.is_read),
                }));
                return resp?.deleted ?? 0;
            } catch (err) {
                update((s) => ({
                    ...s,
                    error: err.detail || err.message || 'Failed to delete read notifications',
                }));
                throw err;
            }
        },

        reset() {
            set({
                items: [],
                unreadCount: 0,
                cursor: null,
                hasMore: false,
                loading: false,
                loadingMore: false,
                error: null,
                initialized: false,
            });
            started = false;
            stopSse();
            stopPolling();
        },

        clearError() {
            update((s) => ({ ...s, error: null }));
        },
    };
}

export const notificationCenter = createNotificationCenterStore();

export const notificationItems = derived(notificationCenter, ($n) => $n.items);

export const unreadCount = derived(notificationCenter, ($n) => $n.unreadCount);

export const notificationsLoading = derived(notificationCenter, ($n) => $n.loading);

export const notificationsError = derived(notificationCenter, ($n) => $n.error);
