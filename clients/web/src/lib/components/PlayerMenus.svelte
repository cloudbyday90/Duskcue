<script>
    import { onMount, onDestroy } from 'svelte';
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    import { getLocale } from '$lib/paraglide/runtime.js';
    import PlayerPopover from './PlayerPopover.svelte';

    let {
        episodes = [], audioChoices = [], subtitleChoices = [], qualityChoices = [], speedChoices = [],
        disabled = false, busy = false, trackStatus = '', onselect = (_kind, _value) => {}, onopenchange = (_open) => {},
    } = $props();
    const identity = $props.id();
    let wrapper = $state();
    let episodeTrigger = $state();
    let tracksTrigger = $state();
    let settingsTrigger = $state();
    let open = $state(null);

    function triggerFor(kind) {
        return kind === 'episodes' ? episodeTrigger : kind === 'tracks' ? tracksTrigger : settingsTrigger;
    }

    export function isOpen() {
        return open !== null;
    }

    export function close(restoreFocus = false) {
        if (!open) return false;
        const trigger = triggerFor(open);
        open = null;
        onopenchange(false);
        if (restoreFocus && trigger?.isConnected && !trigger.disabled) trigger.focus();
        return true;
    }

    function toggle(kind) {
        if (open === kind) close(false);
        else {
            open = kind;
            onopenchange(true);
        }
    }

    function choose(kind, value) {
        close(true);
        onselect(kind, value);
    }

    $effect(() => {
        if (open && (disabled
            || (open === 'episodes' && !episodes.length)
            || (open === 'tracks' && !audioChoices.length && !subtitleChoices.length)
            || (open === 'settings' && !qualityChoices.length && !speedChoices.length))) close(false);
    });

    onMount(() => {
        const outside = (event) => {
            if (open && !wrapper.contains(event.target)) close(false);
        };
        const escape = (event) => {
            if (!open || event.key !== 'Escape') return;
            event.preventDefault();
            event.stopPropagation();
            close(true);
        };
        document.addEventListener('pointerdown', outside, true);
        document.addEventListener('focusin', outside, true);
        document.addEventListener('keydown', escape, true);
        return () => {
            document.removeEventListener('pointerdown', outside, true);
            document.removeEventListener('focusin', outside, true);
            document.removeEventListener('keydown', escape, true);
        };
    });

    onDestroy(() => {
        if (open) onopenchange(false);
    });
</script>

