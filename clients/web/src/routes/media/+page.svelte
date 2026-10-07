<script>
    import { untrack, onDestroy } from 'svelte';
    import { page } from '$app/stores';
    import { goto } from '$app/navigation';
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    import { listMediaItems } from '$lib/api/media.js';
    import { createCursorPager } from '$lib/browsing/cursor-pager.js';
    import { catalogQuery, paginationDepth, browsingUrl, currentOrigin } from '$lib/browsing/query.js';
    import BrowseFilters from '$lib/components/BrowseFilters.svelte';
    import BrowseLibraryFilter from '$lib/components/BrowseLibraryFilter.svelte';
    import GalleryResults from '$lib/components/GalleryResults.svelte';

    const pager = createCursorPager(listMediaItems);
    let query = $derived(catalogQuery($page.url.searchParams));
    let origin = $derived(currentOrigin($page.url));
    let heading = $derived(query.type === 'movie' ? m.tonight_browse_movies() : query.type === 'series' ? m.tonight_browse_tv() : m.tonight_browse_library());

    $effect(() => {
        const params = query;
        const depth = paginationDepth($page.url.searchParams.get('pages'));
        untrack(() => pager.load(params, depth));
    });

    onDestroy(() => pager.dispose());

    function updateFilters(changes) {
        goto(browsingUrl($page.url, changes), { keepFocus: true, noScroll: true });
    }

    async function loadMore(retry = false) {
        const before = origin;
        await (retry ? pager.retry() : pager.next());
        if (origin !== before || pager.getState().error || pager.getState().moreError) return;
        goto(browsingUrl($page.url, { pages: pager.getState().loadedPages }, false), {
            keepFocus: true, noScroll: true, replaceState: true,
        });
    }
</script>

<div class="catalog-page">
    <header><h1 lang={query.type === 'movie' ? messageLocale('tonight_browse_movies') : query.type === 'series' ? messageLocale('tonight_browse_tv') : messageLocale('tonight_browse_library')} class="tonight-heading">{heading}</h1></header>
    <BrowseFilters {query} onchange={updateFilters}>
        <BrowseLibraryFilter value={query.library_id || ''} onchange={(library_id) => updateFilters({ library_id })} />
    </BrowseFilters>
    <GalleryResults state={$pager} {origin} onmore={() => loadMore()} onretry={() => loadMore(true)} />
</div>

<style>
    .catalog-page { display: flex; flex-direction: column; gap: 1.8rem; min-width: 0; }
    h1 { font-size: clamp(2.2rem, 5vw, 3.5rem); }
</style>
