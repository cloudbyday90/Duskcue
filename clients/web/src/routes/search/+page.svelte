<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import { goto } from '$app/navigation';
    import { page } from '$app/stores';
    import { search } from '$lib/api/search.js';
    import MediaCard from '$lib/components/MediaCard.svelte';

    let loading = $state(false);
    let results = $state([]);
    let facets = $state(emptyFacets());
    let error = $state(null);
    let searchRun = 0;

    let query = $derived($page.url.searchParams.get('q') || '');
    let typeFilter = $derived($page.url.searchParams.get('type') || '');
    let genreFilter = $derived($page.url.searchParams.get('genre') || '');
    let yearFilter = $derived($page.url.searchParams.get('year') || '');
    let ratingFilter = $derived($page.url.searchParams.get('rating_min') || '');
    let hasActiveFilters = $derived(Boolean(typeFilter || genreFilter || yearFilter || ratingFilter));

    const typeValues = ['', 'movie', 'series', 'season', 'episode'];
    const ratingValues = ['9', '8', '7', '6'];

    function emptyFacets() {
        return { types: [], genres: [], years: [], ratings: [] };
    }

    async function performSearch() {
        const run = ++searchRun;
        if (!query.trim()) {
            results = [];
            facets = emptyFacets();
            return;
        }
        loading = true;
        error = null;
        try {
            const params = {};
            if (typeFilter) params.type = typeFilter;
            if (genreFilter) params.genre = genreFilter;
            if (yearFilter) params.year = yearFilter;
            if (ratingFilter) params.rating_min = ratingFilter;
            const response = await search(query, params);
            if (run !== searchRun) return;
            results = response.items || [];
            facets = response.facets || emptyFacets();
        } catch (err) {
            if (run !== searchRun) return;
            error = err.detail || err.message || m.routes_search_page_search_failed();
            results = [];
            facets = emptyFacets();
        } finally {
            if (run === searchRun) loading = false;
        }
    }

    $effect(() => {
        if (query) {
            performSearch();
        } else {
            results = [];
        }
    });

    function updateFilter(key, value) {
        const url = new URL($page.url);
        if (value) {
            url.searchParams.set(key, value);
        } else {
            url.searchParams.delete(key);
        }
        goto(`${url.pathname}?${url.searchParams.toString()}`, {
            keepFocus: true,
            noScroll: true,
            replaceState: false,
        });
    }

    function clearFilters() {
        const url = new URL($page.url);
        for (const key of ['type', 'genre', 'year', 'rating_min']) {
            url.searchParams.delete(key);
        }
        goto(`${url.pathname}?${url.searchParams.toString()}`, {
            keepFocus: true,
            noScroll: true,
            replaceState: false,
        });
    }

    function mediaTypeLabel(type) {
        switch (type) {
            case 'movie':
                return m.routes_search_page_movies();
            case 'series':
                return m.routes_search_page_series();
            case 'season':
                return m.routes_search_page_seasons();
            case 'episode':
                return m.routes_search_page_episodes();
            default:
                return m.routes_search_page_all();
        }
    }

    function typeCount(type) {
        return facets.types.find((facet) => facet.value === type)?.count;
    }

    function ratingCount(rating) {
        return facets.ratings.find((facet) => facet.value === rating)?.count;
    }

    function ratingLabel(rating) {
        return m.routes_search_page_rating_plus({ rating });
    }
</script>

