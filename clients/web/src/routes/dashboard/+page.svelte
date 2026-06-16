<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { onMount } from 'svelte';
    import { listMediaItems } from '$lib/api/media.js';
    import { getWatchData } from '$lib/api/playback.js';
    import { listLibraries } from '$lib/api/libraries.js';
    import { libraries } from '$lib/stores/libraries.js';
    import { currentUser } from '$lib/stores/auth.js';
    import MediaCard from '$lib/components/MediaCard.svelte';
    import { formatDuration } from '$lib/utils/format.js';

    let loading = $state(true);
    let recentlyAdded = $state([]);
    let continueWatching = $state([]);

    let libraryCount = $derived($libraries.items.length);
    let libraryLabel = $derived(libraryCount === 1 ? 'library' : 'libraries');

    onMount(async () => {
        await Promise.all([loadRecentlyAdded(), loadContinueWatching(), libraries.fetch()]);
        loading = false;
    });

    async function loadRecentlyAdded() {
        try {
            const response = await listMediaItems({ limit: 18, order: 'desc' });
            recentlyAdded = response.items || response || [];
        } catch {
            recentlyAdded = [];
        }
    }

    async function loadContinueWatching() {
        try {
            const response = await listMediaItems({ limit: 12, order: 'desc', type: 'movie' });
            const items = response.items || response || [];
            const watched = [];
            for (const item of items) {
                try {
                    const wd = await getWatchData(item.id);
                    if (wd.resume_position_ms > 0 && !wd.is_watched) {
                        const durationMs = (item.runtime_seconds || 0) * 1000;
                        const pct = durationMs > 0
                            ? Math.min(100, (wd.resume_position_ms / durationMs) * 100)
                            : 0;
                        watched.push({ ...item, _progress: pct, _resume: wd.resume_position_ms });
                    }
                } catch {
                }
            }
            continueWatching = watched;
        } catch {
            continueWatching = [];
        }
    }
</script>

<div class="dashboard">
    <section class="hero">
        <h1 class="hero-title">
            Welcome back, {$currentUser?.display_name || 'there'}
        </h1>
        <p class="hero-subtitle">
            {#if libraryCount > 0}
                {libraryCount} {libraryLabel} · {recentlyAdded.length} recent {recentlyAdded.length === 1 ? 'item' : 'items'}
            {:else}
                No libraries configured yet. Visit Settings to create one.
            {/if}
        </p>
    </section>

    {#if loading}
        <div class="loading-state">
            <div class="loading-spinner"></div>
            <p>Loading your library…</p>
        </div>
    {:else}
        {#if continueWatching.length > 0}
            <section class="content-row">
                <h2 class="row-title">Continue Watching</h2>
                <div class="card-row">
                    {#each continueWatching as item (item.id)}
                        <div class="card-wrapper">
                            <MediaCard {item} progress={item._progress} showOverview={false} />
                        </div>
                    {/each}
                </div>
            </section>
        {/if}

        <section class="content-row">
            <h2 class="row-title">Recently Added</h2>
            {#if recentlyAdded.length > 0}
                <div class="card-row">
                    {#each recentlyAdded as item (item.id)}
                        <div class="card-wrapper">
                            <MediaCard {item} />
                        </div>
                    {/each}
                </div>
            {:else}
                <div class="empty-state">
                    <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                        <path d="M2 3h20v18H2z" />
                        <path d="M2 8h20M8 3v18" />
                    </svg>
                    <p class="empty-title">No media found</p>
                    <p class="empty-subtitle">
                        Create a library and run a scan to populate your catalog.
                    </p>
                    <a href="/settings/libraries" class="btn-link">Configure Libraries</a>
                </div>
            {/if}
        </section>
    {/if}
</div>

<style>
    .dashboard {
        display: flex;
        flex-direction: column;
        gap: 2.5rem;
    }

    .hero {
        padding: 1rem 0 0.5rem;
    }

    .hero-title {
        font-size: 1.75rem;
        font-weight: 700;
        color: var(--color-text-primary);
    }

    .hero-subtitle {
        font-size: 0.875rem;
        color: var(--color-text-secondary);
        margin-top: 0.375rem;
    }

    .content-row {
        display: flex;
        flex-direction: column;
        gap: 1rem;
    }

    .row-title {
        font-size: 1.125rem;
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .card-row {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
        gap: 1rem;
    }

    .card-wrapper {
        min-width: 0;
    }

    .loading-state {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: 1rem;
        padding: 4rem 0;
        color: var(--color-text-muted);
    }

    .loading-spinner {
        width: 32px;
        height: 32px;
        border: 3px solid var(--color-border);
        border-top-color: var(--color-accent);
        border-radius: 50%;
        animation: spin 0.8s linear infinite;
    }

    @keyframes spin {
        to {
            transform: rotate(360deg);
        }
    }

    .empty-state {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: 0.5rem;
        padding: 3rem 1rem;
        text-align: center;
    }

    .empty-title {
        font-size: 1rem;
        font-weight: 600;
        color: var(--color-text-secondary);
        margin-top: 0.5rem;
    }

    .empty-subtitle {
        font-size: 0.8125rem;
        color: var(--color-text-muted);
    }

    .btn-link {
        display: inline-block;
        margin-top: 1rem;
        padding: 0.5rem 1.25rem;
        background-color: var(--color-accent);
        color: var(--color-bg-deep);
        font-size: 0.8125rem;
        font-weight: 600;
        border-radius: var(--radius-sm);
        transition: background-color var(--transition-fast);
    }

    .btn-link:hover {
        background-color: var(--color-accent-hover);
    }

    @media (max-width: 768px) {
        .hero-title {
            font-size: 1.375rem;
        }

        .card-row {
            grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
            gap: 0.75rem;
        }

        .dashboard {
            gap: 1.75rem;
        }
    }
</style>
