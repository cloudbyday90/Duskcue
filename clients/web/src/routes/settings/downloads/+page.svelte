<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { onMount } from 'svelte';
    import { listDownloadAdminInventory } from '$lib/api/settings.js';
    import { hasCapability } from '$lib/stores/auth.js';
    import { notifications } from '$lib/stores/notifications.js';

    let canManage = $state(false);
    let loading = $state(true);
    let loadError = $state('');
    let statusFilter = $state('');
    let deviceFilter = $state('');
    let inventory = $state({ items: [], summary: {} });

    $effect(() => {
        const unsub = hasCapability('can_manage_server').subscribe((value) => (canManage = value));
        return unsub;
    });

    onMount(async () => {
        if (!canManage) {
            loading = false;
            return;
        }
        await loadInventory();
    });

    async function loadInventory() {
        loading = true;
        loadError = '';
        try {
            inventory = await listDownloadAdminInventory({
                status: statusFilter || undefined,
                device_identifier: deviceFilter || undefined,
                limit: 100,
            });
        } catch (err) {
            loadError = err.detail || err.message || 'Failed to load download inventory';
            notifications.error(loadError);
        } finally {
            loading = false;
        }
    }

    function formatBytes(value) {
        const bytes = Number(value || 0);
        if (bytes >= 1024 ** 4) return `${(bytes / 1024 ** 4).toFixed(1)} TB`;
        if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
        if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
        if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
        return `${bytes} B`;
    }

    function formatDate(value) {
        if (!value) return '—';
        const date = new Date(value);
        if (Number.isNaN(date.getTime())) return '—';
        return date.toLocaleString();
    }

    function statusLabel(item) {
        return item.package_status === item.job_status
            ? item.package_status
            : `${item.package_status} / ${item.job_status}`;
    }
</script>

