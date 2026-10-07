<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->

<script>
    import { onDestroy } from 'svelte';
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    import { scanLibrary } from '$lib/api/libraries.js';

    let { libraryId, oncomplete } = $props();
    let scanning = $state(false);
    let error = $state(false);
    let complete = $state(false);
    let disposed = false;

    onDestroy(() => { disposed = true; });

    async function scan() {
        if (scanning) return;
        const id = libraryId;
        scanning = true;
        error = false;
        complete = false;
        try {
            await scanLibrary(id, { mode: 'full' });
            if (!disposed && libraryId === id) {
                complete = true;
                oncomplete();
            }
        } catch {
            if (!disposed && libraryId === id) error = true;
        } finally {
            if (!disposed && libraryId === id) scanning = false;
        }
    }
</script>

<div class="scan-action">
    <button lang={scanning ? messageLocale('tonight_browse_scanning') : messageLocale('tonight_browse_scan')} class="secondary-action" aria-disabled={scanning} onclick={scan}>{scanning ? m.tonight_browse_scanning() : m.tonight_browse_scan()}</button>
    <p lang={complete ? messageLocale('tonight_browse_scan_complete') : undefined} role="status" aria-live="polite">{complete ? m.tonight_browse_scan_complete() : ''}</p>
    {#if error}<p lang={messageLocale('tonight_browse_scan_error')} role="alert">{m.tonight_browse_scan_error()}</p>{/if}
</div>

<style>
    .scan-action { display: flex; flex-wrap: wrap; align-items: center; gap: 0.8rem; }
    p { color: var(--color-text-secondary); font-size: 0.85rem; }
    button[aria-disabled="true"] { opacity: 0.55; cursor: default; }
</style>
