<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { onMount } from 'svelte';
    import { flip } from 'svelte/animate';
    import { fade } from 'svelte/transition';
    import {
        notificationCenter,
        notificationItems,
        unreadCount,
    } from '$lib/stores/notificationCenter.js';
    import {
        listNotificationTypes,
        listNotificationPreferences,
        updateNotificationPreference,
        listPushDevices,
        deletePushDevice,
        sendTestNotification,
    } from '$lib/api/notifications.js';
    import { hasCapability } from '$lib/stores/auth.js';
    import { notifications as toastStore } from '$lib/stores/notifications.js';

    const CATEGORY_META = {
        security: { label: 'Security', color: 'var(--color-error)' },
        system: { label: 'System', color: 'var(--color-accent)' },
        media: { label: 'Media', color: 'var(--color-success)' },
        task: { label: 'Task', color: 'var(--color-text-secondary)' },
        user: { label: 'User', color: 'var(--color-text-secondary)' },
    };

    let tab = $state('feed');

    let feedFilter = $state('all');
    let loadingFeed = $state(false);
    let feedError = $state(null);

    let preferences = $state([]);
    let preferenceEdits = $state({});
    let loadingPrefs = $state(false);
    let savingPrefsId = $state(null);
    let prefsError = $state(null);

    let devices = $state([]);
    let loadingDevices = $state(false);
    let devicesError = $state(null);
    let revokingId = $state(null);

    let canManage = $state(false);
    let sendingTest = $state(false);

    $effect(() => {
        const unsub = hasCapability('can_manage_server').subscribe((v) => (canManage = v));
        return unsub;
    });

    onMount(async () => {
        await Promise.all([
            loadFeed(),
            loadPreferences(),
            loadDevices(),
        ]);
    });

    async function loadFeed() {
        loadingFeed = true;
        feedError = null;
        try {
            await notificationCenter.refresh();
        } catch (err) {
            feedError = err.detail || err.message || 'Failed to load notifications';
        } finally {
            loadingFeed = false;
        }
    }

    async function loadPreferences() {
        loadingPrefs = true;
        prefsError = null;
        try {
            const resp = await listNotificationPreferences();
            preferences = resp?.preferences || [];
            preferenceEdits = {};
            for (const p of preferences) {
                preferenceEdits[p.notification_type_id] = {
                    in_app_enabled: p.in_app_enabled,
                    webhook_enabled: p.webhook_enabled,
                    push_enabled: p.push_enabled,
                };
            }
        } catch (err) {
            prefsError = err.detail || err.message || 'Failed to load preferences';
        } finally {
            loadingPrefs = false;
        }
    }

    async function loadDevices() {
        loadingDevices = true;
        devicesError = null;
        try {
            const resp = await listPushDevices();
            devices = resp?.devices || [];
        } catch (err) {
            devicesError = err.detail || err.message || 'Failed to load push devices';
        } finally {
            loadingDevices = false;
        }
    }

    function prefDirty(typeId) {
        const orig = preferences.find((p) => p.notification_type_id === typeId);
        const edit = preferenceEdits[typeId];
        if (!orig || !edit) return false;
        return (
            orig.in_app_enabled !== edit.in_app_enabled ||
            orig.webhook_enabled !== edit.webhook_enabled ||
            orig.push_enabled !== edit.push_enabled
        );
    }

    let anyPrefDirty = $derived(
        preferences.some((p) => prefDirty(p.notification_type_id)),
    );

    async function savePreference(typeId) {
        const edit = preferenceEdits[typeId];
        if (!edit) return;
        savingPrefsId = typeId;
        try {
            const resp = await updateNotificationPreference(typeId, {
                in_app_enabled: edit.in_app_enabled,
                webhook_enabled: edit.webhook_enabled,
                push_enabled: edit.push_enabled,
            });
            preferences = preferences.map((p) =>
                p.notification_type_id === typeId
                    ? {
                          ...p,
                          in_app_enabled: resp.in_app_enabled,
                          webhook_enabled: resp.webhook_enabled,
                          push_enabled: resp.push_enabled,
                          is_using_defaults: false,
                      }
                    : p,
            );
            preferenceEdits[typeId] = {
                in_app_enabled: resp.in_app_enabled,
                webhook_enabled: resp.webhook_enabled,
                push_enabled: resp.push_enabled,
            };
            toastStore.success('Notification preference saved');
        } catch (err) {
            toastStore.error(err.detail || err.message || 'Failed to save preference');
        } finally {
            savingPrefsId = null;
        }
    }

    async function revokeDevice(deviceId, name) {
        revokingId = deviceId;
        try {
            await deletePushDevice(deviceId);
            devices = devices.filter((d) => d.id !== deviceId);
            toastStore.success(`Revoked ${name || 'device'}`);
        } catch (err) {
            toastStore.error(err.detail || err.message || 'Failed to revoke device');
        } finally {
            revokingId = null;
        }
    }

    async function sendTest() {
        sendingTest = true;
        try {
            const resp = await sendTestNotification({});
            const status = resp?.delivery_status || {};
            const channels = Object.entries(status)
                .map(([k, v]) => `${k}: ${v}`)
                .join(', ');
            toastStore.success(`Test dispatched (${channels})`);
            await new Promise((r) => setTimeout(r, 600));
            await loadFeed();
        } catch (err) {
            toastStore.error(err.detail || err.message || 'Failed to send test notification');
        } finally {
            sendingTest = false;
        }
    }

    async function handleMarkRead(n) {
        if (n.is_read) return;
        try {
            await notificationCenter.markRead(n.id);
        } catch (err) {
            toastStore.error(err.detail || err.message || 'Failed to mark as read');
        }
    }

    async function handleMarkAllRead() {
        try {
            const count = await notificationCenter.markAllRead();
            toastStore.success(count > 0 ? `Marked ${count} as read` : 'Nothing to mark');
        } catch (err) {
            toastStore.error(err.detail || err.message || 'Failed to mark all as read');
        }
    }

    async function handleDeleteRead() {
        try {
            const count = await notificationCenter.deleteRead();
            toastStore.success(count > 0 ? `Deleted ${count} read notification${count === 1 ? '' : 's'}` : 'No read notifications');
        } catch (err) {
            toastStore.error(err.detail || err.message || 'Failed to delete read notifications');
        }
    }

    async function handleDeleteOne(event, n) {
        event.stopPropagation();
        try {
            await notificationCenter.remove(n.id);
        } catch (err) {
            toastStore.error(err.detail || err.message || 'Failed to delete notification');
        }
    }

    async function loadMoreFeed() {
        try {
            await notificationCenter.loadMore();
        } catch (err) {
            toastStore.error(err.detail || err.message || 'Failed to load more');
        }
    }

    function formatRelative(iso) {
        if (!iso) return '';
        const then = new Date(iso).getTime();
        if (Number.isNaN(then)) return '';
        const diff = Date.now() - then;
        const sec = Math.floor(diff / 1000);
        if (sec < 60) return 'just now';
        const min = Math.floor(sec / 60);
        if (min < 60) return `${min}m ago`;
        const hr = Math.floor(min / 60);
        if (hr < 24) return `${hr}h ago`;
        const day = Math.floor(hr / 24);
        if (day < 7) return `${day}d ago`;
        return new Date(then).toLocaleDateString();
    }

    function formatDateTime(iso) {
        if (!iso) return '—';
        const d = new Date(iso);
        if (Number.isNaN(d.getTime())) return '—';
        return d.toLocaleString();
    }

    function providerLabel(p) {
        if (p === 'fcm') return 'FCM (Firebase)';
        if (p === 'apns') return 'APNs (Apple)';
        if (p === 'unifiedpush') return 'UnifiedPush';
        return p;
    }

    let filteredFeed = $derived(
        feedFilter === 'unread'
            ? $notificationItems.filter((n) => !n.is_read)
            : $notificationItems,
    );
    let hasRead = $derived($notificationItems.some((n) => n.is_read));
