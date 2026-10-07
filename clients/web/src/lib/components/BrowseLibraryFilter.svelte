<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->

<script>
    import { onMount, onDestroy } from 'svelte';
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    import { getLocale } from '$lib/paraglide/runtime.js';
    import { listBrowseLibraries } from '$lib/api/browsing.js';
    import { createCursorPager } from '$lib/browsing/cursor-pager.js';

    let { value = '', onchange } = $props();
    const pager = createCursorPager(listBrowseLibraries, 100);
    let failed = $derived(!!($pager.error || $pager.moreError));

    onMount(() => { pager.loadAll(); });
    onDestroy(() => pager.dispose());
</script>

<div class="library-filter">
    <label lang={messageLocale('tonight_browse_library_label')} for="browse-library">{m.tonight_browse_library_label()}</label>
    <select lang={messageLocale('tonight_browse_library_label')} id="browse-library" {value} disabled={$pager.loading || $pager.loadingMore || failed}
        onchange={(event) => onchange(event.currentTarget.value)}>
        <option lang={messageLocale('tonight_browse_all_libraries')} value="">{m.tonight_browse_all_libraries()}</option>
        {#each $pager.items as library (library.id)}<option lang={getLocale()} value={library.id}>{library.name}</option>{/each}
    </select>
    <p role="status" aria-live="polite">{$pager.loading || $pager.loadingMore ? m.routes_libraries_page_loading_libraries() : ''}</p>
    {#if failed}
        <p lang={messageLocale('tonight_browse_library_error')} role="alert">{m.tonight_browse_library_error()}</p>
        <button lang={messageLocale('tonight_browse_retry')} class="secondary-action" onclick={() => pager.retry()}>{m.tonight_browse_retry()}</button>
    {/if}
</div>

<style>
    .library-filter { display: flex; flex-direction: column; gap: 0.45rem; min-width: 0; }
    label, p { color: var(--color-text-secondary); font-size: 0.8rem; }
    select { width: 100%; min-height: 44px; padding: 0.65rem 0.8rem; border-radius: var(--radius-md); border: 1px solid var(--color-border); background: var(--color-bg-surface); color: var(--color-text-primary); font: inherit; }
</style>
