<script>
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    import Artwork from './Artwork.svelte';
    import { getLocale } from '$lib/paraglide/runtime.js';
    import { titleRoute } from '../navigation/routes.js';
    import { formatDuration, formatTimestamp, formatPercent } from '../utils/format.js';

    let { seasons = [], seasonId = '', episodes = [], selectedId = '', origin, loading = false, error = '', errorLocale = undefined, onseason, onselect, onretry } = $props();

    function selectLink(event, episode) {
        if (event.button !== 0 || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
        event.preventDefault();
        onselect(episode);
    }
</script>

<section class="episode-gallery" aria-labelledby="episode-gallery-heading">
    <div class="gallery-heading">
        <h2 lang={messageLocale('tonight_title_episodes')} id="episode-gallery-heading" tabindex="-1">{m.tonight_title_episodes()}</h2>
        {#if seasons.length}
            <div class="season-label">
                <label lang={messageLocale('tonight_title_season')} for="title-season">{m.tonight_title_season()}</label>
                <select lang={messageLocale('tonight_title_season')} id="title-season" value={seasonId} onchange={(event) => onseason(event.currentTarget.value)}>
                    {#if !seasonId}<option lang={messageLocale('tonight_title_choose_season')} value="" disabled>{m.tonight_title_choose_season()}</option>{/if}
                    {#each seasons as season (season.id)}
                        <option lang={season.season_number === 0 ? messageLocale('tonight_title_specials') : messageLocale('tonight_title_season_number')} value={season.id}>{season.season_number === 0 ? m.tonight_title_specials() : m.tonight_title_season_number({ number: season.season_number })}</option>
                    {/each}
                </select>
            </div>
        {/if}
    </div>
    <p lang={loading ? messageLocale('tonight_title_loading_episodes') : !error && seasonId ? messageLocale('tonight_title_episode_count') : undefined} class="gallery-status" role="status" aria-atomic="true">
        {loading ? m.tonight_title_loading_episodes() : !error && seasonId ? m.tonight_title_episode_count({ count: episodes.length }) : ''}
    </p>
    {#if error}
        <div class="gallery-error">
            <p lang={errorLocale} role="alert">{error}</p>
            <button lang={messageLocale('tonight_title_retry')} type="button" onclick={onretry}>{m.tonight_title_retry()}</button>
        </div>
    {:else if !seasons.length}
        <p lang={messageLocale('tonight_title_no_seasons')}>{m.tonight_title_no_seasons()}</p>
    {:else if !loading && seasonId && !episodes.length}
        <p lang={messageLocale('tonight_title_no_episodes')}>{m.tonight_title_no_episodes()}</p>
    {/if}
    <ul class="episode-list" aria-busy={loading}>
        {#each episodes as episode (episode.id)}
            {@const watched = episode.watch_state?.is_watched === true}
            {@const resume = watched ? 0 : episode.watch_state?.resume_position_ms ?? 0}
            <li class:selected={episode.id === selectedId}>
                <a lang={episode.episode_number == null ? messageLocale('tonight_title_episodes') : messageLocale('tonight_title_episode_number')} class="episode-link" href={titleRoute(episode, origin)} aria-current={episode.id === selectedId ? 'true' : undefined} onclick={(event) => selectLink(event, episode)}>
                    <div class="episode-art"><Artwork itemId={episode.id} type="thumbnail" size="w320" mediaType="episode" /></div>
                    <div lang={getLocale()} class="episode-copy">
                        <p lang={episode.episode_number == null ? messageLocale('tonight_title_episodes') : messageLocale('tonight_title_episode_number')} class="episode-number">{episode.episode_number == null ? m.tonight_title_episodes() : m.tonight_title_episode_number({ number: episode.episode_number })}</p>
                        <h3>{episode.title}</h3>
                        <p class="episode-meta">
                            {#if episode.duration_ms > 0}<span>{formatDuration(episode.duration_ms / 1000)}</span>{/if}
                            <span lang={watched ? messageLocale('tonight_title_watched') : messageLocale('tonight_title_unwatched')}>{watched ? m.tonight_title_watched() : m.tonight_title_unwatched()}</span>
                            <span lang={episode.availability?.can_play ? messageLocale('tonight_title_available') : messageLocale('tonight_title_unavailable')} class:unavailable={!episode.availability?.can_play}>{episode.availability?.can_play ? m.tonight_title_available() : m.tonight_title_unavailable()}</span>
                        </p>
                        {#if resume > 0}
                            <p lang={messageLocale('tonight_title_resume_from')}>{m.tonight_title_resume_from({ position: formatTimestamp(resume) })}</p>
                            {#if episode.duration_ms > 0}<progress lang={messageLocale('tonight_title_progress')} max="100" value={formatPercent(resume, episode.duration_ms)} aria-label={m.tonight_title_progress({ title: episode.title })}></progress>{/if}
                        {/if}
                        <p lang={episode.overview ? undefined : messageLocale('tonight_title_synopsis_unavailable')} class="episode-synopsis">{episode.overview || m.tonight_title_synopsis_unavailable()}</p>
                        {#if episode.id === selectedId}<span lang={messageLocale('tonight_title_selected')} class="selected-label">{m.tonight_title_selected()}</span>{/if}
                    </div>
                </a>
            </li>
        {/each}
    </ul>
</section>

<style>
    .episode-gallery { margin-top: 2.5rem; }
    .gallery-heading { display: flex; align-items: center; justify-content: space-between; gap: 1rem; flex-wrap: wrap; }
    h2 { font-size: 1.7rem; }
    .season-label { display: flex; align-items: center; gap: .75rem; }
    select, button { min-height: 44px; padding: .6rem .85rem; color: var(--color-text-primary); background: var(--color-bg-elevated); border: 1px solid var(--color-border); border-radius: var(--radius-sm); }
    .gallery-status { min-height: 1.5rem; margin: .75rem 0; color: var(--color-text-muted); }
    .gallery-error { padding: 1rem; border: 1px solid var(--color-error); border-radius: var(--radius-md); }
    .gallery-error button { margin-top: .75rem; }
    .episode-list { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 290px), 1fr)); gap: var(--gallery-gap); list-style: none; padding: 0; }
    li { border: 1px solid var(--color-border-subtle); border-radius: var(--radius-md); overflow: hidden; background: var(--color-bg-surface); }
    li.selected { border-color: var(--color-accent); }
    .episode-link { display: block; height: 100%; color: var(--color-text-primary); text-decoration: none; }
    .episode-link:focus-visible { outline-offset: -4px; }
    .episode-art { aspect-ratio: 16 / 9; }
    .episode-copy { padding: 1rem; }
    .episode-number, .episode-meta, .episode-synopsis { color: var(--color-text-secondary); }
    h3 { font-size: 1.1rem; margin: .3rem 0 .6rem; font-family: var(--font-sans); }
    .episode-meta { display: flex; gap: .65rem; flex-wrap: wrap; font-size: .8rem; }
    .episode-synopsis { margin-top: .7rem; font-size: .85rem; line-height: 1.5; }
    .unavailable { color: var(--color-warning); }
    progress { width: 100%; height: .35rem; accent-color: var(--color-accent); }
    .selected-label { display: inline-block; margin-top: .65rem; color: var(--color-accent); }
</style>
