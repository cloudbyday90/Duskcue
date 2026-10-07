<script>
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    import { libraryBrowseRoute } from '$lib/browsing/query.js';

    let { items, origin } = $props();
</script>

<ul class="library-cards">
    {#each items as library (library.id)}
        <li><a href={libraryBrowseRoute(library.id, origin)}>
            <span class="library-mark" aria-hidden="true"><svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3 6h7l2 2h9v12H3zM3 6V4h7l2 2h9v2" /></svg></span>
            <div><h2 class="tonight-heading">{library.name}</h2><p lang={library.type === 'movies' ? messageLocale('tonight_browse_movies') : library.type === 'tvshows' ? messageLocale('tonight_browse_tv') : undefined}>{library.type === 'movies' ? m.tonight_browse_movies() : library.type === 'tvshows' ? m.tonight_browse_tv() : library.type}</p></div>
        </a></li>
    {/each}
</ul>

<style>
    .library-cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(min(100%, 18rem), 1fr)); gap: 1.5rem; list-style: none; margin: 0; padding: 0; }
    a { display: flex; align-items: center; gap: 1rem; padding: 1.3rem; height: 100%; border: 1px solid var(--color-border); border-radius: var(--radius-lg); background: var(--color-bg-surface); }
    a:hover { border-color: var(--color-accent); }
    .library-mark { display: grid; place-items: center; flex: 0 0 3.5rem; height: 3.5rem; border-radius: var(--radius-md); background: var(--color-accent-muted); color: var(--color-accent); font-size: 1.2rem; }
    div { min-width: 0; }
    h2 { font-size: 1.6rem; overflow-wrap: anywhere; }
    p { margin-top: 0.45rem; color: var(--color-text-secondary); font-size: 0.85rem; }
</style>
