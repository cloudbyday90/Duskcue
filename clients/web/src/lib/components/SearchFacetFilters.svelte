<script>
    import { messageLocale } from '$lib/localization/message-locale.js';
    import { m } from '$lib/paraglide/messages.js';
    import { getLocale } from '$lib/paraglide/runtime.js';
    import { facetOptions } from '$lib/browsing/facets.js';

    let { query, facets = null, onchange } = $props();
    let genres = $derived(facetOptions(facets?.genres, query.genre));
    let years = $derived(facetOptions(facets?.years, query.year));
    let ratings = $derived(facetOptions(facets?.ratings, query.rating_min));
    let controls = $derived([
        { key: 'genre', label: m.tonight_browse_genre(), locale: messageLocale('tonight_browse_genre'), all: m.tonight_browse_all_genres(), allLocale: messageLocale('tonight_browse_all_genres'), options: genres },
        { key: 'year', label: m.tonight_browse_year(), locale: messageLocale('tonight_browse_year'), all: m.tonight_browse_all_years(), allLocale: messageLocale('tonight_browse_all_years'), options: years },
        { key: 'rating_min', label: m.tonight_browse_rating(), locale: messageLocale('tonight_browse_rating'), all: m.tonight_browse_all_ratings(), allLocale: messageLocale('tonight_browse_all_ratings'), options: ratings },
    ]);
</script>

{#each controls as control (control.key)}
    <div class="facet-control">
        <label lang={control.locale} for={`browse-search-${control.key}`}>{control.label}</label>
        <select lang={control.locale} id={`browse-search-${control.key}`} value={query[control.key] || ''} onchange={(event) => onchange({ [control.key]: event.currentTarget.value })}>
            <option lang={control.allLocale} value="">{control.all}</option>
            {#each control.options as option (option.value)}<option lang={getLocale()} value={option.value}>{option.label}{option.count === null ? '' : ` (${option.count})`}</option>{/each}
        </select>
    </div>
{/each}

<style>
    .facet-control { display: flex; flex-direction: column; gap: 0.45rem; min-width: 0; }
    label { color: var(--color-text-secondary); font-size: 0.8rem; }
    select { width: 100%; min-height: 44px; padding: 0.65rem 0.8rem; border-radius: var(--radius-md); border: 1px solid var(--color-border); background: var(--color-bg-surface); color: var(--color-text-primary); font: inherit; }
</style>
