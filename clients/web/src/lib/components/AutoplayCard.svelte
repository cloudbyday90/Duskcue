<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->

<script>
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    import { onDestroy, tick } from 'svelte';

    let { state: autoplayState, onplaynext = () => {}, oncancel = () => {}, onretry = () => {}, onfocuschange = (_focused) => {} } = $props();
    let card = $state();
    let playButton = $state();
    let retryButton = $state();
    let heading = $state();
    let focused = $state(false);
    let announcement = $state('');
    let announcementLocale = $state('en');
    let announcementKey;
    let active = true;
    let phase = $derived(autoplayState?.phase ?? 'idle');
    let nextTitle = $derived(autoplayState?.nextEpisode?.title ?? '');
    let visible = $derived(['loading', 'countdown', 'ready', 'unavailable', 'unordered', 'error', 'transitioning'].includes(phase));

    function announcementText(code, title) {
        switch (code) {
            case 'next_loading': return m.tonight_autoplay_loading();
            case 'autoplay_countdown': return m.tonight_autoplay_countdown_status({ title });
            case 'next_ready': return m.tonight_autoplay_ready_status({ title });
            case 'autoplay_paused': return m.tonight_autoplay_paused_status();
            case 'autoplay_cancelled': return m.tonight_autoplay_cancelled_status({ title });
            case 'next_unavailable': return m.tonight_autoplay_unavailable();
            case 'next_failed': return m.tonight_autoplay_failed();
            case 'next_unordered': return m.tonight_autoplay_unordered();
            case 'next_starting': return m.tonight_autoplay_starting({ title });
            case 'next_started': return m.tonight_autoplay_started({ title });
            default: return '';
        }
    }

    $effect(() => {
        const code = autoplayState?.announcement;
        const title = nextTitle;
        const key = `${code ?? ''}\u0000${title}`;
        if (key === announcementKey) return;
        announcementKey = key;
        announcement = announcementText(code, title);
        const messages = { next_loading: 'tonight_autoplay_loading', autoplay_countdown: 'tonight_autoplay_countdown_status', next_ready: 'tonight_autoplay_ready_status', autoplay_paused: 'tonight_autoplay_paused_status', autoplay_cancelled: 'tonight_autoplay_cancelled_status', next_unavailable: 'tonight_autoplay_unavailable', next_failed: 'tonight_autoplay_failed', next_unordered: 'tonight_autoplay_unordered', next_starting: 'tonight_autoplay_starting', next_started: 'tonight_autoplay_started' };
        announcementLocale = messages[code] ? messageLocale(messages[code]) : undefined;
    });

    $effect(() => { if (!visible && focused) reportFocus(false); });

    $effect(() => {
        const currentPhase = phase;
        tick().then(() => { if (active && phase === currentPhase && focused && !card?.contains(document.activeElement)) reportFocus(false); });
    });

    function reportFocus(value) {
        if (focused === value) return;
        focused = value;
        onfocuschange(value);
    }

    function leaveFocus(event) {
        if (!(event.relatedTarget instanceof Node && event.currentTarget.contains(event.relatedTarget))) reportFocus(false);
    }

    async function recoverActionFocus(event, action) {
        const heldFocus = document.activeElement === event.currentTarget;
        try { await action(); }
        finally {
            await tick();
            if (active && heldFocus && document.activeElement === document.body) (playButton || retryButton || heading)?.focus({ preventScroll: true });
        }
    }

    onDestroy(() => { active = false; reportFocus(false); });
</script>

