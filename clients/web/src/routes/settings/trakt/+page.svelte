<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { onDestroy, onMount } from 'svelte';
    import { notifications } from '$lib/stores/notifications.js';
    import {
        getTraktAccount,
        getTraktSyncSettings,
        getTraktSyncStatus,
        pollTraktLink,
        startTraktLink,
        triggerTraktSync,
        unlinkTraktAccount,
        updateTraktSyncSettings,
    } from '$lib/api/trakt.js';

    let loading = $state(true);
    let loadError = $state('');
    let account = $state(null);
    let syncSettings = $state(null);
    let syncStatus = $state(null);
    let deviceLink = $state(null);
    let linkError = $state('');
    let linking = $state(false);
    let polling = $state(false);
    let savingSetting = $state('');
    let syncing = $state(false);
    let disconnecting = $state(false);
    let pollTimer;

    onMount(load);
    onDestroy(() => clearPollTimer());

    async function load() {
        loading = true;
        loadError = '';
        try {
            account = await getTraktAccount();
            if (account.linked) {
                const [settings, status] = await Promise.all([getTraktSyncSettings(), getTraktSyncStatus()]);
                syncSettings = settings;
                syncStatus = status;
            } else {
                syncSettings = null;
                syncStatus = null;
            }
        } catch (err) {
            loadError = err.detail || err.message || 'Failed to load Trakt settings.';
        } finally {
            loading = false;
        }
    }

    async function beginLink() {
        linking = true;
        linkError = '';
        clearPollTimer();
        try {
            deviceLink = await startTraktLink();
        } catch (err) {
            linkError = err.detail || err.message || 'Trakt linking is not available yet.';
        } finally {
            linking = false;
        }
    }

    function clearPollTimer() {
        if (pollTimer) {
            clearTimeout(pollTimer);
            pollTimer = undefined;
        }
    }

    function schedulePoll() {
        clearPollTimer();
        if (!deviceLink) return;
        pollTimer = setTimeout(pollLink, Math.max(1, deviceLink.interval || 5) * 1000);
    }

    async function pollLink() {
        if (!deviceLink || polling) return;
        polling = true;
        linkError = '';
        try {
            const linkedAccount = await pollTraktLink(deviceLink.device_code);
            account = linkedAccount;
            deviceLink = null;
            await load();
            notifications.success('Trakt account linked.');
        } catch (err) {
            const detail = err.detail || err.message || '';
            if (String(detail).toLowerCase().includes('pending')) {
                schedulePoll();
            } else {
                linkError = detail || 'Trakt linking did not complete.';
                deviceLink = null;
            }
        } finally {
            polling = false;
        }
    }

    async function setSyncSetting(key, value) {
        if (!syncSettings) return;
        savingSetting = key;
        try {
            syncSettings = await updateTraktSyncSettings({ [key]: value });
            account = { ...account, ...syncSettings };
        } catch (err) {
            notifications.error(err.detail || err.message || 'Failed to save sync settings.');
        } finally {
            savingSetting = '';
        }
    }

    async function runSync() {
        syncing = true;
        try {
            const result = await triggerTraktSync();
            syncStatus = await getTraktSyncStatus();
            notifications.success(result.message || 'Trakt sync completed.');
        } catch (err) {
            notifications.error(err.detail || err.message || 'Trakt sync failed.');
            try {
                syncStatus = await getTraktSyncStatus();
            } catch {
                syncStatus = syncStatus;
            }
        } finally {
            syncing = false;
        }
    }

    async function disconnect() {
        if (!confirm('Disconnect your Trakt account? Synced state will be removed from Duskcue.')) return;
        disconnecting = true;
        try {
            await unlinkTraktAccount();
            clearPollTimer();
            account = { linked: false };
            syncSettings = null;
            syncStatus = null;
            deviceLink = null;
            notifications.success('Trakt account disconnected.');
        } catch (err) {
            notifications.error(err.detail || err.message || 'Failed to disconnect Trakt account.');
        } finally {
            disconnecting = false;
        }
    }

    function formatDate(value) {
        if (!value) return 'Never';
        const date = new Date(value);
        return Number.isNaN(date.getTime()) ? 'Unknown' : date.toLocaleString();
    }
</script>

