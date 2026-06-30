<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import { onMount } from 'svelte';
    import { libraries, libraryList, librariesLoading } from '$lib/stores/libraries.js';
    import { MEDIA_TYPE_LABELS } from '$lib/utils/constants.js';

    onMount(() => {
        libraries.fetch();
    });

    function typeIcon(mediaType) {
        switch (mediaType) {
            case 'movie':
                return 'M2 4h20v16H2zM8 4v16';
            case 'series':
                return 'M2 3h20v14H2zM6 21h12';
            case 'music':
                return 'M9 18V5l12-2v13';
            default:
                return 'M4 4h16v16H4z';
        }
    }
</script>

<div class="libraries-page">
    <div class="page-header">
        <h1 class="page-title">{m.routes_libraries_page_libraries()}</h1>
    </div>

    {#if $librariesLoading}
        <div class="loading-state">
            <div class="loading-spinner"></div>
            <p>{m.routes_libraries_page_loading_libraries()}</p>
        </div>
    {:else if $libraryList.length === 0}
        <div class="empty-state">
            <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                <path d="M2 3h20v18H2zM2 8h20M8 3v18" />
            </svg>
            <p class="empty-title">{m.routes_libraries_page_no_libraries_yet()}</p>
            <p class="empty-subtitle">{m.routes_libraries_page_create_a_library_to_start_organizing_your_media_()}</p>
            <a href="/settings/libraries" class="btn-primary">{m.routes_libraries_page_create_a_library()}</a>
        </div>
    {:else}
        <div class="library-grid">
            {#each $libraryList as lib (lib.id)}
                <a href="/libraries/{lib.id}" class="library-card">
                    <div class="library-icon">
                        <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                            <path d={typeIcon(lib.media_type)} />
                        </svg>
                    </div>
                    <div class="library-info">
                        <h3 class="library-name">{lib.name}</h3>
                        <div class="library-meta">
                            <span class="badge">{MEDIA_TYPE_LABELS[lib.media_type] || lib.media_type}</span>
                            <span class="item-count">{lib.item_count || 0} items</span>
                        </div>
                    </div>
                </a>
            {/each}
        </div>
    {/if}
</div>

<style>
    .libraries-page {
        display: flex;
        flex-direction: column;
        gap: 1.5rem;
    }

    .page-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
    }

    .page-title {
        font-size: 1.5rem;
        font-weight: 700;
        color: var(--color-text-primary);
    }

    .library-grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
        gap: 1rem;
    }

    .library-card {
        display: flex;
        align-items: center;
        gap: 1rem;
        padding: 1.25rem;
        background-color: var(--color-bg-surface);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-md);
        transition: border-color var(--transition-fast), background-color var(--transition-fast);
    }

    .library-card:hover {
        border-color: var(--color-accent);
        background-color: var(--color-bg-elevated);
    }

    .library-icon {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 48px;
        height: 48px;
        background-color: var(--color-accent-muted);
        color: var(--color-accent);
        border-radius: var(--radius-md);
        flex-shrink: 0;
    }

    .library-info {
        min-width: 0;
        flex: 1;
    }

    .library-name {
        font-size: 1rem;
        font-weight: 600;
        color: var(--color-text-primary);
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .library-meta {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        margin-top: 0.25rem;
    }

    .badge {
        font-size: 0.6875rem;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.05em;
        color: var(--color-accent);
        background-color: var(--color-accent-muted);
        padding: 0.125rem 0.5rem;
        border-radius: var(--radius-sm);
    }

    .item-count {
        font-size: 0.75rem;
        color: var(--color-text-muted);
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
        padding: 4rem 1rem;
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

    .btn-primary {
        margin-top: 1rem;
        padding: 0.625rem 1.5rem;
        background-color: var(--color-accent);
        color: var(--color-bg-deep);
        font-size: 0.8125rem;
        font-weight: 600;
        border-radius: var(--radius-sm);
        transition: background-color var(--transition-fast);
    }

    .btn-primary:hover {
        background-color: var(--color-accent-hover);
    }

    @media (max-width: 768px) {
        .library-grid {
            grid-template-columns: 1fr;
        }

        .page-title {
            font-size: 1.25rem;
        }
    }
</style>