<div class="search-page">
    <div class="search-header">
        <h1 class="page-title">
            {#if query}
                {m.routes_search_page_results_for({ query })}
            {:else}
                {m.routes_search_page_search()}
            {/if}
        </h1>
    </div>

    {#if query}
        <div class="filters">
            <div class="filter-group">
                <span class="filter-label">{m.routes_search_page_type()}</span>
                <div class="filter-bar">
                    {#each typeValues as type}
                        <button
                            class="filter-chip"
                            class:active={typeFilter === type}
                            onclick={() => updateFilter('type', type)}
                        >
                            <span>{mediaTypeLabel(type)}</span>
                            {#if type && typeCount(type)}
                                <span class="filter-count">{typeCount(type)}</span>
                            {/if}
                        </button>
                    {/each}
                </div>
            </div>

            {#if facets.genres.length > 0 || genreFilter}
                <div class="filter-group">
                    <span class="filter-label">{m.routes_search_page_genre()}</span>
                    <div class="filter-bar">
                        {#if genreFilter}
                            <button class="filter-chip active" onclick={() => updateFilter('genre', '')}>
                                {m.routes_search_page_all_genres()}
                            </button>
                        {/if}
                        {#each facets.genres as genre}
                            <button
                                class="filter-chip"
                                class:active={genreFilter === genre.value}
                                onclick={() => updateFilter('genre', genreFilter === genre.value ? '' : genre.value)}
                            >
                                <span>{genre.label}</span>
                                <span class="filter-count">{genre.count}</span>
                            </button>
                        {/each}
                    </div>
                </div>
            {/if}

            {#if facets.years.length > 0 || yearFilter}
                <div class="filter-group">
                    <span class="filter-label">{m.routes_search_page_year()}</span>
                    <div class="filter-bar">
                        {#if yearFilter}
                            <button class="filter-chip active" onclick={() => updateFilter('year', '')}>
                                {m.routes_search_page_all_years()}
                            </button>
                        {/if}
                        {#each facets.years as year}
                            <button
                                class="filter-chip"
                                class:active={yearFilter === year.value}
                                onclick={() => updateFilter('year', yearFilter === year.value ? '' : year.value)}
                            >
                                <span>{year.label}</span>
                                <span class="filter-count">{year.count}</span>
                            </button>
                        {/each}
                    </div>
                </div>
            {/if}

            <div class="filter-group">
                <span class="filter-label">{m.routes_search_page_rating()}</span>
                <div class="filter-bar">
                    {#each ratingValues as rating}
                        <button
                            class="filter-chip"
                            class:active={ratingFilter === rating}
                            onclick={() => updateFilter('rating_min', ratingFilter === rating ? '' : rating)}
                        >
                            <span>{ratingLabel(rating)}</span>
                            {#if ratingCount(rating)}
                                <span class="filter-count">{ratingCount(rating)}</span>
                            {/if}
                        </button>
                    {/each}
                </div>
            </div>

            {#if hasActiveFilters}
                <button class="clear-filters" onclick={clearFilters}>
                    {m.routes_search_page_clear_filters()}
                </button>
            {/if}
        </div>
    {/if}

    {#if loading}
        <div class="loading-state">
            <div class="loading-spinner"></div>
            <p>{m.routes_search_page_searching()}</p>
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
            <p class="empty-title">{m.routes_search_page_no_results_found()}</p>
            <p class="empty-subtitle">{m.routes_search_page_try_different_keywords_or_remove_filters()}</p>
        </div>
    {:else}
        <div class="empty-state">
            <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="11" cy="11" r="8" />
                <line x1="21" y1="21" x2="16.65" y2="16.65" />
            </svg>
            <p class="empty-title">{m.routes_search_page_search_your_media_library()}</p>
            <p class="empty-subtitle">{m.routes_search_page_find_movies_tv_shows_and_more()}</p>
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

    .filters {
        display: flex;
        flex-direction: column;
        gap: 0.75rem;
        padding-block-end: 0.25rem;
    }

    .filter-group {
        display: grid;
        grid-template-columns: 4.5rem 1fr;
        align-items: start;
        gap: 0.625rem;
    }

    .filter-label {
        padding-top: 0.4375rem;
        color: var(--color-text-muted);
        font-size: 0.75rem;
        font-weight: 600;
    }

    .filter-chip {
        display: inline-flex;
        align-items: center;
        gap: 0.375rem;
        padding: 0.375rem 0.875rem;
        font-size: 0.75rem;
        font-weight: 500;
        color: var(--color-text-secondary);
        background-color: var(--color-bg-elevated);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
        transition: all var(--transition-fast);
    }

    .filter-count {
        color: var(--color-text-muted);
        font-size: 0.6875rem;
        font-variant-numeric: tabular-nums;
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

    .filter-chip.active .filter-count {
        color: var(--color-accent);
    }

    .clear-filters {
        align-self: flex-start;
        padding: 0.375rem 0.875rem;
        color: var(--color-text-secondary);
        background-color: transparent;
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
        font-size: 0.75rem;
        font-weight: 500;
    }

    .clear-filters:hover {
        color: var(--color-text-primary);
        border-color: var(--color-border);
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

    @media (max-width: 768px) {
        .results-grid {
            grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
            gap: 0.75rem;
        }

        .filter-bar {
            overflow-x: auto;
            flex-wrap: nowrap;
            padding-bottom: 0.25rem;
        }

        .filter-group {
            grid-template-columns: 1fr;
            gap: 0.375rem;
        }

        .filter-label {
            padding-top: 0;
        }

        .filter-chip {
            flex-shrink: 0;
        }

        .page-title {
            font-size: 1.25rem;
        }
    }
</style>