<div class="downloads-settings">
    <div class="page-header">
        <div>
            <a href="/settings" class="back-link">Settings</a>
            <h1 class="page-title">Downloads</h1>
        </div>
        {#if canManage}
            <button class="btn-primary" onclick={loadInventory} disabled={loading}>
                {loading ? 'Refreshing…' : 'Refresh'}
            </button>
        {/if}
    </div>

    {#if loading}
        <div class="loading-state"><div class="loading-spinner"></div></div>
    {:else if !canManage}
        <div class="empty-state">You do not have permission to manage download settings.</div>
    {:else if loadError}
        <div class="empty-state">
            <p class="error-text">{loadError}</p>
            <button class="btn-secondary" onclick={loadInventory}>Retry</button>
        </div>
    {:else}
        <section class="summary-grid">
            <div class="metric">
                <span class="metric-label">Packages</span>
                <span class="metric-value">{inventory.summary?.total_packages || 0}</span>
            </div>
            <div class="metric">
                <span class="metric-label">Storage</span>
                <span class="metric-value">{formatBytes(inventory.summary?.total_bytes)}</span>
            </div>
            <div class="metric">
                <span class="metric-label">Active Jobs</span>
                <span class="metric-value">{inventory.summary?.active_jobs || 0}</span>
            </div>
            <div class="metric">
                <span class="metric-label">Failures</span>
                <span class="metric-value">{inventory.summary?.failed_jobs || 0}</span>
            </div>
            <div class="metric">
                <span class="metric-label">Expired</span>
                <span class="metric-value">{inventory.summary?.expired_packages || 0}</span>
            </div>
            <div class="metric">
                <span class="metric-label">Revoked</span>
                <span class="metric-value">{inventory.summary?.revoked_packages || 0}</span>
            </div>
        </section>

        <section class="inventory-panel">
            <div class="panel-header">
                <div>
                    <h2 class="panel-title">Package Inventory</h2>
                    <p class="panel-desc">Recent user/device packages and cleanup-relevant state.</p>
                </div>
                <div class="filters">
                    <select bind:value={statusFilter} onchange={loadInventory} aria-label="Status filter">
                        <option value="">All statuses</option>
                        <option value="ready">Ready</option>
                        <option value="serving">Serving</option>
                        <option value="expired">Expired</option>
                        <option value="revoked">Revoked</option>
                        <option value="failed">Failed</option>
                        <option value="cleanup_pending">Cleanup pending</option>
                        <option value="cleaned">Cleaned</option>
                    </select>
                    <input
                        type="text"
                        bind:value={deviceFilter}
                        onkeydown={(event) => {
                            if (event.key === 'Enter') loadInventory();
                        }}
                        placeholder="Device identifier"
                        aria-label="Device identifier"
                    />
                </div>
            </div>

            {#if inventory.items.length === 0}
                <div class="empty-inline">No download packages match the current filters.</div>
            {:else}
                <div class="table-wrap">
                    <table>
                        <thead>
                            <tr>
                                <th>Media</th>
                                <th>User</th>
                                <th>Device</th>
                                <th>Status</th>
                                <th>Bytes</th>
                                <th>Expiry</th>
                                <th>Last Online</th>
                            </tr>
                        </thead>
                        <tbody>
                            {#each inventory.items as item}
                                <tr>
                                    <td>
                                        <div class="primary">{item.media_title || item.media_item_id}</div>
                                        {#if item.failure_reason}<div class="failure">{item.failure_reason}</div>{/if}
                                    </td>
                                    <td>{item.user_display_name || item.user_id || '—'}</td>
                                    <td><span class="mono">{item.device_identifier}</span></td>
                                    <td><span class="status-pill">{statusLabel(item)}</span></td>
                                    <td>
                                        <div>{formatBytes(item.bytes_downloaded)} / {formatBytes(item.total_bytes)}</div>
                                        <div class="muted">{item.files_verified || 0} files verified</div>
                                    </td>
                                    <td>{formatDate(item.expires_at)}</td>
                                    <td>{formatDate(item.last_online_check_at || item.last_served_at)}</td>
                                </tr>
                            {/each}
                        </tbody>
                    </table>
                </div>
            {/if}
        </section>
    {/if}
</div>

<style>
    .downloads-settings {
        display: flex;
        flex-direction: column;
        gap: 1.5rem;
        max-width: 1180px;
    }

    .page-header,
    .panel-header {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 1rem;
    }

    .back-link {
        font-size: 0.8125rem;
        color: var(--color-text-muted);
    }

    .page-title {
        margin-top: 0.25rem;
        font-size: 1.5rem;
        font-weight: 700;
        color: var(--color-text-primary);
    }

    .summary-grid {
        display: grid;
        grid-template-columns: repeat(6, minmax(0, 1fr));
        gap: 0.75rem;
    }

    .metric,
    .inventory-panel {
        background-color: var(--color-bg-surface);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-md);
    }

    .metric {
        padding: 0.875rem;
        min-width: 0;
    }

    .metric-label,
    .muted,
    .panel-desc {
        color: var(--color-text-muted);
    }

    .metric-label {
        display: block;
        font-size: 0.6875rem;
        text-transform: uppercase;
        letter-spacing: 0.04em;
    }

    .metric-value {
        display: block;
        margin-top: 0.375rem;
        font-size: 1.125rem;
        font-weight: 700;
        color: var(--color-text-primary);
    }

    .inventory-panel {
        overflow: hidden;
    }

    .panel-header {
        padding: 1rem 1.25rem;
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .panel-title {
        font-size: 1rem;
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .panel-desc {
        margin: 0.25rem 0 0;
        font-size: 0.75rem;
    }

    .filters {
        display: grid;
        grid-template-columns: 170px 230px;
        gap: 0.5rem;
    }

    .filters select,
    .filters input {
        width: 100%;
        min-width: 0;
        padding: 0.5rem 0.625rem;
        background-color: var(--color-bg-elevated);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
    }

    .table-wrap {
        overflow-x: auto;
    }

    table {
        width: 100%;
        border-collapse: collapse;
        min-width: 900px;
    }

    th,
    td {
        padding: 0.75rem 1rem;
        border-bottom: 1px solid var(--color-border-subtle);
        text-align: left;
        vertical-align: top;
        font-size: 0.8125rem;
    }

    th {
        color: var(--color-text-muted);
        font-weight: 600;
    }

    .primary {
        color: var(--color-text-primary);
        font-weight: 600;
    }

    .failure {
        margin-top: 0.25rem;
        color: var(--color-danger);
        font-size: 0.75rem;
    }

    .mono {
        font-family: var(--font-mono, monospace);
        font-size: 0.75rem;
    }

    .status-pill {
        display: inline-block;
        padding: 0.1875rem 0.5rem;
        border-radius: var(--radius-sm);
        background-color: var(--color-bg-elevated);
        color: var(--color-text-primary);
        border: 1px solid var(--color-border-subtle);
        white-space: nowrap;
    }

    .empty-state,
    .empty-inline,
    .loading-state {
        padding: 2rem;
        color: var(--color-text-muted);
        background-color: var(--color-bg-surface);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-md);
    }

    .empty-inline {
        margin: 1rem;
    }

    @media (max-width: 900px) {
        .summary-grid {
            grid-template-columns: repeat(2, minmax(0, 1fr));
        }

        .panel-header {
            flex-direction: column;
        }

        .filters {
            width: 100%;
            grid-template-columns: 1fr;
        }
    }
</style>
