<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { getConfigGroup, listDownloadAdminInventory, updateConfigGroup } from '$lib/api/settings.js';
    import { hasCapability } from '$lib/stores/auth.js';
    import { notifications } from '$lib/stores/notifications.js';
    import ConfigGroupForm from '$lib/components/ConfigGroupForm.svelte';
    import {
        cloneConfig,
        getConfigPath,
        hydrateConfigGroup,
        isConfigGroupDirty,
        serializeConfigGroup,
        updateConfigField,
    } from '$lib/admin/configForms.js';

    let canManage = $state(false);
    let loading = $state(true);
    let loadError = $state('');
    let statusFilter = $state('');
    let deviceFilter = $state('');
    let inventory = $state({ items: [], summary: {} });
    let loadedOnce = $state(false);
    let policy = $state({});
    let originalPolicy = $state({});
    let policyError = $state('');
    let savingPolicy = $state(false);

    const policyFields = [
        { path: 'enabled', label: 'Enable offline downloads', type: 'boolean', hint: '' },
        { path: 'max_quality_resolution', label: 'Max Quality Resolution', type: 'select', options: ['480p', '720p', '1080p', '1440p', '2160p'], hint: '' },
        { path: 'max_bytes_per_user', label: 'Max Bytes per User', type: 'number', min: 1073741824, max: 10995116277760, step: 1073741824, unit: 'bytes', nullable: false },
        { path: 'max_bytes_per_device', label: 'Max Bytes per Device', type: 'number', min: 1073741824, max: 5497558138880, step: 1073741824, unit: 'bytes', nullable: false },
        { path: 'max_active_jobs_per_user', label: 'Max Active Jobs per User', type: 'number', min: 1, max: 50, step: 1, unit: '', nullable: false },
        { path: 'max_active_jobs_per_device', label: 'Max Active Jobs per Device', type: 'number', min: 1, max: 25, step: 1, unit: '', nullable: false },
        { path: 'max_retained_packages_per_user', label: 'Max Retained Packages per User', type: 'number', min: 1, max: 500, step: 1, unit: '', nullable: false },
        { path: 'max_retained_packages_per_device', label: 'Max Retained Packages per Device', type: 'number', min: 1, max: 250, step: 1, unit: '', nullable: false },
        { path: 'allow_lan_downloads', label: 'Allow LAN Downloads', type: 'boolean', hint: '' },
        { path: 'allow_remote_downloads', label: 'Allow Remote Downloads', type: 'boolean', hint: '' },
        { path: 'allow_transcoded_downloads', label: 'Allow Transcoded Downloads', type: 'boolean', hint: '' },
        { path: 'default_package_expiry_days', label: 'Default Package Expiry', type: 'number', min: 1, max: 365, step: 1, unit: 'days', nullable: false },
        { path: 'ready_package_retention_days', label: 'Ready Package Retention', type: 'number', min: 1, max: 90, step: 1, unit: 'days', nullable: false },
        { path: 'user_overrides', label: 'User Overrides JSON', type: 'json', hint: 'Map user UUIDs to partial download policy overrides.' },
        { path: 'library_overrides', label: 'Library Overrides JSON', type: 'json', hint: 'Map library UUIDs to partial download policy overrides.' },
    ];

    let policyDirty = $derived(isConfigGroupDirty(policy, originalPolicy, policyFields));

    $effect(() => {
        const unsub = hasCapability('can_manage_server').subscribe((value) => (canManage = value));
        return unsub;
    });

    $effect(() => {
        if (!canManage) {
            loading = false;
            return;
        }
        if (loadedOnce) return;
        loadedOnce = true;
        load();
    });

    async function load() {
        loading = true;
        await Promise.all([loadInventory({ quiet: true }), loadPolicy()]);
        loading = false;
    }

    async function loadInventory(options = {}) {
        if (!options.quiet) loading = true;
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
            if (!options.quiet) loading = false;
        }
    }

    async function loadPolicy() {
        policyError = '';
        try {
            const response = await getConfigGroup('downloads');
            policy = hydrateConfigGroup(response.value, policyFields);
            originalPolicy = cloneConfig(policy);
        } catch (err) {
            policyError = err.detail || err.message || 'Failed to load download policy';
        }
    }

    function policyValue(field) {
        const value = getConfigPath(policy, field.path);
        if (field.type === 'boolean') return Boolean(value);
        return value === undefined || value === null ? '' : value;
    }

    function updatePolicyField(field, value) {
        policy = updateConfigField(policy, field, value);
    }

    async function savePolicy() {
        savingPolicy = true;
        policyError = '';
        try {
            const response = await updateConfigGroup('downloads', serializeConfigGroup(policy, policyFields));
            policy = hydrateConfigGroup(response.value, policyFields);
            originalPolicy = cloneConfig(policy);
            notifications.success('Download policy saved');
        } catch (err) {
            policyError = err.detail || err.message || 'Failed to save download policy';
            notifications.error(policyError);
        } finally {
            savingPolicy = false;
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
            <a href="/admin" class="back-link">Admin</a>
            <h1 class="page-title">Downloads</h1>
        </div>
        {#if canManage}
            <button class="btn-primary" onclick={load} disabled={loading}>
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
            <button class="btn-secondary" onclick={load}>Retry</button>
        </div>
    {:else}
        <section class="policy-panel">
            <div class="panel-header">
                <div>
                    <h2 class="panel-title">Download Policy</h2>
                    <p class="panel-desc">Control offline package eligibility, limits, retention, and network access.</p>
                </div>
                <button class="btn-primary" onclick={savePolicy} disabled={!policyDirty || savingPolicy}>
                    {savingPolicy ? 'Saving…' : 'Save Policy'}
                </button>
            </div>
            <div class="policy-body">
                {#if policyError}
                    <p class="error-text">{policyError}</p>
                {/if}
                <ConfigGroupForm fields={policyFields} valueFor={policyValue} onchange={updatePolicyField} />
            </div>
        </section>

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
    .inventory-panel,
    .policy-panel {
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

    .policy-panel {
        overflow: hidden;
    }

    .panel-header {
        padding: 1rem 1.25rem;
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .policy-body {
        padding: 1.25rem;
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
