<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->

<script>
    import { tick } from 'svelte';
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    import { player } from '$lib/stores/player.js';

    let release = $derived($player.release);
    let phase = $derived(release?.phase || 'idle');
    let retryButton = $state();

    async function retry(event) {
        const heldFocus = document.activeElement === event.currentTarget;
        try { await player.stop({ retry: true }); } catch {}
        await tick();
        if (heldFocus && document.activeElement === document.body) (retryButton || document.getElementById('main-content'))?.focus();
    }
</script>

<div class="release-notice" class:visible={phase !== 'idle'}>
    <p lang={phase === 'failed' ? messageLocale('tonight_playback_release_failed') : phase === 'releasing' ? messageLocale('tonight_playback_release_pending') : undefined} role="status" aria-live="polite" aria-atomic="true">{phase === 'failed' ? m.tonight_playback_release_failed() : phase === 'releasing' ? m.tonight_playback_release_pending() : ''}</p>
    {#if phase === 'failed'}
        <button lang={messageLocale('tonight_playback_release_retry')} type="button" class="secondary-action" bind:this={retryButton} onclick={retry}>{m.tonight_playback_release_retry()}</button>
    {/if}
</div>

<style>
    .release-notice.visible { display: flex; flex-wrap: wrap; align-items: center; gap: 0.75rem; padding: 1rem; inline-size: calc(100% - var(--space-page) - var(--space-page)); max-inline-size: 1600px; margin: 1rem auto 0; border: 1px solid var(--color-border); border-radius: var(--radius-md); background: var(--color-bg-surface); }
    p { flex: 1 1 16rem; min-inline-size: 0; overflow-wrap: anywhere; color: var(--color-text-secondary); }
    button { min-block-size: 44px; max-inline-size: 100%; white-space: normal; }
</style>