</script>

<div class="page">
    <div class="page-header">
        <div>
            <a href="/settings" class="back-link">← Settings</a>
            <h1 class="page-title">Notifications</h1>
        </div>
    </div>

    <div class="tabs" role="tablist">
        <button
            class="tab"
            class:active={tab === 'feed'}
            onclick={() => (tab = 'feed')}
            role="tab"
            aria-selected={tab === 'feed'}
        >
            Feed
            {#if $unreadCount > 0}
                <span class="tab-badge">{$unreadCount}</span>
            {/if}
        </button>
        <button
            class="tab"
            class:active={tab === 'preferences'}
            onclick={() => (tab = 'preferences')}
            role="tab"
            aria-selected={tab === 'preferences'}
        >
            Preferences
            {#if anyPrefDirty}<span class="tab-dot"></span>{/if}
        </button>
        <button
            class="tab"
            class:active={tab === 'devices'}
            onclick={() => (tab = 'devices')}
            role="tab"
            aria-selected={tab === 'devices'}
        >
            Push Devices
            {#if devices.length > 0}<span class="tab-count">{devices.length}</span>{/if}
        </button>
    </div>

    {#if tab === 'feed'}
        <section class="card">
            <div class="card-head">
                <div class="filter-group">
                    <button class="chip" class:active={feedFilter === 'all'} onclick={() => (feedFilter = 'all')}>
                        All
                    </button>
                    <button class="chip" class:active={feedFilter === 'unread'} onclick={() => (feedFilter = 'unread')}>
                        Unread
                    </button>
                </div>
                <div class="action-group">
                    {#if $unreadCount > 0}
                        <button class="btn-ghost" onclick={handleMarkAllRead}>Mark all read</button>
                    {/if}
                    {#if hasRead}
                        <button class="btn-ghost danger" onclick={handleDeleteRead}>Delete read</button>
                    {/if}
                    <button class="btn-ghost" onclick={loadFeed} aria-label="Refresh">
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <path d="M23 4v6h-6M1 20v-6h6" />
                            <path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15" />
                        </svg>
                    </button>
                </div>
            </div>

            <div class="feed-body">
                {#if loadingFeed && $notificationItems.length === 0}
                    <div class="state-block">
                        <div class="mini-spinner"></div>
                        <p>Loading notifications…</p>
                    </div>
                {:else if feedError}
                    <div class="state-block error">
                        <p>{feedError}</p>
                        <button class="btn-secondary" onclick={loadFeed}>Retry</button>
                    </div>
                {:else if filteredFeed.length === 0}
                    <div class="state-block">
                        <p>{feedFilter === 'unread' ? 'No unread notifications.' : 'No notifications yet.'}</p>
                    </div>
                {:else}
                    <div class="feed-list">
                        {#each filteredFeed as n (n.id)}
                            <div
                                class="feed-item"
                                class:unread={!n.is_read}
                                onclick={() => handleMarkRead(n)}
                                role="button"
                                tabindex="0"
                                onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && handleMarkRead(n)}
                                animate:flip={{ duration: 180 }}
                                in:fade={{ duration: 150 }}
                            >
                                <span class="feed-cat" style="--cat-color: {(CATEGORY_META[n.category] || CATEGORY_META.user).color}">
                                    {(CATEGORY_META[n.category] || CATEGORY_META.user).label}
                                </span>
                                <div class="feed-main">
                                    <div class="feed-top">
                                        {#if n.title}
                                            <p class="feed-title">{n.title}</p>
                                        {/if}
                                        {#if !n.is_read}
                                            <span class="unread-dot" title="Unread"></span>
                                        {/if}
                                    </div>
                                    <p class="feed-text">{n.body}</p>
                                    <div class="feed-meta">
                                        <span>{formatRelative(n.created_at)}</span>
                                        {#if n.priority === 'high'}
                                            <span class="meta-sep">·</span>
                                            <span class="feed-priority">High priority</span>
                                        {/if}
                                        {#if n.is_read && n.read_at}
                                            <span class="meta-sep">·</span>
                                            <span class="feed-read">Read {formatRelative(n.read_at)}</span>
                                        {/if}
                                    </div>
                                </div>
                                <button
                                    class="feed-delete"
                                    onclick={(e) => handleDeleteOne(e, n)}
                                    aria-label="Delete notification"
                                >
                                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                        <path d="M3 6h18M19 6v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6m3 0V4a2 2 0 012-2h4a2 2 0 012 2v2" />
                                    </svg>
                                </button>
                            </div>
                        {/each}
                    </div>
                    {#if feedFilter === 'all' && notificationCenter.hasMore}
                        <div class="load-more">
                            <button class="btn-secondary" onclick={loadMoreFeed} disabled={notificationCenter.loadingMore}>
                                {notificationCenter.loadingMore ? 'Loading…' : 'Load more'}
                            </button>
                        </div>
                    {/if}
                {/if}
            </div>
        </section>
    {:else if tab === 'preferences'}
        <section class="card">
            <div class="card-head">
                <div>
                    <h2 class="card-title">Notification Preferences</h2>
                    <p class="card-sub">Choose how you want to be notified for each type.</p>
                </div>
            </div>

            <div class="prefs-body">
                {#if loadingPrefs}
                    <div class="state-block">
                        <div class="mini-spinner"></div>
                        <p>Loading preferences…</p>
                    </div>
                {:else if prefsError}
                    <div class="state-block error">
                        <p>{prefsError}</p>
                        <button class="btn-secondary" onclick={loadPreferences}>Retry</button>
                    </div>
                {:else if preferences.length === 0}
                    <div class="state-block">
                        <p>No notification types configured.</p>
                    </div>
                {:else}
                    <div class="prefs-table">
                        <div class="prefs-row prefs-head">
                            <div class="prefs-type">Type</div>
                            <div class="prefs-channel">In-App</div>
                            <div class="prefs-channel">Webhook</div>
                            <div class="prefs-channel">Push</div>
                            <div class="prefs-action"></div>
                        </div>
                        {#each preferences as p (p.notification_type_id)}
                            <div class="prefs-row">
                                <div class="prefs-type">
                                    <div class="prefs-name">
                                        {p.name.replace(/_/g, ' ')}
                                        {#if p.is_using_defaults}<span class="default-tag">default</span>{/if}
                                    </div>
                                    <div class="prefs-cat" style="--cat-color: {(CATEGORY_META[p.category] || CATEGORY_META.user).color}">
                                        {(CATEGORY_META[p.category] || CATEGORY_META.user).label} · {p.priority}
                                    </div>
                                </div>
                                <div class="prefs-channel" data-label="In-App">
                                    <label class="toggle">
                                        <input
                                            type="checkbox"
                                            checked={preferenceEdits[p.notification_type_id]?.in_app_enabled}
                                            onchange={(e) => {
                                                preferenceEdits[p.notification_type_id].in_app_enabled = e.currentTarget.checked;
                                                preferenceEdits = preferenceEdits;
                                            }}
                                        />
                                        <span class="toggle-track"></span>
                                    </label>
                                </div>
                                <div class="prefs-channel" data-label="Webhook">
                                    <label class="toggle">
                                        <input
                                            type="checkbox"
                                            checked={preferenceEdits[p.notification_type_id]?.webhook_enabled}
                                            onchange={(e) => {
                                                preferenceEdits[p.notification_type_id].webhook_enabled = e.currentTarget.checked;
                                                preferenceEdits = preferenceEdits;
                                            }}
                                        />
                                        <span class="toggle-track"></span>
                                    </label>
                                </div>
                                <div class="prefs-channel" data-label="Push">
                                    <label class="toggle">
                                        <input
                                            type="checkbox"
                                            checked={preferenceEdits[p.notification_type_id]?.push_enabled}
                                            onchange={(e) => {
                                                preferenceEdits[p.notification_type_id].push_enabled = e.currentTarget.checked;
                                                preferenceEdits = preferenceEdits;
                                            }}
                                        />
                                        <span class="toggle-track"></span>
                                    </label>
                                </div>
                                <div class="prefs-action">
                                    {#if prefDirty(p.notification_type_id)}
                                        <button
                                            class="btn-small"
                                            onclick={() => savePreference(p.notification_type_id)}
                                            disabled={savingPrefsId === p.notification_type_id}
                                        >
                                            {savingPrefsId === p.notification_type_id ? '…' : 'Save'}
                                        </button>
                                    {/if}
                                </div>
                            </div>
                        {/each}
                    </div>
                {/if}
            </div>
        </section>
    {:else if tab === 'devices'}
        <section class="card">
            <div class="card-head">
                <div>
                    <h2 class="card-title">Push Devices</h2>
                    <p class="card-sub">
                        Devices registered for mobile push. New devices register automatically from the mobile app.
                    </p>
                </div>
                <button class="btn-ghost" onclick={loadDevices} aria-label="Refresh">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <path d="M23 4v6h-6M1 20v-6h6" />
                        <path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15" />
                    </svg>
                </button>
            </div>

            <div class="devices-body">
                {#if loadingDevices}
                    <div class="state-block">
                        <div class="mini-spinner"></div>
                        <p>Loading devices…</p>
                    </div>
                {:else if devicesError}
                    <div class="state-block error">
                        <p>{devicesError}</p>
                        <button class="btn-secondary" onclick={loadDevices}>Retry</button>
                    </div>
                {:else if devices.length === 0}
                    <div class="state-block empty-devices">
                        <svg width="36" height="36" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round">
                            <rect x="5" y="2" width="14" height="20" rx="2" />
                            <path d="M12 18h.01" />
                        </svg>
                        <p>No push devices registered.</p>
                        <p class="muted">Install the Duskcue mobile app and sign in to register a device for push notifications.</p>
                    </div>
                {:else}
                    <ul class="device-list">
                        {#each devices as d (d.id)}
                            <li class="device-item">
                                <div class="device-icon">
                                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
                                        <rect x="5" y="2" width="14" height="20" rx="2" />
                                        <path d="M12 18h.01" />
                                    </svg>
                                </div>
                                <div class="device-main">
                                    <div class="device-name">
                                        {d.device_name || 'Unnamed device'}
                                        {#if !d.is_active}<span class="inactive-tag">inactive</span>{/if}
                                    </div>
                                    <div class="device-meta">
                                        <span>{providerLabel(d.provider)}</span>
                                        {#if d.platform}<span class="meta-sep">·</span><span>{d.platform}</span>{/if}
                                        {#if d.app_version}<span class="meta-sep">·</span><span>v{d.app_version}</span>{/if}
                                    </div>
                                    <div class="device-sub">
                                        <span class="device-token">{d.token_preview}</span>
                                        {#if d.last_seen_at}
                                            <span class="meta-sep">·</span>
                                            <span>Last seen {formatRelative(d.last_seen_at)}</span>
                                        {/if}
                                        {#if d.invalidated_at}
                                            <span class="meta-sep">·</span>
                                            <span>Invalidated {formatRelative(d.invalidated_at)}</span>
                                        {/if}
                                    </div>
                                </div>
                                <button
                                    class="btn-ghost danger"
                                    onclick={() => revokeDevice(d.id, d.device_name)}
                                    disabled={revokingId === d.id}
                                >
                                    {revokingId === d.id ? 'Revoking…' : 'Revoke'}
                                </button>
                            </li>
                        {/each}
                    </ul>
                {/if}
            </div>
        </section>

        {#if canManage}
            <section class="card admin-card">
                <div class="card-head">
                    <div>
                        <h2 class="card-title">Test Notification</h2>
                        <p class="card-sub">Send a test notification to yourself to verify the dispatch pipeline.</p>
                    </div>
                </div>
                <div class="test-body">
                    <p class="test-desc">
                        Dispatches a <code>server_alert</code> notification through the standard pipeline
                        (in-app + SSE + webhook + push). Check the Feed tab and your configured webhook/push destination.
                    </p>
                    <button class="btn-primary" onclick={sendTest} disabled={sendingTest}>
                        {sendingTest ? 'Sending…' : 'Send Test Notification'}
                    </button>
                </div>
            </section>
        {/if}
    {/if}
</div>

<style>
    .page {
        display: flex;
        flex-direction: column;
        gap: 1.25rem;
        max-width: 880px;
    }

    .page-header {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
    }

    .back-link {
        display: inline-block;
        font-size: 0.8125rem;
        color: var(--color-text-muted);
        margin-bottom: 0.25rem;
    }

    .back-link:hover {
        color: var(--color-accent);
    }

    .page-title {
        font-size: 1.5rem;
        font-weight: 700;
        color: var(--color-text-primary);
    }

    .tabs {
        display: flex;
        gap: 0.25rem;
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .tab {
        display: inline-flex;
        align-items: center;
        gap: 0.5rem;
        padding: 0.625rem 1rem;
        font-size: 0.875rem;
        font-weight: 500;
        color: var(--color-text-secondary);
        border-bottom: 2px solid transparent;
        transition: color var(--transition-fast), border-color var(--transition-fast);
    }

    .tab:hover {
        color: var(--color-text-primary);
    }

    .tab.active {
        color: var(--color-accent);
        border-bottom-color: var(--color-accent);
    }

    .tab-badge {
        font-size: 0.625rem;
        font-weight: 700;
        color: #fff;
        background-color: var(--color-error);
        padding: 0.0625rem 0.4rem;
        border-radius: 8px;
        min-width: 16px;
        text-align: center;
    }

    .tab-count {
        font-size: 0.625rem;
        font-weight: 700;
        color: var(--color-text-secondary);
        background-color: var(--color-bg-hover);
        padding: 0.0625rem 0.4rem;
        border-radius: 8px;
        min-width: 16px;
        text-align: center;
    }

    .tab-dot {
        width: 6px;
        height: 6px;
        border-radius: 50%;
        background-color: var(--color-accent);
    }

    .card {
        background-color: var(--color-bg-surface);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-lg);
        overflow: hidden;
    }

    .card-head {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 1rem;
        padding: 1rem 1.25rem;
        border-bottom: 1px solid var(--color-border-subtle);
        flex-wrap: wrap;
    }

    .card-title {
        font-size: 0.9375rem;
        font-weight: 700;
        color: var(--color-text-primary);
    }

    .card-sub {
        font-size: 0.75rem;
        color: var(--color-text-muted);
        margin-top: 0.125rem;
    }

    .filter-group,
    .action-group {
        display: flex;
        align-items: center;
        gap: 0.375rem;
    }

    .chip {
        padding: 0.3125rem 0.75rem;
        font-size: 0.75rem;
        font-weight: 600;
        color: var(--color-text-secondary);
        background-color: var(--color-bg-elevated);
        border: 1px solid var(--color-border-subtle);
        border-radius: 999px;
        transition: all var(--transition-fast);
    }

    .chip.active {
        color: var(--color-accent);
        border-color: var(--color-accent);
        background-color: var(--color-accent-muted);
    }

    .btn-ghost {
        display: inline-flex;
        align-items: center;
        gap: 0.375rem;
        padding: 0.375rem 0.625rem;
        font-size: 0.75rem;
        font-weight: 500;
        color: var(--color-text-secondary);
        border-radius: var(--radius-sm);
        transition: color var(--transition-fast), background-color var(--transition-fast);
    }

    .btn-ghost:hover {
        color: var(--color-text-primary);
        background-color: var(--color-bg-hover);
    }

    .btn-ghost.danger:hover {
        color: var(--color-error);
    }

    .btn-ghost:disabled {
        opacity: 0.5;
        cursor: default;
    }

    .btn-secondary {
        padding: 0.5rem 1rem;
        font-size: 0.8125rem;
        font-weight: 600;
        color: var(--color-text-primary);
        background-color: var(--color-bg-elevated);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        transition: background-color var(--transition-fast);
    }

    .btn-secondary:hover {
        background-color: var(--color-bg-hover);
    }

    .btn-primary {
        padding: 0.5rem 1.125rem;
        font-size: 0.8125rem;
        font-weight: 600;
        color: var(--color-bg-deep);
        background-color: var(--color-accent);
        border-radius: var(--radius-sm);
        transition: background-color var(--transition-fast);
    }

    .btn-primary:hover:not(:disabled) {
        background-color: var(--color-accent-hover);
    }

    .btn-primary:disabled {
        opacity: 0.6;
        cursor: default;
    }

    .btn-small {
        padding: 0.25rem 0.625rem;
        font-size: 0.6875rem;
        font-weight: 600;
        color: var(--color-bg-deep);
        background-color: var(--color-accent);
        border-radius: var(--radius-sm);
    }

    .btn-small:disabled {
        opacity: 0.6;
        cursor: default;
    }

    .feed-body,
    .prefs-body,
    .devices-body,
    .test-body {
        padding: 0.5rem 0;
    }

    .state-block {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: 0.75rem;
        padding: 2.5rem 1rem;
        color: var(--color-text-muted);
        font-size: 0.8125rem;
        text-align: center;
    }

    .state-block.error {
        color: var(--color-error);
    }

    .state-block.empty-devices .muted {
        color: var(--color-text-muted);
        font-size: 0.75rem;
        max-width: 360px;
    }

    .mini-spinner {
        width: 22px;
        height: 22px;
        border: 2px solid var(--color-border);
        border-top-color: var(--color-accent);
        border-radius: 50%;
        animation: page-spin 0.8s linear infinite;
    }

    @keyframes page-spin {
        to { transform: rotate(360deg); }
    }

    .feed-list,
    .device-list {
        list-style: none;
    }

    .feed-item {
        display: flex;
        align-items: flex-start;
        gap: 0.875rem;
        padding: 0.875rem 1.25rem;
        border-bottom: 1px solid var(--color-border-subtle);
        transition: background-color var(--transition-fast);
    }

    .feed-item:last-child {
        border-bottom: none;
    }

    .feed-item:hover {
        background-color: var(--color-bg-hover);
    }

    .feed-item.unread {
        background-color: var(--color-accent-muted);
    }

    .feed-item.unread:hover {
        background-color: rgba(200, 150, 90, 0.22);
    }

    .feed-cat {
        flex-shrink: 0;
        align-self: flex-start;
        margin-top: 0.0625rem;
        font-size: 0.5625rem;
        font-weight: 700;
        text-transform: uppercase;
        letter-spacing: 0.05em;
        color: var(--cat-color, var(--color-text-muted));
        padding: 0.1875rem 0.5rem;
        border: 1px solid color-mix(in srgb, var(--cat-color, var(--color-text-muted)) 40%, transparent);
        border-radius: 999px;
        background-color: color-mix(in srgb, var(--cat-color, var(--color-text-muted)) 12%, transparent);
        min-width: 60px;
        text-align: center;
    }

    .feed-main {
        flex: 1;
        min-width: 0;
    }

    .feed-top {
        display: flex;
        align-items: center;
        gap: 0.5rem;
    }

    .feed-title {
        font-size: 0.875rem;
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .unread-dot {
        width: 8px;
        height: 8px;
        border-radius: 50%;
        background-color: var(--color-accent);
        flex-shrink: 0;
    }

    .feed-text {
        font-size: 0.8125rem;
        color: var(--color-text-secondary);
        line-height: 1.5;
        margin-top: 0.125rem;
    }

    .feed-meta {
        display: flex;
        align-items: center;
        gap: 0.375rem;
        margin-top: 0.375rem;
        font-size: 0.6875rem;
        color: var(--color-text-muted);
    }

    .meta-sep {
        opacity: 0.6;
    }

    .feed-priority {
        color: var(--color-error);
        font-weight: 600;
    }

    .feed-read {
        opacity: 0.8;
    }

    .feed-delete {
        flex-shrink: 0;
        color: var(--color-text-muted);
        padding: 0.375rem;
        border-radius: var(--radius-sm);
        transition: color var(--transition-fast), background-color var(--transition-fast);
        opacity: 0;
    }

    .feed-item:hover .feed-delete {
        opacity: 1;
    }

    .feed-delete:hover {
        color: var(--color-error);
        background-color: var(--color-error-bg);
    }

    .load-more {
        padding: 1rem;
        text-align: center;
    }

    .prefs-table {
        display: flex;
        flex-direction: column;
    }

    .prefs-row {
        display: grid;
        grid-template-columns: 1fr 80px 80px 80px 64px;
        align-items: center;
        gap: 0.5rem;
        padding: 0.75rem 1.25rem;
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .prefs-row:last-child {
        border-bottom: none;
    }

    .prefs-head {
        font-size: 0.6875rem;
        font-weight: 700;
        text-transform: uppercase;
        letter-spacing: 0.05em;
        color: var(--color-text-muted);
        background-color: var(--color-bg-elevated);
    }

    .prefs-channel {
        text-align: center;
    }

    .prefs-action {
        text-align: right;
    }

    .prefs-name {
        font-size: 0.8125rem;
        font-weight: 600;
        color: var(--color-text-primary);
        text-transform: capitalize;
        display: flex;
        align-items: center;
        gap: 0.5rem;
    }

    .default-tag {
        font-size: 0.5625rem;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.04em;
        color: var(--color-text-muted);
        background-color: var(--color-bg-hover);
        padding: 0.0625rem 0.375rem;
        border-radius: 3px;
    }

    .prefs-cat {
        font-size: 0.6875rem;
        color: var(--cat-color, var(--color-text-muted));
        text-transform: capitalize;
        margin-top: 0.125rem;
    }

    .toggle {
        position: relative;
        display: inline-block;
        cursor: pointer;
    }

    .toggle input {
        position: absolute;
        opacity: 0;
        width: 100%;
        height: 100%;
        margin: 0;
        cursor: pointer;
    }

    .toggle-track {
        display: block;
        width: 34px;
        height: 18px;
        background-color: var(--color-border);
        border-radius: 9px;
        position: relative;
        transition: background-color var(--transition-fast);
    }

    .toggle-track::after {
        content: '';
        position: absolute;
        top: 2px;
        left: 2px;
        width: 14px;
        height: 14px;
        background-color: var(--color-text-primary);
        border-radius: 50%;
        transition: transform var(--transition-fast);
    }

    .toggle input:checked + .toggle-track {
        background-color: var(--color-accent);
    }

    .toggle input:checked + .toggle-track::after {
        transform: translateX(16px);
        background-color: var(--color-bg-deep);
    }

    .toggle input:focus-visible + .toggle-track {
        outline: 2px solid var(--color-accent);
        outline-offset: 2px;
    }

    .device-item {
        display: flex;
        align-items: center;
        gap: 0.875rem;
        padding: 0.875rem 1.25rem;
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .device-item:last-child {
        border-bottom: none;
    }

    .device-icon {
        flex-shrink: 0;
        width: 36px;
        height: 36px;
        display: flex;
        align-items: center;
        justify-content: center;
        background-color: var(--color-bg-elevated);
        border-radius: var(--radius-sm);
        color: var(--color-text-secondary);
    }

    .device-main {
        flex: 1;
        min-width: 0;
    }

    .device-name {
        font-size: 0.875rem;
        font-weight: 600;
        color: var(--color-text-primary);
        display: flex;
        align-items: center;
        gap: 0.5rem;
    }

    .inactive-tag {
        font-size: 0.5625rem;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.04em;
        color: var(--color-text-muted);
        background-color: var(--color-bg-hover);
        padding: 0.0625rem 0.375rem;
        border-radius: 3px;
    }

    .device-meta {
        font-size: 0.75rem;
        color: var(--color-text-secondary);
        display: flex;
        align-items: center;
        gap: 0.375rem;
        margin-top: 0.125rem;
    }

    .device-sub {
        font-size: 0.6875rem;
        color: var(--color-text-muted);
        display: flex;
        align-items: center;
        gap: 0.375rem;
        margin-top: 0.25rem;
        flex-wrap: wrap;
    }

    .device-token {
        font-family: ui-monospace, 'SF Mono', Menlo, Consolas, monospace;
        font-size: 0.625rem;
        letter-spacing: 0.02em;
    }

    .admin-card {
        margin-top: 1rem;
    }

    .test-body {
        padding: 1.25rem;
    }

    .test-desc {
        font-size: 0.8125rem;
        color: var(--color-text-secondary);
        margin-bottom: 1rem;
        line-height: 1.5;
    }

    .test-desc code {
        font-family: ui-monospace, 'SF Mono', Menlo, Consolas, monospace;
        font-size: 0.75rem;
        background-color: var(--color-bg-elevated);
        padding: 0.0625rem 0.375rem;
        border-radius: var(--radius-sm);
        color: var(--color-accent);
    }

    @media (max-width: 640px) {
        .prefs-row {
            grid-template-columns: 1fr;
            gap: 0.625rem;
            padding: 0.875rem 1rem;
        }

        .prefs-head {
            display: none;
        }

        .prefs-channel {
            text-align: left;
            display: flex;
            align-items: center;
            gap: 0.625rem;
        }

        .prefs-channel::before {
            content: attr(data-label);
            font-size: 0.6875rem;
            font-weight: 600;
            text-transform: uppercase;
            letter-spacing: 0.04em;
            color: var(--color-text-muted);
        }

        .prefs-action {
            text-align: left;
        }

        .feed-cat {
            min-width: 48px;
        }

        .card-head {
            flex-direction: column;
            align-items: flex-start;
        }
    }
</style>
