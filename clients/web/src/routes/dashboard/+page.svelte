<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    import { onMount } from 'svelte';
    import { listMediaItems, listContinueWatching } from '$lib/api/media.js';
    import { titleRoute } from '$lib/navigation/routes.js';
    import MediaCard from '$lib/components/MediaCard.svelte';
    import ContinueCard from '$lib/components/ContinueCard.svelte';
    import Artwork from '$lib/components/Artwork.svelte';

    let recentlyAdded = $state([]);
    let continueWatching = $state([]);
    let recentLoading = $state(true);
    let continueLoading = $state(true);
    let recentError = $state(false);
    let continueError = $state(false);
    let continueMoreError = $state(false);
    let continueCursor = $state(null);
    let continueHasMore = $state(false);
    let loadingMore = $state(false);
    let requests;
    let feature = $derived(recentlyAdded.find((item) => item.type === 'movie' && item.availability?.can_play));

    onMount(() => {
        requests = new AbortController();
        loadRecentlyAdded();
        loadContinueWatching();
        return () => requests.abort();
    });

    async function loadRecentlyAdded() {
        recentLoading = true;
        recentError = false;
        try {
            const response = await listMediaItems({ limit: 18, order: 'desc' }, { signal: requests.signal });
            recentlyAdded = response.items || [];
        } catch (error) {
            if (error.name !== 'AbortError') recentError = true;
        } finally {
            if (!requests.signal.aborted) recentLoading = false;
        }
    }

    async function loadContinueWatching(append = false) {
        if (append && (loadingMore || !continueHasMore)) return;
        if (append) loadingMore = true;
        else continueLoading = true;
        continueError = false;
        continueMoreError = false;
        try {
            const response = await listContinueWatching({ limit: 6, ...(append ? { cursor: continueCursor } : {}) }, { signal: requests.signal });
            continueWatching = append ? [...continueWatching, ...response.items] : response.items;
            continueCursor = response.cursor;
            continueHasMore = response.has_more;
        } catch (error) {
            if (error.name !== 'AbortError') {
                if (append) continueMoreError = true;
                else continueError = true;
            }
        } finally {
            if (!requests.signal.aborted) {
                continueLoading = false;
                loadingMore = false;
            }
        }
    }
</script>

<svelte:head><title>{m.routes_layout_home()} · Duskcue</title></svelte:head>