<div class="trakt-settings">
    <header class="page-header">
        <div>
            <a href="/settings" class="back-link">Settings</a>
            <h1 class="page-title">Trakt</h1>
            <p class="page-description">Link your Trakt account to keep watched history in sync with Duskcue.</p>
        </div>
        {#if account?.linked}
            <button class="btn-secondary danger" onclick={disconnect} disabled={disconnecting}>
                {disconnecting ? 'Disconnecting…' : 'Disconnect'}
            </button>
        {/if}
    </header>

    {#if loading}
        <div class="loading-state"><div class="loading-spinner"></div></div>
    {:else if loadError}
        <div class="empty-state">
            <p class="error-text">{loadError}</p>
            <button class="btn-secondary" onclick={load}>Retry</button>
        </div>
    {:else if !account?.linked}
        <section class="settings-card connect-card">
            <div>
                <h2>Connect Trakt</h2>
                <p>Authorize Duskcue from a browser, then return here while we finish linking your account.</p>
            </div>
            {#if deviceLink}
                <div class="device-flow">
                    <p>Open Trakt and enter this code:</p>
                    <strong class="user-code">{deviceLink.user_code}</strong>
                    <a class="btn-primary" href={deviceLink.verification_url_complete || deviceLink.verification_url} target="_blank" rel="noopener">
                        Open Trakt activation
                    </a>
                    <button class="btn-secondary" onclick={pollLink} disabled={polling}>
                        {polling ? 'Checking…' : 'I have authorized'}
                    </button>
                    <p class="hint">The code expires in about {Math.ceil(deviceLink.expires_in / 60)} minutes.</p>
                </div>
            {:else}
                <button class="btn-primary" onclick={beginLink} disabled={linking}>
                    {linking ? 'Starting…' : 'Link Trakt account'}
                </button>
            {/if}
            {#if linkError}<p class="error-text">{linkError}</p>{/if}
        </section>
    {:else}
        <section class="settings-card account-card">
            <div>
                <h2>Connected as {account.trakt_username}</h2>
                <p>Token refreshes automatically. Access expires {formatDate(account.token_expires_at)}.</p>
            </div>
            <span class="status-badge connected">Connected</span>
        </section>

        <section class="settings-card">
            <div class="card-header">
                <div>
                    <h2>Sync preferences</h2>
                    <p>Watched history is synchronized both ways. Ratings and collection are imported from Trakt.</p>
                </div>
                <button class="btn-primary" onclick={runSync} disabled={syncing || !syncSettings?.sync_enabled}>
                    {syncing ? 'Syncing…' : 'Sync now'}
                </button>
            </div>
            <div class="card-body toggle-list">
                <label class="toggle-row">
                    <input type="checkbox" checked={syncSettings?.sync_enabled} onchange={(event) => setSyncSetting('sync_enabled', event.currentTarget.checked)} disabled={savingSetting === 'sync_enabled'} />
                    <span><strong>Enable Trakt sync</strong><small>Allow scheduled and manual synchronization for this account.</small></span>
                </label>
                <label class="toggle-row">
                    <input type="checkbox" checked={syncSettings?.sync_watched} onchange={(event) => setSyncSetting('sync_watched', event.currentTarget.checked)} disabled={!syncSettings?.sync_enabled || savingSetting === 'sync_watched'} />
                    <span><strong>Watched history</strong><small>Pull Trakt history and send newly watched Duskcue items to Trakt.</small></span>
                </label>
                <label class="toggle-row">
                    <input type="checkbox" checked={syncSettings?.sync_ratings} onchange={(event) => setSyncSetting('sync_ratings', event.currentTarget.checked)} disabled={!syncSettings?.sync_enabled || savingSetting === 'sync_ratings'} />
                    <span><strong>Ratings</strong><small>Import Trakt ratings when no local rating is set.</small></span>
                </label>
                <label class="toggle-row">
                    <input type="checkbox" checked={syncSettings?.sync_collection} onchange={(event) => setSyncSetting('sync_collection', event.currentTarget.checked)} disabled={!syncSettings?.sync_enabled || savingSetting === 'sync_collection'} />
                    <span><strong>Collection</strong><small>Import Trakt collection state for matched movies.</small></span>
                </label>
                <p class="hint">Watchlist sync is not available yet, so it is intentionally not offered as a setting.</p>
            </div>
        </section>

        <section class="settings-card">
            <div class="card-header">
                <div>
                    <h2>Sync status</h2>
                    <p>Review the most recent sync result before retrying a failed run.</p>
                </div>
                <button class="btn-secondary" onclick={load}>Refresh</button>
            </div>
            <div class="status-grid">
                <div><span>Last successful sync</span><strong>{formatDate(syncStatus?.last_full_sync_at)}</strong></div>
                <div><span>Last attempt</span><strong>{formatDate(syncStatus?.last_sync_attempt_at)}</strong></div>
                <div><span>Watched items</span><strong>{syncStatus?.watched_count ?? 0}</strong></div>
                <div><span>Rated items</span><strong>{syncStatus?.rated_count ?? 0}</strong></div>
                <div><span>Collection items</span><strong>{syncStatus?.collection_count ?? 0}</strong></div>
                <div><span>Tracked items</span><strong>{syncStatus?.total_items ?? 0}</strong></div>
            </div>
            {#if syncStatus?.last_error}<p class="sync-error">Last error: {syncStatus.last_error}</p>{/if}
        </section>
    {/if}
</div>

<style>
    .trakt-settings { display: flex; flex-direction: column; gap: 1.25rem; max-width: 820px; }
    .page-header, .card-header, .account-card { display: flex; align-items: flex-start; justify-content: space-between; gap: 1rem; }
    .back-link, .hint, small { color: var(--color-text-muted); font-size: 0.8125rem; }
    .page-title { margin: 0.25rem 0; color: var(--color-text-primary); font-size: 1.5rem; }
    .page-description, .card-header p, .account-card p, .connect-card p { margin: 0; color: var(--color-text-secondary); }
    .settings-card, .empty-state { border: 1px solid var(--color-border-subtle); border-radius: var(--radius-sm); background: var(--color-bg-surface); }
    .loading-state, .empty-state { display: grid; min-height: 160px; place-items: center; padding: 1rem; }
    .loading-spinner { width: 1.5rem; height: 1.5rem; border: 2px solid var(--color-border); border-top-color: var(--color-accent); border-radius: 50%; animation: spin 0.8s linear infinite; }
    .connect-card, .account-card { padding: 1rem; }
    .connect-card { display: flex; flex-direction: column; align-items: flex-start; gap: 1rem; }
    .connect-card h2, .account-card h2, .card-header h2 { margin: 0 0 0.25rem; color: var(--color-text-primary); font-size: 1rem; }
    .device-flow { display: flex; align-items: center; flex-wrap: wrap; gap: 0.75rem; }
    .user-code { border-radius: var(--radius-sm); padding: 0.5rem 0.75rem; letter-spacing: 0.08em; color: var(--color-text-primary); background: var(--color-bg-elevated); }
    .status-badge { flex: 0 0 auto; border-radius: 999px; padding: 0.25rem 0.5rem; color: var(--color-success); background: var(--color-bg-elevated); font-size: 0.75rem; }
    .card-header { padding: 1rem; border-bottom: 1px solid var(--color-border-subtle); }
    .card-body { padding: 0 1rem 1rem; }
    .toggle-list { display: flex; flex-direction: column; gap: 0.75rem; }
    .toggle-row { display: flex; align-items: flex-start; gap: 0.75rem; padding-top: 1rem; color: var(--color-text-primary); }
    .toggle-row input { margin-top: 0.2rem; }
    .toggle-row span { display: flex; flex-direction: column; gap: 0.125rem; }
    .toggle-row strong { font-size: 0.875rem; }
    .status-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 1px; background: var(--color-border-subtle); }
    .status-grid div { display: flex; flex-direction: column; gap: 0.25rem; padding: 0.875rem 1rem; background: var(--color-bg-surface); }
    .status-grid span { color: var(--color-text-muted); font-size: 0.75rem; }
    .status-grid strong { color: var(--color-text-primary); font-size: 0.875rem; }
    .sync-error, .error-text { margin: 1rem; color: var(--color-error); }
    .danger { color: var(--color-error); }
    @keyframes spin { to { transform: rotate(360deg); } }
    @media (max-width: 640px) { .page-header, .card-header, .account-card { flex-direction: column; } .status-grid { grid-template-columns: 1fr 1fr; } }
</style>
