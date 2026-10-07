<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->

<script>
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    import Artwork from './Artwork.svelte';
    import { collectionBrowseRoute } from '$lib/browsing/query.js';

    let { items, origin } = $props();
</script>

<ul class="collection-cards">
    {#each items as collection (collection.id)}
        <li>
            <a href={collectionBrowseRoute(collection.id, origin)}>
                <div class="collection-cover"><Artwork itemId={collection.cover_media_item_id} type="backdrop" size="w780" alt="" /></div>
                <div class="collection-copy">
                    <h2 class="tonight-heading">{collection.name}</h2>
                    {#if collection.description}<p>{collection.description}</p>{/if}
                    <p lang={messageLocale('tonight_browse_collection_count')} class="collection-count">{m.tonight_browse_collection_count({ count: collection.item_count })}</p>
                </div>
            </a>
        </li>
    {/each}
</ul>

<style>
    .collection-cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(min(100%, 18rem), 1fr)); gap: 1.8rem; list-style: none; padding: 0; margin: 0; }
    li { min-width: 0; }
    a { display: flex; flex-direction: column; height: 100%; background: var(--color-bg-surface); border: 1px solid var(--color-border); border-radius: var(--radius-lg); overflow: hidden; }
    a:hover { border-color: var(--color-accent); }
    .collection-cover { aspect-ratio: 16 / 9; }
    .collection-copy { display: flex; flex-direction: column; gap: 0.7rem; padding: 1.3rem; }
    h2 { font-size: 1.8rem; overflow-wrap: anywhere; }
    p { color: var(--color-text-secondary); }
    .collection-count { margin-top: auto; font-size: 0.8rem; }
</style>
