<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->

<script>
    import { untrack, onDestroy } from 'svelte';
    import { page } from '$app/stores';
    import { goto } from '$app/navigation';
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    import { listBrowseCollections } from '$lib/api/browsing.js';
    import { createCursorPager } from '$lib/browsing/cursor-pager.js';
    import { paginationDepth, browsingUrl, currentOrigin } from '$lib/browsing/query.js';
    import BrowseCollectionCards from '$lib/components/BrowseCollectionCards.svelte';
    import GalleryResults from '$lib/components/GalleryResults.svelte';

    const pager = createCursorPager(listBrowseCollections);
    let origin = $derived(currentOrigin($page.url));

    $effect(() => {
        const depth = paginationDepth($page.url.searchParams.get('pages'));
        untrack(() => pager.load({}, depth));
    });

    onDestroy(() => pager.dispose());

    async function loadMore(retry = false) {
        const before = origin;
        await (retry ? pager.retry() : pager.next());
        if (origin !== before || pager.getState().error || pager.getState().moreError) return;
        goto(browsingUrl($page.url, { pages: pager.getState().loadedPages }, false), {
            keepFocus: true, noScroll: true, replaceState: true,
        });
    }
</script>

<div class="collections-page">
    <header><h1 lang={messageLocale('tonight_browse_collections')} class="tonight-heading">{m.tonight_browse_collections()}</h1><p lang={messageLocale('tonight_browse_collection_description')}>{m.tonight_browse_collection_description()}</p></header>
    <GalleryResults state={$pager} {origin} onmore={() => loadMore()} onretry={() => loadMore(true)}
        collections={true} emptyTitle={m.tonight_browse_collection_empty()} emptyTitleLocale={messageLocale('tonight_browse_collection_empty')} emptyHelp={m.tonight_browse_collection_empty_help()} emptyHelpLocale={messageLocale('tonight_browse_collection_empty_help')}>
        <BrowseCollectionCards items={$pager.items} {origin} />
    </GalleryResults>
</div>

<style>
    .collections-page { display: flex; flex-direction: column; gap: 1.8rem; min-width: 0; }
    header { display: flex; flex-direction: column; gap: 0.8rem; }
    h1 { font-size: clamp(2.2rem, 5vw, 3.5rem); }
    header p { color: var(--color-text-secondary); }
</style>
