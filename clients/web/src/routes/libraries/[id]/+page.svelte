<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { onMount } from 'svelte';
    import { page } from '$app/stores';
    import { listLibraryItems } from '$lib/api/libraries.js';
    import { libraries } from '$lib/stores/libraries.js';
    import { notifications } from '$lib/stores/notifications.js';
    import { hasCapability } from '$lib/stores/auth.js';
    import MediaCard from '$lib/components/MediaCard.svelte';
    import { MEDIA_TYPE_LABELS } from '$lib/utils/constants.js';

    let libraryId = $derived($page.params.id);
    let loading = $state(true);
    let items = $state([]);
    let cursor = $state(null);
    let hasMore = $state(false);
    let loadingMore = $state(false);
    let typeFilter = $state('');
    let canManage = $state(false);

    $effect(() => {
        canManage = false;
        const unsub = hasCapability('can_manage_libraries').subscribe((v) => (canManage = v));
        return unsub;
    });

    onMount(async () => {
        await libraries.fetch();
        await loadItems();
        loading = false;
    });

    async function loadItems() {
        loading = true;
        try {
            const params = { limit: 24, order: 'desc' };
            if (typeFilter) params.type = typeFilter;
            const response = await listLibraryItems(libraryId, params);
            items = response.items || response || [];
            cursor = response.cursor || null;
            hasMore = response.has_more || false;
        } catch (err) {
            notifications.error(err.detail || err.message || 'Failed to load library');
            items = [];
        } finally {
            loading = false;
        }
    }

    async function loadMore() {
        if (!hasMore || loadingMore) return;
        loadingMore = true;
        try {
            const params = { limit: 24, order: 'desc' };
            if (cursor) params.cursor = cursor;
            if (typeFilter) params.type = typeFilter;
            const response = await listLibraryItems(libraryId, params);
            const newItems = response.items || response || [];
            items = [...items, ...newItems];
            cursor = response.cursor || null;
            hasMore = response.has_more || false;
        } catch (err) {
            notifications.error(err.detail || 'Failed to load more items');
        } finally {
            loadingMore = false;
        }
    }

    function changeFilter(type) {
        typeFilter = type;
        loadItems();
    }

    async function handleScan() {
        try {
            await libraries.scan(libraryId, 'full');
            notifications.success('Library scan complete');
            await loadItems();
        } catch (err) {
            notifications.error(err.detail || err.message || 'Scan failed');
        }
    }

    let currentLib = $derived(libraries.getById(libraryId));
</script>

<div class="library-detail">
    {#if loading}
        <div class="loading-state">
            <div class="loading-spinner"></div>
            <p>Loading library…</p>
        </div>
    {:else}
        <div class="library-header">
            <div class="header-left">
                <a href="/libraries" class="back-link">← Libraries</a>
                <h1 class="library-title">{currentLib?.name || 'Library'}</h1>
                {#if currentLib}
                    <span class="type-badge">
                        {MEDIA_TYPE_LABELS[currentLib.media_type] || currentLib.media_type}
                    </span>
                {/if}
            </div>
            <div class="header-right">
                {#if canManage}
                    <button class="btn-secondary" onclick={handleScan} disabled={libraries.isScanning(libraryId)}>
                        {libraries.isScanning(libraryId) ? 'Scanning…' : 'Scan Library'}
                    </button>
                {/if}
            </div>
        </div>

        <div class="filter-bar">
            {#each ['', 'movie', 'series', 'season', 'episode'] as type}
                <button
                    class="filter-chip"
                    class:active={typeFilter === type}
                    onclick={() => changeFilter(type)}
                >
                    {type || 'All'}
                </button>
            {/each}
        </div>

        {#if items.length > 0}
            <div class="media-grid">
                {#each items as item (item.id)}
                    <MediaCard {item} />
                {/each}
            </div>

            {#if hasMore}
                <div class="load-more">
                    <button class="btn-secondary" onclick={loadMore} disabled={loadingMore}>
                        {loadingMore ? 'Loading…' : 'Load More'}
                    </button>
                </div>
            {/if}
        {:else}
            <div class="empty-state">
                <p class="empty-title">No media in this library</p>
                <p class="empty-subtitle">
                    {#if canManage}
                        Run a scan to discover media files.
                    {:else}
                        Ask an administrator to scan this library.
                    {/if}
                </p>
            </div>
        {/if}
    {/if}
</div>

<style>
    .library-detail {
        display: flex;
        flex-direction: column;
        gap: 1.5rem;
    }

    .library-header {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 1rem;
        flex-wrap: wrap;
    }

    .header-left {
        display: flex;
        align-items: center;
        gap: 0.75rem;
        flex-wrap: wrap;
    }

    .back-link {
        font-size: 0.8125rem;
        color: var(--color-text-muted);
        transition: color var(--transition-fast);
    }

    .back-link:hover {
        color: var(--color-text-secondary);
    }

    .library-title {
        font-size: 1.5rem;
        font-weight: 700;
        color: var(--color-text-primary);
    }

    .type-badge {
        font-size: 0.6875rem;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.05em;
        color: var(--color-accent);
        background-color: var(--color-accent-muted);
        padding: 0.125rem 0.625rem;
        border-radius: var(--radius-sm);
    }

    .filter-bar {
        display: flex;
        gap: 0.375rem;
        flex-wrap: wrap;
    }

    .filter-chip {
        padding: 0.375rem 0.875rem;
        font-size: 0.75rem;
        font-weight: 500;
        color: var(--color-text-secondary);
        background-color: var(--color-bg-elevated);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
        text-transform: capitalize;
        transition: all var(--transition-fast);
    }

    .filter-chip:hover {
        color: var(--color-text-primary);
        border-color: var(--color-border);
    }

    .filter-chip.active {
        color: var(--color-accent);
        border-color: var(--color-accent);
        background-color: var(--color-accent-muted);
    }

    .media-grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
        gap: 1rem;
    }

    .load-more {
        display: flex;
        justify-content: center;
        padding: 1.5rem 0;
    }

    .btn-secondary {
        padding: 0.5rem 1.25rem;
        background-color: var(--color-bg-elevated);
        color: var(--color-text-primary);
        font-size: 0.8125rem;
        font-weight: 500;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        transition: border-color var(--transition-fast), background-color var(--transition-fast);
    }

    .btn-secondary:hover:not(:disabled) {
        border-color: var(--color-accent);
        background-color: var(--color-bg-hover);
    }

    .btn-secondary:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .loading-state {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: 1rem;
        padding: 4rem 0;
        color: var(--color-text-muted);
    }

    .loading-spinner {
        width: 32px;
        height: 32px;
        border: 3px solid var(--color-border);
        border-top-color: var(--color-accent);
        border-radius: 50%;
        animation: spin 0.8s linear infinite;
    }

    @keyframes spin {
        to {
            transform: rotate(360deg);
        }
    }

    .empty-state {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: 0.25rem;
        padding: 3rem 1rem;
        text-align: center;
    }

    .empty-title {
        font-size: 1rem;
        font-weight: 600;
        color: var(--color-text-secondary);
    }

    .empty-subtitle {
        font-size: 0.8125rem;
        color: var(--color-text-muted);
    }
</style>
