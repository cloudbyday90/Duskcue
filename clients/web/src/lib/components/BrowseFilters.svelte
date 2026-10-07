<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->

<script>
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';

    let { query, onchange, mode = 'catalog', legend = null, legendLocale = undefined, children = null, typeFacets = [] } = $props();
    let sortValue = $derived(`${query.sort || 'position'}:${query.order}`);
    let favoriteValue = $derived(query.favorite === undefined ? '' : String(query.favorite));
    let sortOptions = $derived([
        ...(mode === 'search' ? [
            { value: 'relevance:desc', label: m.tonight_browse_relevance(), locale: messageLocale('tonight_browse_relevance') },
            { value: 'relevance:asc', label: m.tonight_browse_relevance_ascending(), locale: messageLocale('tonight_browse_relevance_ascending') },
        ] : []),
        ...(mode === 'collection' ? [{ value: 'position:asc', label: m.tonight_browse_collection_order(), locale: messageLocale('tonight_browse_collection_order') }] : []),
        ...(mode !== 'search' ? [
            { value: 'added:desc', label: m.tonight_browse_recent_first(), locale: messageLocale('tonight_browse_recent_first') },
            { value: 'added:asc', label: m.tonight_browse_oldest_first(), locale: messageLocale('tonight_browse_oldest_first') },
        ] : []),
        { value: 'title:asc', label: m.tonight_browse_title_az(), locale: messageLocale('tonight_browse_title_az') },
        { value: 'title:desc', label: m.tonight_browse_title_za(), locale: messageLocale('tonight_browse_title_za') },
        { value: 'year:desc', label: m.tonight_browse_year_newest(), locale: messageLocale('tonight_browse_year_newest') },
        { value: 'year:asc', label: m.tonight_browse_year_oldest(), locale: messageLocale('tonight_browse_year_oldest') },
    ]);

    function changeSort(event) {
        const [sort, order] = event.currentTarget.value.split(':');
        onchange({ sort: sort === 'position' ? '' : sort, order });
    }

    function typeLabel(type, label) {
        const count = typeFacets?.find((facet) => facet.value === type)?.count;
        return count === undefined ? label : `${label} (${count})`;
    }
</script>

<fieldset class="browse-filters">
    <legend lang={legend ? legendLocale : messageLocale('tonight_browse_filters')}>{legend || m.tonight_browse_filters()}</legend>
    {#if children}{@render children()}{/if}
    <div class="filter-control">
        <label lang={messageLocale('tonight_browse_format')} for="browse-format">{m.tonight_browse_format()}</label>
        <select lang={messageLocale('tonight_browse_format')} id="browse-format" value={query.type || ''} onchange={(event) => onchange({ type: event.currentTarget.value })}>
            <option lang={messageLocale('tonight_browse_all_formats')} value="">{m.tonight_browse_all_formats()}</option>
            <option lang={messageLocale('tonight_browse_movies')} value="movie">{typeLabel('movie', m.tonight_browse_movies())}</option>
            <option lang={messageLocale('tonight_browse_tv')} value="series">{typeLabel('series', m.tonight_browse_tv())}</option>
            <option lang={messageLocale('tonight_browse_seasons')} value="season">{typeLabel('season', m.tonight_browse_seasons())}</option>
            <option lang={messageLocale('tonight_browse_episodes')} value="episode">{typeLabel('episode', m.tonight_browse_episodes())}</option>
        </select>
    </div>
    <div class="filter-control">
        <label lang={messageLocale('tonight_browse_sort')} for="browse-sort">{m.tonight_browse_sort()}</label>
        <select lang={messageLocale('tonight_browse_sort')} id="browse-sort" value={sortValue} onchange={changeSort}>
            {#each sortOptions as option}<option lang={option.locale} value={option.value}>{option.label}</option>{/each}
        </select>
    </div>
    <div class="filter-control">
        <label lang={messageLocale('tonight_browse_watch')} for="browse-watch">{m.tonight_browse_watch()}</label>
        <select lang={messageLocale('tonight_browse_watch')} id="browse-watch" value={query.watch || 'all'} onchange={(event) => onchange({ watch: event.currentTarget.value === 'all' ? '' : event.currentTarget.value })}>
            <option lang={messageLocale('tonight_browse_all_watch')} value="all">{m.tonight_browse_all_watch()}</option>
            <option lang={messageLocale('tonight_browse_unwatched')} value="unwatched">{m.tonight_browse_unwatched()}</option>
            <option lang={messageLocale('tonight_browse_in_progress')} value="in_progress">{m.tonight_browse_in_progress()}</option>
            <option lang={messageLocale('tonight_browse_watched')} value="watched">{m.tonight_browse_watched()}</option>
        </select>
    </div>
    <div class="filter-control">
        <label lang={messageLocale('tonight_browse_favorite')} for="browse-favorite">{m.tonight_browse_favorite()}</label>
        <select lang={messageLocale('tonight_browse_favorite')} id="browse-favorite" value={favoriteValue} onchange={(event) => onchange({ favorite: event.currentTarget.value })}>
            <option lang={messageLocale('tonight_browse_all_watch')} value="">{m.tonight_browse_all_watch()}</option>
            <option lang={messageLocale('tonight_browse_favorites_only')} value="true">{m.tonight_browse_favorites_only()}</option>
            <option lang={messageLocale('tonight_browse_not_favorites')} value="false">{m.tonight_browse_not_favorites()}</option>
        </select>
    </div>
</fieldset>

<style>
    .browse-filters { display: grid; grid-template-columns: repeat(auto-fit, minmax(10rem, 1fr)); gap: 1rem; border: 0; padding: 0; margin: 0; min-width: 0; }
    legend { padding: 0; margin-bottom: 0.8rem; color: var(--color-text-secondary); font-size: 0.85rem; }
    .filter-control { display: flex; flex-direction: column; gap: 0.45rem; min-width: 0; }
    label { color: var(--color-text-secondary); font-size: 0.8rem; }
    select { width: 100%; min-height: 44px; padding: 0.65rem 0.8rem; border-radius: var(--radius-md); border: 1px solid var(--color-border); background: var(--color-bg-surface); color: var(--color-text-primary); font: inherit; }
    @media (max-width: 440px) { .browse-filters { grid-template-columns: 1fr; } }
</style>