<p lang={announcementLocale} class="autoplay-status visually-hidden" role="status" aria-live="polite" aria-atomic="true">{announcement}</p>
{#if visible}
    <section class="autoplay-card" bind:this={card} aria-labelledby="autoplay-next-heading" aria-busy={phase === 'loading' || phase === 'transitioning'} onfocusin={() => reportFocus(true)} onfocusout={leaveFocus}>
        {#if nextTitle}<p lang={messageLocale('tonight_autoplay_up_next')} class="eyebrow">{m.tonight_autoplay_up_next()}</p>{/if}
        <h2 lang={nextTitle ? undefined : messageLocale('tonight_autoplay_up_next')} id="autoplay-next-heading" bind:this={heading} tabindex="-1">{nextTitle || m.tonight_autoplay_up_next()}</h2>
        {#if phase === 'loading'}
            <p lang={messageLocale('tonight_autoplay_loading')}>{m.tonight_autoplay_loading()}</p>
        {:else if phase === 'countdown'}
            <p lang={autoplayState.paused ? messageLocale('tonight_autoplay_paused_seconds') : messageLocale('tonight_autoplay_seconds')} class="countdown" aria-hidden="true">{autoplayState.paused ? m.tonight_autoplay_paused_seconds({ seconds: autoplayState.seconds }) : m.tonight_autoplay_seconds({ seconds: autoplayState.seconds })}</p>
        {:else if phase === 'ready'}
            <p lang={messageLocale('tonight_autoplay_untimed')}>{m.tonight_autoplay_untimed()}</p>
        {:else if phase === 'unavailable'}
            <p lang={messageLocale('tonight_autoplay_unavailable')}>{m.tonight_autoplay_unavailable()}</p>
        {:else if phase === 'error'}
            <p lang={autoplayState.error?.detail ? 'en' : messageLocale('tonight_autoplay_failed')}>{autoplayState.error?.detail || m.tonight_autoplay_failed()}</p>
        {:else if phase === 'unordered'}
            <p lang={messageLocale('tonight_autoplay_unordered')}>{m.tonight_autoplay_unordered()}</p>
        {:else if phase === 'transitioning'}
            <p lang={messageLocale('tonight_autoplay_starting')}>{m.tonight_autoplay_starting({ title: nextTitle })}</p>
        {/if}
        <div class="actions">
            {#if phase === 'countdown' || phase === 'ready'}
                <button lang={phase === 'countdown' ? messageLocale('tonight_autoplay_play_now') : messageLocale('tonight_autoplay_play_next')} class="play-next" type="button" bind:this={playButton} onclick={() => onplaynext()}>{phase === 'countdown' ? m.tonight_autoplay_play_now() : m.tonight_autoplay_play_next()}</button>
                {#if phase === 'countdown'}<button lang={messageLocale('tonight_autoplay_cancel')} type="button" onclick={(event) => recoverActionFocus(event, oncancel)}>{m.tonight_autoplay_cancel()}</button>{/if}
            {:else if phase === 'error' || phase === 'unavailable'}
                <button lang={messageLocale('tonight_autoplay_retry')} type="button" bind:this={retryButton} onclick={(event) => recoverActionFocus(event, onretry)}>{m.tonight_autoplay_retry()}</button>
            {/if}
        </div>
    </section>
{/if}

<style>
    .autoplay-card { inline-size: 100%; max-inline-size: 380px; min-inline-size: 0; padding: 1.25rem; border: 1px solid var(--color-border); border-radius: var(--radius-lg); background: var(--color-bg-surface); color: var(--color-text-primary); box-shadow: var(--shadow-elevated); overflow-wrap: anywhere; }
    .eyebrow { color: var(--color-accent); font-size: .8rem; margin-block-end: .4rem; }
    h2 { font-size: 1.4rem; line-height: 1.3; }
    p { color: var(--color-text-secondary); line-height: 1.5; }
    h2 + p { margin-block-start: .65rem; }
    .actions { display: flex; gap: .75rem; flex-wrap: wrap; margin-block-start: 1rem; }
    .actions:empty { display: none; }
    button { min-block-size: 44px; min-inline-size: 44px; max-inline-size: 100%; padding: .65rem 1rem; border: 1px solid var(--color-border); border-radius: var(--radius-sm); color: var(--color-text-primary); background: var(--color-bg-elevated); font: inherit; overflow-wrap: anywhere; }
    .play-next { color: var(--color-on-accent); background: var(--color-accent); border-color: var(--color-accent); }
    button:focus-visible, h2:focus-visible { outline: 2px solid var(--color-accent); outline-offset: 3px; }
</style>
