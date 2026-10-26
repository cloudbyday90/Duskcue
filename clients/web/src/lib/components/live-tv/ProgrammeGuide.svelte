<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import { formatInstant, deviceTimeZone } from '$lib/live-tv/presentation.js';
    import CatalogPager from './CatalogPager.svelte';
    import ProgrammeEvidence from './ProgrammeEvidence.svelte';
    import GuideEvidence from './GuideEvidence.svelte';
    let { state, locale, heading = $bindable(), onview, onwindow, onpage } = $props();
</script>

<section aria-labelledby="live-tv-guide" aria-busy={state.guideLoading}>
    <h2 id="live-tv-guide" tabindex="-1" bind:this={heading}>{m.live_tv_guide()}{#if state.selected}: <bdi lang="">{state.selected.name || m.live_tv_unnamed_channel()}</bdi>{/if}</h2>
    {#if !state.selected}<p>{m.live_tv_select_channel()}</p>
    {:else}
        <div class="controls" role="group" aria-label={m.live_tv_guide_view()}>
            <button class="secondary-action" type="button" aria-pressed={state.view === 'window'} onclick={() => onview('window')}>{m.live_tv_scheduled()}</button>
            <button class="secondary-action" type="button" aria-pressed={state.view === 'unpositionable'} onclick={() => onview('unpositionable')}>{m.live_tv_incomplete_times()}</button>
        </div>
        {#if state.view === 'window'}
            <div class="controls" role="group" aria-label={m.live_tv_window_controls()}>
                <button class="secondary-action" type="button" onclick={() => onwindow(-1)}>{m.live_tv_earlier()}</button>
                <button class="secondary-action" type="button" onclick={() => onwindow(0)}>{m.live_tv_current_time()}</button>
                <button class="secondary-action" type="button" onclick={() => onwindow(1)}>{m.live_tv_later()}</button>
            </div>
            <p><time datetime={state.window.from}>{formatInstant(state.window.from, locale)}</time> – <time datetime={state.window.to}>{formatInstant(state.window.to, locale)}</time></p>
        {:else}<p>{m.live_tv_incomplete_help()}</p>{/if}
        <p>{m.live_tv_time_zone()}: {#if deviceTimeZone()}<bdi lang="">{deviceTimeZone()}</bdi>{:else}{m.live_tv_zone_unknown()}{/if}</p>
        <CatalogPager label={m.live_tv_programme_pages()} page={state.guidePage} loading={state.guideLoading} onchange={onpage} />
        {#if state.guideLoading}<p>{m.live_tv_loading_guide()}</p>
        {:else}
            <GuideEvidence guide={state.guide} />
            {#if !state.programmes.length}<p>{m.live_tv_empty_programmes()}</p>
            {:else}<ul class="programmes">{#each state.programmes as programme (programme.id)}<li><ProgrammeEvidence {programme} {locale} /></li>{/each}</ul>{/if}
        {/if}
    {/if}
</section>

<style>
    section { display: flex; flex-direction: column; gap: 1rem; min-width: 0; overflow-wrap: anywhere; }
    h2 { font-size: 1.35rem; }
    .controls { display: flex; flex-wrap: wrap; gap: 0.6rem; }
    button[aria-pressed='true'] { border-color: var(--color-accent); }
    .programmes { list-style: none; padding: 0; display: grid; gap: 1rem; }
    li { min-width: 0; }
    p { color: var(--color-text-secondary); }
</style>
