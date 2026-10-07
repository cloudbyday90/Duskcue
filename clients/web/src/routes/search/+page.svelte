<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->

<script>
    import { untrack, onDestroy, getContext } from 'svelte';
    import { page } from '$app/stores';
    import { goto } from '$app/navigation';
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    import { search } from '$lib/api/search.js';
    import { createCursorPager } from '$lib/browsing/cursor-pager.js';
    import { searchQuery, paginationDepth, browsingUrl, currentOrigin } from '$lib/browsing/query.js';
    import BrowseFilters from '$lib/components/BrowseFilters.svelte';
    import SearchFacetFilters from '$lib/components/SearchFacetFilters.svelte';
    import GalleryResults from '$lib/components/GalleryResults.svelte';

    const activeProfile = getContext('active-profile');
    const pager = createCursorPager((params, options) => {
        const { q, ...filters } = params;
        return search(q, filters, options);
    });
    let query = $derived(searchQuery($page.url.searchParams));
    let origin = $derived(currentOrigin($page.url));
    let searchAllowed = $derived(activeProfile?.()?.allow_search !== false);
    let draft = $state('');
    let appliedQuery = null;
    let hasFilters = $derived(!!(query.type || query.genre || query.year || query.rating_min || query.watch || query.favorite !== undefined || query.sort !== 'relevance' || query.order !== 'desc'));

    $effect(() => {
        const value = query.q;
        untrack(() => {
            if (value !== appliedQuery) {
                draft = value;
                appliedQuery = value;
            }
        });
    });

    $effect(() => {
        const params = query;
        const allowed = searchAllowed;
        const depth = paginationDepth($page.url.searchParams.get('pages'));
        untrack(() => {
            if (params.q && allowed) pager.load(params, depth);
            else pager.dispose();
        });
    });

    onDestroy(() => pager.dispose());

    function submit(event) {
        event.preventDefault();
        goto(browsingUrl($page.url, { q: draft.trim() }), { keepFocus: true, noScroll: true });
    }

    function updateFilters(changes) {
        goto(browsingUrl($page.url, changes), { keepFocus: true, noScroll: true });
    }

    function clearFilters() {
        updateFilters(Object.fromEntries(['type', 'genre', 'year', 'rating_min', 'sort', 'order', 'watch', 'favorite'].map((key) => [key, ''])));
    }

    async function loadMore(retry = false) {
        const before = origin;
        await (retry ? pager.retry() : pager.next());
        if (origin !== before || pager.getState().error || pager.getState().moreError) return;
        goto(browsingUrl($page.url, { pages: pager.getState().loadedPages }, false), { keepFocus: true, noScroll: true, replaceState: true });
    }
</script>

<div class="search-page">
    <header><h1 lang={query.q ? messageLocale('tonight_browse_search_heading') : messageLocale('tonight_browse_search')} class="tonight-heading">{query.q ? m.tonight_browse_search_heading({ query: query.q }) : m.tonight_browse_search()}</h1></header>
    {#if !searchAllowed}
        <p lang={messageLocale('tonight_browse_search_unavailable')} role="status">{m.tonight_browse_search_unavailable()}</p>
    {:else}
        <form lang={messageLocale('tonight_browse_search_label')} role="search" aria-label={m.tonight_browse_search_label()} onsubmit={submit}>
            <label lang={messageLocale('tonight_browse_search_label')} for="browse-search-query">{m.tonight_browse_search_label()}</label>
            <div class="search-entry">
                <input lang={messageLocale('tonight_browse_search_label')} id="browse-search-query" name="q" type="search" bind:value={draft} autocomplete="off" aria-describedby="browse-search-hint" />
                <button lang={messageLocale('tonight_browse_search_submit')} class="primary-action" type="submit">{m.tonight_browse_search_submit()}</button>
            </div>
            <p lang={messageLocale('tonight_browse_search_hint')} id="browse-search-hint">{m.tonight_browse_search_hint()}</p>
        </form>
        {#if query.q}
            <BrowseFilters {query} mode="search" legend={m.tonight_browse_search_filters()} legendLocale={messageLocale('tonight_browse_search_filters')} typeFacets={$pager.facets?.types || []} onchange={updateFilters}>
                <SearchFacetFilters {query} facets={$pager.facets} onchange={updateFilters} />
            </BrowseFilters>
            {#if hasFilters}<button lang={messageLocale('tonight_browse_clear')} class="secondary-action clear-filters" onclick={clearFilters}>{m.tonight_browse_clear()}</button>{/if}
            <GalleryResults state={$pager} {origin} onmore={() => loadMore()} onretry={() => loadMore(true)}
                emptyTitle={m.tonight_browse_search_empty()} emptyTitleLocale={messageLocale('tonight_browse_search_empty')} emptyHelp={m.tonight_browse_search_empty_help()} emptyHelpLocale={messageLocale('tonight_browse_search_empty_help')} />
        {/if}
    {/if}
</div>

<style>
    .search-page { display: flex; flex-direction: column; gap: 1.8rem; min-width: 0; }
    h1 { font-size: clamp(2.2rem, 5vw, 3.5rem); overflow-wrap: anywhere; }
    form { display: flex; flex-direction: column; gap: 0.7rem; max-width: 48rem; }
    label { color: var(--color-text-secondary); font-size: 0.85rem; }
    .search-entry { display: flex; gap: 0.8rem; }
    input { width: 100%; min-width: 0; min-height: 44px; padding: 0.7rem 1rem; border: 1px solid var(--color-border); border-radius: var(--radius-md); background: var(--color-bg-surface); color: var(--color-text-primary); font: inherit; }
    form p { color: var(--color-text-secondary); font-size: 0.85rem; }
    .clear-filters { align-self: flex-start; }
    @media (max-width: 440px) { .search-entry { flex-direction: column; } }
</style>
