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
    import { listBrowseLibraries } from '$lib/api/browsing.js';
    import { hasCapability } from '$lib/stores/auth.js';
    import { createCursorPager } from '$lib/browsing/cursor-pager.js';
    import { paginationDepth, browsingUrl, currentOrigin } from '$lib/browsing/query.js';
    import BrowseLibraryCards from '$lib/components/BrowseLibraryCards.svelte';
    import GalleryResults from '$lib/components/GalleryResults.svelte';

    const activeProfile = getContext('active-profile');
    const manageLibraries = hasCapability('can_manage_libraries');
    const pager = createCursorPager(listBrowseLibraries);
    let origin = $derived(currentOrigin($page.url));
    let canManage = $derived($manageLibraries && activeProfile?.()?.profile_type === 'standard');
    let statusText = $derived($pager.loading || $pager.loadingMore ? m.routes_libraries_page_loading_libraries() : m.tonight_browse_libraries_loaded({ count: $pager.items.length }));

    $effect(() => {
        const depth = paginationDepth($page.url.searchParams.get('pages'));
        untrack(() => pager.load({}, depth));
    });

    onDestroy(() => pager.dispose());

    async function loadMore(retry = false) {
        const before = origin;
        await (retry ? pager.retry() : pager.next());
        if (origin !== before || pager.getState().error || pager.getState().moreError) return;
        goto(browsingUrl($page.url, { pages: pager.getState().loadedPages }, false), { keepFocus: true, noScroll: true, replaceState: true });
    }
</script>

<div class="libraries-page">
    <header>
        <h1 class="tonight-heading">{m.routes_libraries_page_libraries()}</h1>
        {#if canManage}<a class="secondary-action" href="/settings/libraries">{m.routes_settings_libraries_page_library_management()}</a>{/if}
    </header>
    <GalleryResults state={$pager} {origin} {statusText} statusLocale={$pager.loading || $pager.loadingMore ? messageLocale('routes_libraries_page_loading_libraries') : messageLocale('tonight_browse_libraries_loaded')} onmore={() => loadMore()} onretry={() => loadMore(true)}
        emptyTitle={m.tonight_browse_libraries_empty()} emptyTitleLocale={messageLocale('tonight_browse_libraries_empty')} emptyHelp={m.tonight_browse_libraries_empty_help()} emptyHelpLocale={messageLocale('tonight_browse_libraries_empty_help')}
        errorTitle={m.tonight_browse_libraries_error()} errorLocale={messageLocale('tonight_browse_libraries_error')} moreErrorTitle={m.tonight_browse_libraries_more_error()} moreErrorLocale={messageLocale('tonight_browse_libraries_more_error')}>
        <BrowseLibraryCards items={$pager.items} {origin} />
    </GalleryResults>
</div>

<style>
    .libraries-page { display: flex; flex-direction: column; gap: 1.8rem; min-width: 0; }
    header { display: flex; flex-wrap: wrap; justify-content: space-between; align-items: center; gap: 1rem; }
    h1 { font-size: clamp(2.2rem, 5vw, 3.5rem); }
</style>
