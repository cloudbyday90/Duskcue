<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import CatalogPager from './CatalogPager.svelte';
    let { state, onselect, onpage } = $props();
</script>

<section aria-labelledby="live-tv-channels" aria-busy={state.channelsLoading}>
    <h2 id="live-tv-channels">{m.live_tv_channels()}</h2>
    <CatalogPager label={m.live_tv_channel_pages()} page={state.channelPage} loading={state.channelsLoading} onchange={onpage} />
    {#if state.channelsLoading}<p>{m.live_tv_loading_channels()}</p>
    {:else if !state.channels.length}<p>{m.live_tv_empty_channels()}</p>
    {:else}
        <ul class="channels">
            {#each state.channels as channel (channel.id)}
                <li><button type="button" aria-pressed={state.selected?.id === channel.id} onclick={(event) => onselect(channel.id, event.currentTarget)}>
                    <bdi lang="" class="name">{channel.name || m.live_tv_unnamed_channel()}</bdi>
                    {#if channel.number}<span>{m.live_tv_number()}: <bdi lang="">{channel.number}</bdi></span>{/if}
                    {#if channel.group}<span>{m.live_tv_group()}: <bdi lang="">{channel.group}</bdi></span>{/if}
                    {#if channel.tvg_name}<span>{m.live_tv_guide_name()}: <bdi lang="">{channel.tvg_name}</bdi></span>{/if}
                </button></li>
            {/each}
        </ul>
    {/if}
</section>

<style>
    section { display: flex; flex-direction: column; gap: 1rem; min-width: 0; }
    h2 { font-size: 1.35rem; }
    .channels { list-style: none; padding: 0; display: grid; gap: 0.6rem; }
    button { display: flex; flex-direction: column; gap: 0.35rem; width: 100%; min-height: 44px; padding: 0.9rem; border: 1px solid var(--color-border); border-radius: var(--radius-md); background: var(--color-bg-surface); text-align: start; overflow-wrap: anywhere; }
    button:hover { background: var(--color-bg-hover); }
    button[aria-pressed='true'] { border-color: var(--color-accent); background: var(--color-accent-muted); }
    span { color: var(--color-text-secondary); font-size: 0.9rem; max-width: 100%; }
    .name { font-weight: 600; max-width: 100%; }
</style>
