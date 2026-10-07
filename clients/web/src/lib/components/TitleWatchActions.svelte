<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->

<script>
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    import { onDestroy } from 'svelte';
    import { updateWatchData } from '../api/playback.js';

    let { itemId, itemTitle = '', watch, loading = false, onchange, onretry } = $props();
    let busy = $state(false);
    let error = $state('');
    let status = $state('');
    let statusLocale = $state('en');
    let errorLocale = $state('en');
    let ratingDraft = $state('');
    let active = true;
    $effect(() => { ratingDraft = watch?.user_rating == null ? '' : String(watch.user_rating); });
    onDestroy(() => { active = false; });

    async function save(patch) {
        if (busy || loading || !watch) return;
        busy = true;
        error = '';
        status = m.tonight_title_saving();
        statusLocale = messageLocale('tonight_title_saving');
        const id = itemId;
        try {
            const updated = await updateWatchData(id, patch);
            if (!active || id !== itemId) return;
            onchange(updated);
            status = m.tonight_title_saved();
            statusLocale = messageLocale('tonight_title_saved');
        } catch (failure) {
            if (!active || failure.name === 'AbortError') return;
            error = failure.detail || m.tonight_title_save_failed();
            errorLocale = failure.detail ? 'en' : messageLocale('tonight_title_save_failed');
            ratingDraft = watch?.user_rating == null ? '' : String(watch.user_rating);
            status = '';
        } finally { if (active) busy = false; }
    }
</script>

<fieldset class="watch-actions" disabled={loading}>
    <legend class="visually-hidden">{itemTitle}</legend>
    {#if watch}
        <div class="controls">
            <button lang={messageLocale('tonight_title_favorite')} type="button" aria-pressed={watch.is_favorite === true} disabled={busy} onclick={() => save({ is_favorite: !watch.is_favorite })}>{m.tonight_title_favorite()}</button>
            <button lang={watch.is_watched ? messageLocale('tonight_title_mark_unwatched') : messageLocale('tonight_title_mark_watched')} type="button" disabled={busy} onclick={() => save({ is_watched: !watch.is_watched })}>{watch.is_watched ? m.tonight_title_mark_unwatched() : m.tonight_title_mark_watched()}</button>
            <div class="rating-control">
                <label lang={messageLocale('tonight_title_your_rating')} for={`title-rating-${itemId}`}>{m.tonight_title_your_rating()}</label>
                <select lang={messageLocale('tonight_title_your_rating')} id={`title-rating-${itemId}`} bind:value={ratingDraft} disabled={busy} onchange={(event) => save({ user_rating: event.currentTarget.value ? Number(event.currentTarget.value) : null })}>
                    <option lang={messageLocale('tonight_title_not_rated')} value="">{m.tonight_title_not_rated()}</option>
                    {#each Array.from({ length: 10 }, (_, index) => index + 1) as value}<option lang={messageLocale('tonight_title_rating_value')} value={String(value)}>{m.tonight_title_rating_value({ value })}</option>{/each}
                </select>
            </div>
        </div>
    {:else}
        <p lang={messageLocale('tonight_title_watch_failed')}>{m.tonight_title_watch_failed()}</p>
        <button lang={messageLocale('tonight_title_retry')} type="button" onclick={onretry}>{m.tonight_title_retry()}</button>
    {/if}
    <p lang={loading ? messageLocale('tonight_title_loading') : statusLocale} class="save-status" role="status" aria-atomic="true">{loading ? m.tonight_title_loading() : status}</p>
    {#if error}<p lang={errorLocale} role="alert">{error}</p>{/if}
</fieldset>

<style>
    .watch-actions { margin-top: 1rem; padding: 0; border: 0; min-width: 0; }
    .controls { display: flex; align-items: center; gap: .75rem; flex-wrap: wrap; }
    button, select { min-height: 44px; padding: .55rem .85rem; color: var(--color-text-primary); border: 1px solid var(--color-border); border-radius: var(--radius-sm); background: var(--color-bg-elevated); }
    button[aria-pressed="true"] { color: var(--color-accent); border-color: var(--color-accent); }
    .rating-control { display: flex; align-items: center; gap: .5rem; }
    .save-status { min-height: 1.3rem; margin-top: .35rem; color: var(--color-text-secondary); font-size: .85rem; }
    [role="alert"] { color: var(--color-error); }
</style>
