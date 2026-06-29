<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { onMount } from 'svelte';
    import { listMigrationSources } from '$lib/api/migrations.js';
    import { hasCapability } from '$lib/stores/auth.js';

    let loading = $state(true);
    let canManage = $state(false);
    let loadError = $state(null);
    let migrations = $state([]);
    let selectedSource = $state('jellyfin');

    const sources = [
        { id: 'jellyfin', label: 'Jellyfin', detail: 'REST API' },
        { id: 'emby', label: 'Emby', detail: 'REST API' },
        { id: 'plex', label: 'Plex', detail: 'SQLite upload' },
    ];

    const steps = ['Source', 'Connect', 'Preflight', 'Users', 'Review', 'Import'];

    $effect(() => {
        const unsub = hasCapability('can_manage_users').subscribe((value) => (canManage = value));
        return unsub;
    });

    onMount(async () => {
        if (!canManage) {
            loading = false;
            return;
        }
        await load();
    });

    async function load() {
        loading = true;
        loadError = null;
        try {
            const response = await listMigrationSources({ page: 1, page_size: 25 });
            migrations = response.items || [];
        } catch (err) {
            if (err.title === 'MIGR_011') {
                migrations = [];
            } else {
                loadError = err.detail || err.message || 'Failed to load migrations';
            }
        } finally {
            loading = false;
        }
    }

    function formatDate(value) {
        if (!value) return 'Not run';
        return new Intl.DateTimeFormat(undefined, {
            dateStyle: 'medium',
            timeStyle: 'short',
        }).format(new Date(value));
    }
</script>

