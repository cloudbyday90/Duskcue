<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import { onMount } from 'svelte';
    import { listMediaItems } from '$lib/api/media.js';
    import { notifications } from '$lib/stores/notifications.js';
    import MediaCard from '$lib/components/MediaCard.svelte';

    let loading = $state(true);
    let items = $state([]);
    let cursor = $state(null);
    let hasMore = $state(false);
    let loadingMore = $state(false);
    let typeFilter = $state('');

    onMount(async () => {
        await loadItems();
        loading = false;
    });

    async function loadItems() {
        loading = true;
        try {
            const params = { limit: 30, order: 'desc' };
            if (typeFilter) params.type = typeFilter;
            const response = await listMediaItems(params);
            items = response.items || response || [];
            cursor = response.cursor || null;
            hasMore = response.has_more || false;
        } catch (err) {
            notifications.error(err.detail || m.routes_media_page_failed_to_load_media());
            items = [];
        } finally {
            loading = false;
        }
    }

    async function loadMore() {
        if (!hasMore || loadingMore) return;
        loadingMore = true;
        try {
            const params = { limit: 30, order: 'desc' };
            if (cursor) params.cursor = cursor;
            if (typeFilter) params.type = typeFilter;
            const response = await listMediaItems(params);
            const newItems = response.items || response || [];
            items = [...items, ...newItems];
            cursor = response.cursor || null;
            hasMore = response.has_more || false;
        } catch {
            notifications.error(m.routes_media_page_failed_to_load_more());
        } finally {
            loadingMore = false;
        }
    }

    function changeFilter(type) {
        typeFilter = type;
        loadItems();
    }
</script>

<div class="media-page">
    <div class="page-header">
        <h1 class="page-title">{m.routes_media_page_all_media()}</h1>
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

    {#if loading}
        <div class="loading-state">
            <div class="loading-spinner"></div>
        </div>
    {:else if items.length > 0}
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
            <p class="empty-title">{m.routes_media_page_no_media_found()}</p>
            <p class="empty-subtitle">{m.routes_media_page_your_library_is_empty_or_no_items_match_the_filt()}</p>
        </div>
    {/if}
</div>

<style>
    .media-page {
        display: flex;
        flex-direction: column;
        gap: 1.5rem;
    }

    .page-title {
        font-size: 1.5rem;
        font-weight: 700;
        color: var(--color-text-primary);
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
        transition: all var(--transition-fast);
    }

    .btn-secondary:hover:not(:disabled) {
        border-color: var(--color-accent);
    }

    .btn-secondary:disabled {
        opacity: 0.5;
    }

    .loading-state {
        display: flex;
        align-items: center;
        justify-content: center;
        padding: 4rem 0;
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
        padding: 4rem 1rem;
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

    @media (max-width: 768px) {
        .media-grid {
            grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
            gap: 0.75rem;
        }

        .filter-bar {
            overflow-x: auto;
            flex-wrap: nowrap;
            padding-bottom: 0.25rem;
        }

        .filter-chip {
            flex-shrink: 0;
        }

        .page-title {
            font-size: 1.25rem;
        }
    }
</style>
