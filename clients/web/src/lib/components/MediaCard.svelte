<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { goto } from '$app/navigation';
    import { formatYear, formatRating } from '../utils/format.js';
    import { MEDIA_TYPE_LABELS } from '../utils/constants.js';
    import { posterUrl } from '../utils/artwork.js';

    let {
        item,
        posterSize = 'w342',
        progress = 0,
        showOverview = true,
        onclick = null,
    } = $props();

    let imgError = $state(false);

    let posterSrc = $derived(posterUrl(item.id, posterSize));

    let year = $derived(formatYear(item?.premiere_date));
    let rating = $derived(formatRating(item?.rating_average));
    let typeLabel = $derived(MEDIA_TYPE_LABELS[item?.type] || null);
    let subtitle = $derived.by(() => {
        if (item?.type === 'episode' && item.season_number != null && item.episode_number != null) {
            return `S${item.season_number} E${item.episode_number}`;
        }
        if (item?.type === 'season' && item.season_number != null) {
            return `Season ${item.season_number}`;
        }
        return year ? String(year) : null;
    });

    let initial = $derived((item?.title || '?').charAt(0).toUpperCase());

    let href = $derived(`/media/${item.id}`);

    function handleClick(event) {
        if (onclick) {
            event.preventDefault();
            onclick(item);
        }
    }
</script>

<a
    class="media-card"
    {href}
    aria-label="{item.title}{subtitle ? `, ${subtitle}` : ''}"
    onclick={handleClick}
>
    <div class="poster-wrapper">
        {#if !imgError}
            <img
                src={posterSrc}
                alt={item.title}
                class="poster"
                loading="lazy"
                onerror={() => imgError = true}
            />
        {:else}
            <div class="poster-placeholder">
                <span class="placeholder-initial">{initial}</span>
            </div>
        {/if}

        {#if rating}
            <div class="badge badge-rating">
                <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                    <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" />
                </svg>
                <span>{rating.toFixed(1)}</span>
            </div>
        {/if}

        {#if typeLabel && item.type !== 'movie'}
            <div class="badge badge-type">{typeLabel}</div>
        {/if}

        {#if progress > 0}
            <div class="progress-bar">
                <div class="progress-fill" style="width: {Math.min(100, progress)}%"></div>
            </div>
        {/if}

        {#if showOverview && item.overview}
            <div class="overlay">
                <p class="overlay-overview">{item.overview}</p>
            </div>
        {/if}
    </div>

    <div class="card-info">
        <h3 class="card-title">{item.title}</h3>
        {#if subtitle}
            <p class="card-subtitle">{subtitle}</p>
        {/if}
    </div>
</a>

<style>
    .media-card {
        cursor: pointer;
        display: flex;
        flex-direction: column;
        gap: 0.5rem;
        outline: none;
        transition: transform var(--transition-normal);
    }

    .media-card:hover {
        transform: translateY(-4px);
    }

    .media-card:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 4px;
        border-radius: var(--radius-md);
    }

    .poster-wrapper {
        position: relative;
        aspect-ratio: 2 / 3;
        border-radius: var(--radius-md);
        overflow: hidden;
        background-color: var(--color-bg-surface);
        box-shadow: var(--shadow-card);
        transition: box-shadow var(--transition-normal);
    }

    .media-card:hover .poster-wrapper {
        box-shadow: var(--shadow-elevated);
    }

    .poster {
        width: 100%;
        height: 100%;
        object-fit: cover;
    }

    .poster-placeholder {
        width: 100%;
        height: 100%;
        display: flex;
        align-items: center;
        justify-content: center;
        background: linear-gradient(135deg, var(--color-bg-elevated), var(--color-bg-surface));
    }

    .placeholder-initial {
        font-size: 3rem;
        font-weight: 700;
        color: var(--color-text-muted);
    }

    .badge {
        position: absolute;
        display: flex;
        align-items: center;
        gap: 0.25rem;
        padding: 0.25rem 0.5rem;
        font-size: 0.6875rem;
        font-weight: 600;
        border-radius: var(--radius-sm);
        backdrop-filter: blur(8px);
    }

    .badge-rating {
        top: 0.5rem;
        right: 0.5rem;
        color: var(--color-text-primary);
        background-color: rgba(0, 0, 0, 0.7);
    }

    .badge-type {
        bottom: 0.5rem;
        left: 0.5rem;
        color: var(--color-text-primary);
        background-color: rgba(0, 0, 0, 0.7);
        text-transform: capitalize;
    }

    .progress-bar {
        position: absolute;
        bottom: 0;
        left: 0;
        right: 0;
        height: 3px;
        background-color: rgba(0, 0, 0, 0.5);
    }

    .progress-fill {
        height: 100%;
        background-color: var(--color-accent);
        transition: width var(--transition-normal);
    }

    .overlay {
        position: absolute;
        inset: 0;
        display: flex;
        align-items: flex-end;
        padding: 0.75rem;
        background: linear-gradient(to top, rgba(0, 0, 0, 0.85) 0%, transparent 60%);
        opacity: 0;
        transition: opacity var(--transition-normal);
        pointer-events: none;
    }

    .media-card:hover .overlay,
    .media-card:focus-within .overlay {
        opacity: 1;
    }

    .overlay-overview {
        font-size: 0.75rem;
        line-height: 1.4;
        color: var(--color-text-primary);
        display: -webkit-box;
        -webkit-line-clamp: 4;
        line-clamp: 4;
        -webkit-box-orient: vertical;
        overflow: hidden;
    }

    .card-info {
        display: flex;
        flex-direction: column;
        gap: 0.125rem;
    }

    .card-title {
        font-size: 0.875rem;
        font-weight: 600;
        color: var(--color-text-primary);
        line-height: 1.3;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .card-subtitle {
        font-size: 0.75rem;
        color: var(--color-text-secondary);
    }
</style>