<div class="migration-settings">
    <div class="page-header">
        <div>
            <a href="/settings" class="back-link">← Settings</a>
            <h1 class="page-title">Platform Migration</h1>
        </div>
        {#if !loading && canManage}
            <button class="btn-secondary" onclick={load}>Refresh</button>
        {/if}
    </div>

    {#if loading}
        <div class="loading-state"><div class="loading-spinner"></div></div>
    {:else if !canManage}
        <div class="empty-state">You do not have permission to manage users.</div>
    {:else if loadError}
        <div class="empty-state">
            <p class="error-text">{loadError}</p>
            <button class="btn-secondary" onclick={load}>Retry</button>
        </div>
    {:else}
        <section class="wizard-shell">
            <div class="step-strip">
                {#each steps as step, index}
                    <div class="step" class:active={index === 0}>
                        <span>{index + 1}</span>
                        <strong>{step}</strong>
                    </div>
                {/each}
            </div>

            <div class="source-grid">
                {#each sources as source}
                    <button
                        class="source-tile"
                        class:selected={selectedSource === source.id}
                        onclick={() => (selectedSource = source.id)}
                    >
                        <span>{source.label}</span>
                        <strong>{source.detail}</strong>
                    </button>
                {/each}
            </div>

            <div class="connect-panel">
                <div>
                    <span class="panel-label">Selected Source</span>
                    <strong>{sources.find((source) => source.id === selectedSource)?.label}</strong>
                </div>
                <button class="btn-primary" disabled>Create Source</button>
            </div>
        </section>

        <section class="migration-list">
            <div class="section-header">
                <h2>Migration Sources</h2>
                <span>{migrations.length}</span>
            </div>

            {#if migrations.length === 0}
                <div class="empty-state">No migration sources have been created.</div>
            {:else}
                <div class="table">
                    <div class="table-row table-head">
                        <span>Name</span>
                        <span>Platform</span>
                        <span>Status</span>
                        <span>Last Run</span>
                    </div>
                    {#each migrations as migration}
                        <div class="table-row">
                            <span>{migration.name}</span>
                            <span>{migration.platform}</span>
                            <span>{migration.status}</span>
                            <span>{formatDate(migration.last_run_at)}</span>
                        </div>
                    {/each}
                </div>
            {/if}
        </section>
    {/if}
</div>

<style>
    .migration-settings {
        display: flex;
        flex-direction: column;
        gap: 1.5rem;
        max-width: 1100px;
    }

    .page-header {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 1rem;
    }

    .back-link {
        font-size: 0.8125rem;
        color: var(--color-text-muted);
    }

    .back-link:hover {
        color: var(--color-text-secondary);
    }

    .page-title {
        margin-top: 0.5rem;
        font-size: 1.75rem;
        font-weight: 700;
        color: var(--color-text-primary);
    }

    .wizard-shell,
    .migration-list {
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        background: var(--color-surface);
        padding: 1rem;
    }

    .step-strip {
        display: grid;
        grid-template-columns: repeat(6, minmax(0, 1fr));
        gap: 0.5rem;
        margin-bottom: 1rem;
    }

    .step {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        min-width: 0;
        color: var(--color-text-muted);
        font-size: 0.8125rem;
    }

    .step span {
        display: grid;
        place-items: center;
        width: 1.5rem;
        height: 1.5rem;
        flex: 0 0 auto;
        border-radius: 50%;
        border: 1px solid var(--color-border);
        font-size: 0.75rem;
    }

    .step strong {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .step.active {
        color: var(--color-text-primary);
    }

    .step.active span {
        border-color: var(--color-accent);
        background: var(--color-accent-muted);
        color: var(--color-accent);
    }

    .source-grid {
        display: grid;
        grid-template-columns: repeat(3, minmax(0, 1fr));
        gap: 0.75rem;
    }

    .source-tile {
        display: flex;
        flex-direction: column;
        align-items: flex-start;
        gap: 0.35rem;
        min-height: 5rem;
        padding: 1rem;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-background);
        color: var(--color-text-primary);
        cursor: pointer;
    }

    .source-tile.selected {
        border-color: var(--color-accent);
        box-shadow: 0 0 0 1px var(--color-accent);
    }

    .source-tile span {
        font-size: 1rem;
        font-weight: 700;
    }

    .source-tile strong {
        font-size: 0.8125rem;
        color: var(--color-text-secondary);
        font-weight: 500;
    }

    .connect-panel {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 1rem;
        margin-top: 1rem;
        padding-top: 1rem;
        border-top: 1px solid var(--color-border);
    }

    .connect-panel div {
        display: flex;
        flex-direction: column;
        gap: 0.25rem;
    }

    .panel-label {
        font-size: 0.75rem;
        color: var(--color-text-muted);
        text-transform: uppercase;
        letter-spacing: 0.04em;
    }

    .section-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        margin-bottom: 0.75rem;
    }

    .section-header h2 {
        font-size: 1rem;
        font-weight: 700;
        color: var(--color-text-primary);
    }

    .section-header span {
        font-size: 0.8125rem;
        color: var(--color-text-muted);
    }

    .table {
        display: grid;
        gap: 0.25rem;
    }

    .table-row {
        display: grid;
        grid-template-columns: 2fr 1fr 1fr 1.25fr;
        gap: 1rem;
        align-items: center;
        min-height: 2.5rem;
        padding: 0.5rem 0.75rem;
        border-radius: var(--radius-sm);
        background: var(--color-background);
        color: var(--color-text-secondary);
        font-size: 0.875rem;
    }

    .table-head {
        background: transparent;
        color: var(--color-text-muted);
        font-size: 0.75rem;
        font-weight: 700;
        text-transform: uppercase;
        letter-spacing: 0.04em;
    }

    .empty-state,
    .loading-state {
        display: flex;
        min-height: 8rem;
        align-items: center;
        justify-content: center;
        text-align: center;
        color: var(--color-text-muted);
    }

    .error-text {
        color: var(--color-danger);
    }

    @media (max-width: 760px) {
        .page-header,
        .connect-panel {
            flex-direction: column;
            align-items: stretch;
        }

        .step-strip,
        .source-grid,
        .table-row {
            grid-template-columns: 1fr;
        }

        .table-head {
            display: none;
        }
    }
</style>