<div class="dashboard">
    <header class="page-intro">
        <h1 lang={messageLocale('tonight_home_heading')} class="tonight-heading">{m.tonight_home_heading()}</h1>
        <p lang={messageLocale('tonight_home_description')}>{m.tonight_home_description()}</p>
    </header>
    <section class="content-section" aria-labelledby="continue-heading" aria-busy={continueLoading}>
        <h2 lang={messageLocale('tonight_continue_watching')} id="continue-heading">{m.tonight_continue_watching()}</h2>
        {#if continueLoading}<p lang={messageLocale('tonight_loading')} class="section-state" role="status">{m.tonight_loading()}</p>
        {:else if continueError}
            <div class="section-state"><p lang={messageLocale('tonight_home_continue_error')} role="status">{m.tonight_home_continue_error()}</p><button lang={messageLocale('tonight_try_again')} class="secondary-action" onclick={() => loadContinueWatching()}>{m.tonight_try_again()}</button></div>
        {:else if continueWatching.length}
            <div class="continue-gallery">{#each continueWatching as item (item.id)}<ContinueCard {item} />{/each}</div>
            {#if continueMoreError}<p lang={messageLocale('tonight_home_continue_error')} role="status">{m.tonight_home_continue_error()}</p>{/if}
            {#if continueHasMore}<button lang={loadingMore ? messageLocale('tonight_loading') : messageLocale('tonight_load_more')} class="secondary-action more" onclick={() => loadContinueWatching(true)} disabled={loadingMore}>{loadingMore ? m.tonight_loading() : m.tonight_load_more()}</button>{/if}
        {:else}<p lang={messageLocale('tonight_home_no_continue')} class="section-state">{m.tonight_home_no_continue()}</p>{/if}
    </section>
    {#if feature}
        <section class="feature" aria-labelledby="feature-title">
            <div class="feature-art"><Artwork itemId={feature.id} type="backdrop" size="w780" eager /></div>
            <div class="feature-copy">
                <p lang={messageLocale('tonight_home_feature')} class="eyebrow">{m.tonight_home_feature()}</p>
                <h2 id="feature-title" class="tonight-heading">{feature.title}</h2>
                {#if feature.overview}<p class="overview">{feature.overview}</p>{/if}
                <a lang={messageLocale('tonight_home_explore')} class="primary-action" href={titleRoute(feature, '/dashboard')}>{m.tonight_home_explore()}</a>
            </div>
        </section>
    {/if}
    <section class="content-section" aria-labelledby="recent-heading" aria-busy={recentLoading}>
        <div class="section-heading"><h2 lang={messageLocale('tonight_recently_added')} id="recent-heading">{m.tonight_recently_added()}</h2><a lang={messageLocale('tonight_home_view_all')} href="/media" class="view-all">{m.tonight_home_view_all()}</a></div>
        {#if recentLoading}<p lang={messageLocale('tonight_loading')} class="section-state" role="status">{m.tonight_loading()}</p>
        {:else if recentError}
            <div class="section-state"><p lang={messageLocale('tonight_home_recent_error')} role="status">{m.tonight_home_recent_error()}</p><button lang={messageLocale('tonight_try_again')} class="secondary-action" onclick={loadRecentlyAdded}>{m.tonight_try_again()}</button></div>
        {:else if recentlyAdded.length}
            <div class="poster-gallery">{#each recentlyAdded as item (item.id)}<MediaCard {item} href={titleRoute(item, '/dashboard')} />{/each}</div>
        {:else}<p lang={messageLocale('tonight_home_empty')} class="section-state">{m.tonight_home_empty()}</p>{/if}
    </section>
</div>

<style>
    .dashboard { display: flex; flex-direction: column; gap: 2.75rem; }
    .page-intro { display: grid; gap: 0.65rem; }
    .page-intro h1 { font-size: clamp(2rem, 4vw, 3rem); }
    .page-intro p { color: var(--color-text-secondary); }
    .content-section { display: flex; flex-direction: column; gap: 1.25rem; }
    .content-section h2 { font-size: 1.2rem; font-weight: 500; }
    .continue-gallery { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 18rem), 1fr)); gap: var(--gallery-gap); }
    .section-heading { display: flex; align-items: center; justify-content: space-between; gap: 1rem; }
    .view-all { color: var(--color-accent); min-height: 44px; display: flex; align-items: center; font-size: 0.85rem; }
    .section-state { display: flex; align-items: center; flex-wrap: wrap; gap: 1rem; color: var(--color-text-secondary); padding: 1.5rem; background: var(--color-bg-surface); border-radius: var(--radius-lg); }
    .more { align-self: center; }
    .feature { display: grid; grid-template-columns: 1fr 1fr; min-height: 250px; max-height: 420px; background: var(--color-bg-surface); border: 1px solid var(--color-border-subtle); border-radius: var(--radius-lg); overflow: hidden; }
    .feature-art { min-width: 0; }
    .feature-copy { display: flex; flex-direction: column; align-items: flex-start; justify-content: center; gap: 1rem; padding: clamp(1.5rem, 3vw, 3rem); min-width: 0; }
    .eyebrow { font-size: 0.75rem; color: var(--color-accent); text-transform: uppercase; letter-spacing: 0.1em; }
    .feature h2 { font-size: clamp(1.8rem, 3vw, 3rem); overflow-wrap: anywhere; }
    .overview { color: var(--color-text-secondary); display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; }
    @media (max-width: 700px) { .feature { grid-template-columns: 1fr; max-height: none; } .feature-art { aspect-ratio: 16 / 9; } }
</style>
