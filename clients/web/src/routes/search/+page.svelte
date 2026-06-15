<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { onMount } from 'svelte';
    import { page } from '$app/stores';
    import { search } from '$lib/api/search.js';
    import MediaCard from '$lib/components/MediaCard.svelte';

    let loading = $state(false);
    let results = $state([]);
    let error = $state(null);
    let typeFilter = $state('');

    let query = $derived($page.url.searchParams.get('q') || '');

    async function performSearch() {
        if (!query.trim()) {
            results = [];
            return;
        }
        loading = true;
        error = null;
        try {
            const params = {};
            if (typeFilter) params.type = typeFilter;
            const response = await search(query, params);
            results = response.items || response || [];
        } catch (err) {
            error = err.detail || err.message || 'Search failed';
            results = [];
        } finally {
            loading = false;
        }
    }

    $effect(() => {
        if (query) {
            performSearch();
        } else {
            results = [];
        }
    });

    function changeFilter(type) {
        typeFilter = type;
        performSearch();
    }
</script>

<div class="search-page">
    <div class="search-header">
        <h1 class="page-title">
            {#if query}
                Results for "{query}"
            {:else}
                Search
            {/if}
        </h1>
    </div>

    {#if query}
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
    {/if}

    {#if loading}
        <div class="loading-state">
            <div class="loading-spinner"></div>
            <p>Searching…</p>
        </div>
    {:else if error}
        <div class="error-state">{error}</div>
    {:else if query && results.length > 0}
        <div class="results-grid">
            {#each results as item (item.id)}
                <MediaCard {item} />
            {/each}
        </div>
    {:else if query}
        <div class="empty-state">
            <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="11" cy="11" r="8" />
                <line x1="21" y1="21" x2="16.65" y2="16.65" />
            </svg>
            <p class="empty-title">No results found</p>
            <p class="empty-subtitle">Try different keywords or remove filters.</p>
        </div>
    {:else}
        <div class="empty-state">
            <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="11" cy="11" r="8" />
                <line x1="21" y1="21" x2="16.65" y2="16.65" />
            </svg>
            <p class="empty-title">Search your media library</p>
            <p class="empty-subtitle">Find movies, TV shows, and more.</p>
        </div>
    {/if}
</div>

<style>
    .search-page {
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

    .results-grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
        gap: 1rem;
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

    .error-state {
        padding: 0.75rem 1rem;
        background-color: var(--color-error-bg);
        color: var(--color-error);
        font-size: 0.875rem;
        border-radius: var(--radius-sm);
    }

    .empty-state {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: 0.5rem;
        padding: 4rem 1rem;
        text-align: center;
    }

    .empty-title {
        font-size: 1rem;
        font-weight: 600;
        color: var(--color-text-secondary);
        margin-top: 0.5rem;
    }

    .empty-subtitle {
        font-size: 0.8125rem;
        color: var(--color-text-muted);
    }
</style>
