<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->

<script>
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    import PosterGallery from './PosterGallery.svelte';

    let { state, origin, onmore, onretry, emptyTitle = null, emptyHelp = null, children = null, collections = false, statusText = null, errorTitle = null, moreErrorTitle = null, statusLocale = undefined, errorLocale = undefined, moreErrorLocale = undefined, emptyTitleLocale = undefined, emptyHelpLocale = undefined } = $props();
</script>

<div class="gallery-results" aria-busy={state.loading || state.loadingMore}>
    <p lang={statusText ? statusLocale : state.loading ? messageLocale('tonight_browse_loading') : state.loadingMore ? messageLocale('tonight_browse_loading_more') : collections ? messageLocale('tonight_browse_collections_loaded') : messageLocale('tonight_browse_loaded')} class="result-status" role="status" aria-live="polite" aria-atomic="true">
        {statusText || (state.loading ? m.tonight_browse_loading() : state.loadingMore ? m.tonight_browse_loading_more() : collections ? m.tonight_browse_collections_loaded({ count: state.items.length }) : m.tonight_browse_loaded({ count: state.items.length }))}
    </p>
    {#if state.error}
        <div class="result-state">
            <p lang={errorTitle ? errorLocale : messageLocale('tonight_browse_error')} role="alert">{errorTitle || m.tonight_browse_error()}</p>
            <button lang={messageLocale('tonight_browse_retry')} class="secondary-action" onclick={onretry}>{m.tonight_browse_retry()}</button>
        </div>
    {:else if !state.loading && !state.items.length}
        <div class="result-state">
            <h2 lang={emptyTitle ? emptyTitleLocale : messageLocale('tonight_browse_empty')}>{emptyTitle || m.tonight_browse_empty()}</h2>
            <p lang={emptyHelp ? emptyHelpLocale : messageLocale('tonight_browse_empty_help')}>{emptyHelp || m.tonight_browse_empty_help()}</p>
        </div>
    {:else}
        {#if children}{@render children()}{:else}<PosterGallery items={state.items} {origin} />{/if}
    {/if}
    {#if state.moreError}
        <div class="more-error">
            <p lang={moreErrorTitle ? moreErrorLocale : messageLocale('tonight_browse_more_error')} role="alert">{moreErrorTitle || m.tonight_browse_more_error()}</p>
            <button lang={messageLocale('tonight_browse_retry')} class="secondary-action" onclick={onretry}>{m.tonight_browse_retry()}</button>
        </div>
    {/if}
    {#if state.hasMore || state.loadedPages > 1}
        <button lang={state.loadingMore ? messageLocale('tonight_browse_loading_more') : state.hasMore ? messageLocale('tonight_browse_load_more') : messageLocale('tonight_browse_all_loaded')} class="secondary-action load-more" aria-disabled={state.loadingMore || !state.hasMore}
            onclick={() => { if (!state.loadingMore && state.hasMore) onmore(); }}>
            {state.loadingMore ? m.tonight_browse_loading_more() : state.hasMore ? m.tonight_browse_load_more() : m.tonight_browse_all_loaded()}
        </button>
    {/if}
</div>

<style>
    .gallery-results { display: flex; flex-direction: column; gap: 1.1rem; min-width: 0; }
    .result-status { color: var(--color-text-secondary); font-size: 0.8rem; min-height: 1.4em; }
    .result-state { display: flex; flex-direction: column; align-items: flex-start; gap: 1rem; padding: 2.2rem 1.5rem; background: var(--color-bg-surface); border: 1px solid var(--color-border); border-radius: var(--radius-lg); }
    .result-state h2 { font-size: 1.1rem; }
    .result-state p { color: var(--color-text-secondary); }
    .more-error { display: flex; align-items: center; flex-wrap: wrap; gap: 1rem; color: var(--color-text-secondary); }
    .load-more { align-self: center; }
    button[aria-disabled="true"] { opacity: 0.55; cursor: default; }
</style>