<div class="player-menus" bind:this={wrapper} aria-busy={busy}>
    {#if episodes.length}
        <button lang={messageLocale('tonight_player_menu_episodes')} bind:this={episodeTrigger} type="button" class="trigger" aria-expanded={open === 'episodes'} aria-controls={`${identity}-episodes`} {disabled} onclick={() => toggle('episodes')}>
            {m.tonight_player_menu_episodes()}
        </button>
        <PlayerPopover id={`${identity}-episodes`} open={open === 'episodes'} label={m.tonight_player_menu_episodes()} labelLocale={messageLocale('tonight_player_menu_episodes')}>
            <div class="choices">
                {#each episodes as episode, index (episode.id)}
                    <button lang={episode.locale} type="button" class="choice episode" aria-label={episode.label} aria-pressed={episode.selected === true} aria-describedby={episode.synopsis ? `${identity}-episode-copy-${index}` : undefined} disabled={disabled || busy || episode.disabled} onclick={() => choose('episode', episode.id)}>
                        <span class="choice-label">{episode.label}</span>
                        <span class="selected-mark" aria-hidden="true">{episode.selected ? '✓' : ''}</span>
                        {#if episode.synopsis}<span lang={getLocale()} class="synopsis" id={`${identity}-episode-copy-${index}`}>{episode.synopsis}</span>{/if}
                    </button>
                {/each}
            </div>
        </PlayerPopover>
    {/if}

    {#if audioChoices.length || subtitleChoices.length}
        <button lang={messageLocale('tonight_player_menu_tracks')} bind:this={tracksTrigger} type="button" class="trigger" aria-expanded={open === 'tracks'} aria-controls={`${identity}-tracks`} {disabled} onclick={() => toggle('tracks')}>
            {m.tonight_player_menu_tracks()}
        </button>
        <PlayerPopover id={`${identity}-tracks`} open={open === 'tracks'} label={m.tonight_player_menu_tracks()} labelLocale={messageLocale('tonight_player_menu_tracks')}>
            {#if audioChoices.length}
                <fieldset><legend lang={messageLocale('tonight_player_menu_audio')}>{m.tonight_player_menu_audio()}</legend><div class="choices">
                    {#each audioChoices as choice (choice.value)}
                        <button lang={choice.locale} type="button" class="choice" aria-pressed={choice.selected === true} disabled={disabled || busy || choice.disabled} onclick={() => choose('audio', choice.value)}>
                            <span class="choice-label">{#if choice.parts}{#each choice.parts as part, index}{index ? ' · ' : ''}<span lang={part.lang}>{part.text}</span>{/each}{:else}{choice.label}{/if}</span><span class="selected-mark" aria-hidden="true">{choice.selected ? '✓' : ''}</span>
                        </button>
                    {/each}
                </div></fieldset>
            {/if}
            {#if subtitleChoices.length}
                <fieldset><legend lang={messageLocale('tonight_player_menu_subtitles')}>{m.tonight_player_menu_subtitles()}</legend><div class="choices">
                    {#each subtitleChoices as choice (choice.value)}
                        <button lang={choice.locale} type="button" class="choice" aria-pressed={choice.selected === true} disabled={disabled || busy || choice.disabled} onclick={() => choose('subtitle', choice.value)}>
                            <span class="choice-label">{#if choice.parts}{#each choice.parts as part, index}{index ? ' · ' : ''}<span lang={part.lang}>{part.text}</span>{/each}{:else}{choice.label}{/if}</span><span class="selected-mark" aria-hidden="true">{choice.selected ? '✓' : ''}</span>
                        </button>
                    {/each}
                </div></fieldset>
            {/if}
            {#if trackStatus}<p lang={messageLocale('tonight_player_track_fallback')} class="track-status" role="status" aria-live="polite">{trackStatus}</p>{/if}
        </PlayerPopover>
    {/if}

    {#if qualityChoices.length || speedChoices.length}
        <button lang={messageLocale('tonight_player_menu_settings')} bind:this={settingsTrigger} type="button" class="trigger" aria-expanded={open === 'settings'} aria-controls={`${identity}-settings`} {disabled} onclick={() => toggle('settings')}>
            {m.tonight_player_menu_settings()}
        </button>
        <PlayerPopover id={`${identity}-settings`} open={open === 'settings'} label={m.tonight_player_menu_settings()} labelLocale={messageLocale('tonight_player_menu_settings')}>
            {#if qualityChoices.length}
                <fieldset><legend lang={messageLocale('tonight_player_menu_quality')}>{m.tonight_player_menu_quality()}</legend><div class="choices">
                    {#each qualityChoices as choice (choice.value)}
                        <button lang={choice.locale} type="button" class="choice" aria-pressed={choice.selected === true} disabled={disabled || busy || choice.disabled} onclick={() => choose('quality', choice.value)}>
                            <span class="choice-label">{#if choice.parts}{#each choice.parts as part, index}{index ? ' · ' : ''}<span lang={part.lang}>{part.text}</span>{/each}{:else}{choice.label}{/if}</span><span class="selected-mark" aria-hidden="true">{choice.selected ? '✓' : ''}</span>
                        </button>
                    {/each}
                </div></fieldset>
            {/if}
            {#if speedChoices.length}
                <fieldset><legend lang={messageLocale('tonight_player_menu_speed')}>{m.tonight_player_menu_speed()}</legend><div class="choices">
                    {#each speedChoices as choice (choice.value)}
                        <button lang={choice.locale} type="button" class="choice" aria-pressed={choice.selected === true} disabled={disabled || busy || choice.disabled} onclick={() => choose('speed', choice.value)}>
                            <span class="choice-label">{#if choice.parts}{#each choice.parts as part, index}{index ? ' · ' : ''}<span lang={part.lang}>{part.text}</span>{/each}{:else}{choice.label}{/if}</span><span class="selected-mark" aria-hidden="true">{choice.selected ? '✓' : ''}</span>
                        </button>
                    {/each}
                </div></fieldset>
            {/if}
        </PlayerPopover>
    {/if}
</div>

<style>
    .player-menus { position: relative; display: flex; flex-wrap: wrap; align-items: center; gap: 0.25rem; max-inline-size: 100%; }
    .trigger { min-block-size: 44px; padding: 0.5rem 0.65rem; border-radius: var(--radius-sm); color: var(--color-text-primary); font-size: 0.875rem; line-height: 1.4; }
    .trigger[aria-expanded='true'], .trigger:hover, .choice:hover { background: var(--color-bg-hover); }
    .choices { display: grid; gap: 0.2rem; }
    .choice { display: grid; grid-template-columns: minmax(0, 1fr) 1.25rem; align-items: center; gap: 0.25rem 0.5rem; min-block-size: 44px; inline-size: 100%; padding: 0.65rem; text-align: start; border: 1px solid transparent; border-radius: var(--radius-sm); font-size: 0.875rem; line-height: 1.45; }
    .choice[aria-pressed='true'] { background: var(--color-accent-muted); border-color: var(--color-border); }
    .choice-label { overflow-wrap: anywhere; }
    .selected-mark { color: var(--color-accent); text-align: end; }
    .synopsis { grid-column: 1 / -1; color: var(--color-text-secondary); font-size: 0.8rem; line-height: 1.45; display: -webkit-box; -webkit-box-orient: vertical; -webkit-line-clamp: 2; line-clamp: 2; overflow: hidden; }
    fieldset { min-inline-size: 0; border: 0; margin-block: 0.6rem; }
    legend { padding-inline: 0.65rem; margin-block-end: 0.4rem; color: var(--color-text-secondary); font-size: 0.8rem; font-weight: 600; }
    .track-status { color: var(--color-text-secondary); font-size: 0.8rem; line-height: 1.5; margin: 0.65rem; }
    button:focus-visible { outline: 2px solid var(--color-accent); outline-offset: -2px; }
    button:disabled { opacity: 0.5; cursor: default; }
    @media (prefers-reduced-motion: reduce) { .player-menus { scroll-behavior: auto; } }
</style>
