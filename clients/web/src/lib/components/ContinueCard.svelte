<script>
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    import Artwork from './Artwork.svelte';
    import { titleRoute, playRoute } from '$lib/navigation/routes.js';
    import { formatDuration } from '$lib/utils/format.js';

    let { item, origin = '/dashboard' } = $props();
    let canPlay = $derived(item.availability?.can_play === true);
    let title = $derived(item.series_title || item.title);
    let position = $derived(item.watch_state?.resume_position_ms || 0);
    let duration = $derived(item.duration_ms || (item.runtime_seconds ? item.runtime_seconds * 1000 : 0));
    let progress = $derived(duration > 0 ? Math.min(100, 100 * position / duration) : 0);
    let remaining = $derived(duration > position ? formatDuration((duration - position) / 1000) : null);
</script>

<article class="continue-card">
    {#if canPlay}
        <a lang={messageLocale('tonight_resume')} class="resume-art" href={playRoute(item, origin)} aria-label={`${m.tonight_resume()} ${title}`}>
            <Artwork itemId={item.id} type="thumbnail" size="w640" mediaType={item.type} />
            <span class="play-symbol" aria-hidden="true">▶</span>
            {#if progress > 0}<span class="progress" aria-hidden="true"><span style:width={`${progress}%`}></span></span>{/if}
        </a>
    {:else}
        <div class="resume-art"><Artwork itemId={item.id} type="thumbnail" size="w640" mediaType={item.type} /></div>
    {/if}
    <div class="information">
        <a class="title" href={titleRoute(item, origin)}>{title}</a>
        {#if item.type === 'episode'}
            <p>S{item.season_number} · E{item.episode_number} · {item.title}</p>
        {/if}
        {#if canPlay && remaining}<p lang={messageLocale('tonight_home_remaining')}>{m.tonight_home_remaining({ duration: remaining })}</p>{/if}
        {#if !canPlay}<p lang={messageLocale('tonight_home_unavailable')} class="unavailable">{m.tonight_home_unavailable()}</p>{/if}
        <a lang={messageLocale('tonight_title_details')} class="details" href={titleRoute(item, origin)}>{m.tonight_title_details()}</a>
    </div>
</article>

<style>
    .continue-card { min-width: 0; border-radius: var(--radius-lg); overflow: hidden; background: var(--color-bg-surface); }
    .resume-art { display: block; position: relative; aspect-ratio: 16 / 9; }
    .play-symbol { position: absolute; inset: 0; display: grid; place-items: center; background: linear-gradient(transparent, rgb(0 0 0 / 35%)); font-size: 1.75rem; color: white; text-shadow: 0 1px 8px black; }
    .resume-art:hover .play-symbol { background: rgb(0 0 0 / 25%); }
    .progress { position: absolute; bottom: 0; inset-inline: 0; height: 4px; background: rgb(0 0 0 / 65%); }
    .progress span { display: block; height: 100%; background: var(--color-accent); }
    .information { display: grid; gap: 0.25rem; padding: 1rem 1.15rem; }
    .title { font-weight: 600; font-size: 1.05rem; overflow-wrap: anywhere; }
    p { font-size: 0.8rem; color: var(--color-text-secondary); overflow-wrap: anywhere; }
    .details { width: fit-content; display: flex; align-items: center; min-height: 44px; font-size: 0.8rem; color: var(--color-accent); }
    .unavailable { color: var(--color-warning); }
</style>
