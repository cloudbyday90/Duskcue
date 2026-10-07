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
    import { getBrowseLibrary, getBrowseCollection, listBrowseCollectionItems } from '$lib/api/browsing.js';
    import { listMediaItems } from '$lib/api/media.js';
    import { hasCapability } from '$lib/stores/auth.js';
    import { createScopedResource } from '$lib/browsing/scoped-resource.js';
    import { createCursorPager } from '$lib/browsing/cursor-pager.js';
    import { catalogQuery, collectionQuery, collectionListOrigin, libraryListOrigin, paginationDepth, browsingUrl, currentOrigin } from '$lib/browsing/query.js';
    import BrowseFilters from './BrowseFilters.svelte';
    import GalleryResults from './GalleryResults.svelte';
    import LibraryScanAction from './LibraryScanAction.svelte';

    let { kind, itemId } = $props();
    const activeProfile = getContext('active-profile');
    const manageLibraries = hasCapability('can_manage_libraries');
    const resource = createScopedResource((id, options) => kind === 'library' ? getBrowseLibrary(id, options) : getBrowseCollection(id, options));
    const pager = createCursorPager((params, options) => {
        const { collection_id, ...query } = params;
        return collection_id ? listBrowseCollectionItems(collection_id, query, options) : listMediaItems(query, options);
    });
    let query = $derived(kind === 'library' ? catalogQuery($page.url.searchParams, { libraryId: itemId }) : collectionQuery($page.url.searchParams));
    let origin = $derived(currentOrigin($page.url));
    let back = $derived(kind === 'collection' ? collectionListOrigin($page.url.searchParams.get('from')) : libraryListOrigin($page.url.searchParams.get('from')));
    let canManage = $derived(kind === 'library' && $manageLibraries && activeProfile?.()?.profile_type === 'standard');

    $effect(() => {
        const id = itemId;
        untrack(() => resource.load(id));
    });

    $effect(() => {
        const params = kind === 'collection' ? { ...query, collection_id: itemId } : query;
        const depth = paginationDepth($page.url.searchParams.get('pages'));
        untrack(() => pager.load(params, depth));
    });

    onDestroy(() => { resource.dispose(); pager.dispose(); });

    function updateFilters(changes) {
        goto(browsingUrl($page.url, changes), { keepFocus: true, noScroll: true });
    }

    async function loadMore(retry = false) {
        const before = origin;
        await (retry ? pager.retry() : pager.next());
        if (origin !== before || pager.getState().error || pager.getState().moreError) return;
        goto(browsingUrl($page.url, { pages: pager.getState().loadedPages }, false), { keepFocus: true, noScroll: true, replaceState: true });
    }

    function reloadAfterScan() {
        pager.refresh();
    }
</script>

<div class="scope-page">
    <a lang={kind === 'collection' ? messageLocale('tonight_browse_back_collections') : messageLocale('routes_libraries_page_libraries')} class="back-link" href={back}>{kind === 'collection' ? m.tonight_browse_back_collections() : m.routes_libraries_page_libraries()}</a>
    <p lang={$resource.loading ? messageLocale('tonight_browse_loading') : undefined} role="status" aria-live="polite">{$resource.loading ? m.tonight_browse_loading() : ''}</p>
    {#if $resource.error}
        <div class="scope-error"><p lang={kind === 'library' ? messageLocale('tonight_browse_library_error') : messageLocale('tonight_browse_collection_error')} role="alert">{kind === 'library' ? m.tonight_browse_library_error() : m.tonight_browse_collection_error()}</p><button lang={messageLocale('tonight_browse_retry')} class="secondary-action" onclick={() => resource.retry()}>{m.tonight_browse_retry()}</button></div>
    {:else if $resource.item}
        <header>
            <h1 class="tonight-heading">{$resource.item.name}</h1>
            {#if kind === 'collection'}
                {#if $resource.item.description}<p>{$resource.item.description}</p>{/if}
                <p lang={messageLocale('tonight_browse_collection_count')} class="scope-count">{m.tonight_browse_collection_count({ count: $resource.item.item_count })}</p>
            {:else}<p lang={$resource.item.type === 'tvshows' ? messageLocale('tonight_browse_tv') : messageLocale('tonight_browse_movies')}>{$resource.item.type === 'tvshows' ? m.tonight_browse_tv() : m.tonight_browse_movies()}</p>{/if}
        </header>
        {#if canManage}{#key itemId}<LibraryScanAction libraryId={itemId} oncomplete={reloadAfterScan} />{/key}{/if}
        <BrowseFilters {query} mode={kind === 'collection' ? 'collection' : 'catalog'} onchange={updateFilters} />
        <GalleryResults state={$pager} {origin} onmore={() => loadMore()} onretry={() => loadMore(true)} />
    {/if}
</div>

<style>
    .scope-page { display: flex; flex-direction: column; gap: 1.4rem; min-width: 0; }
    header { display: flex; flex-direction: column; gap: 0.75rem; }
    h1 { font-size: clamp(2.2rem, 5vw, 3.5rem); overflow-wrap: anywhere; }
    header p, .scope-count, .back-link { color: var(--color-text-secondary); }
    .scope-count { font-size: 0.85rem; }
    .scope-error { display: flex; flex-wrap: wrap; align-items: center; gap: 1rem; }
</style>
