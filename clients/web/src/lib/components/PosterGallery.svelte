<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->

<script>
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    import MediaCard from './MediaCard.svelte';
    import { titleRoute } from '$lib/navigation/routes.js';
    import { formatDuration } from '$lib/utils/format.js';

    let { items = [], origin } = $props();

    function progress(item) {
        if (!item.duration_ms || item.watch_state?.is_watched) return 0;
        return Math.min(100, Math.max(0, ((item.watch_state?.resume_position_ms || 0) / item.duration_ms) * 100));
    }

    function remaining(item) {
        if (!item.duration_ms || !item.watch_state?.resume_position_ms || item.watch_state.is_watched) return null;
        return formatDuration(Math.max(0, item.duration_ms - item.watch_state.resume_position_ms) / 1000);
    }
</script>

<ul class="poster-gallery">
    {#each items as item (item.id)}
        <li>
            <MediaCard {item} href={titleRoute(item, origin)} progress={progress(item)} showOverview={false} />
            <div class="watch-detail">
                {#if item.watch_state?.is_watched}<span lang={messageLocale('tonight_browse_watched')}>{m.tonight_browse_watched()}</span>
                {:else if remaining(item)}<span lang={messageLocale('tonight_browse_remaining')}>{m.tonight_browse_remaining({ duration: remaining(item) })}</span>{/if}
                {#if item.availability?.can_play === false && ['movie', 'episode'].includes(item.type)}<span lang={messageLocale('tonight_browse_unavailable')}>{m.tonight_browse_unavailable()}</span>{/if}
            </div>
        </li>
    {/each}
</ul>

<style>
    .poster-gallery { list-style: none; display: grid; grid-template-columns: repeat(auto-fill, minmax(175px, 1fr)); gap: 2rem 1.4rem; margin: 0; padding: 0; }
    li { min-width: 0; }
    .watch-detail { display: flex; flex-wrap: wrap; gap: 0.35rem 0.6rem; margin-top: 0.45rem; color: var(--color-text-secondary); font-size: 0.75rem; }
    @media (max-width: 640px) { .poster-gallery { grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 1.5rem 1rem; } }
</style>
