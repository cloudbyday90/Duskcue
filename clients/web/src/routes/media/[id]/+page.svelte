<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import { onMount } from 'svelte';
    import { page } from '$app/stores';
    import { goto } from '$app/navigation';
    import { getMediaItem, listMediaFiles } from '$lib/api/media.js';
    import { getWatchData, updateWatchData } from '$lib/api/playback.js';
    import { notifications } from '$lib/stores/notifications.js';
    import { formatDuration, formatYear, formatRating } from '$lib/utils/format.js';
    import { MEDIA_TYPE_LABELS } from '$lib/utils/constants.js';
    import { posterUrl, backdropUrl } from '$lib/utils/artwork.js';

    let itemId = $derived($page.params.id);
    let loading = $state(true);
    let item = $state(null);
    let files = $state([]);
    let watchData = $state(null);
    let isFavorite = $state(false);
    let userRating = $state(0);
    let backdropError = $state(false);
    let posterError = $state(false);

    onMount(async () => {
        await loadData();
        loading = false;
    });

    async function loadData() {
        try {
            const [itemData, filesData] = await Promise.all([
                getMediaItem(itemId),
                listMediaFiles(itemId),
            ]);
            item = itemData;
            files = filesData.items || filesData || [];
            try {
                watchData = await getWatchData(itemId);
                isFavorite = watchData.is_favorite || false;
                userRating = watchData.user_rating || 0;
            } catch {
            }
        } catch (err) {
            notifications.error(err.detail || err.message || m.routes_media_id_page_failed_to_load_media_item());
        }
    }

    function handlePlay() {
        if (!files.length) {
            notifications.warning(m.routes_media_id_page_no_playable_files_available());
            return;
        }
        const file = files[0];
        goto(`/play/${itemId}?file=${file.id}`);
    }

    async function toggleFavorite() {
        const newVal = !isFavorite;
        isFavorite = newVal;
        try {
            await updateWatchData(itemId, { is_favorite: newVal });
        } catch {
            isFavorite = !newVal;
            notifications.error(m.routes_media_id_page_failed_to_update_favorite_status());
        }
    }

    async function setRating(rating) {
        const newRating = userRating === rating ? 0 : rating;
        userRating = newRating;
        try {
            await updateWatchData(itemId, { user_rating: newRating || null });
        } catch {
            userRating = userRating === 0 ? rating : 0;
            notifications.error(m.routes_media_id_page_failed_to_update_rating());
        }
    }

    let year = $derived(item ? formatYear(item.premiere_date) : null);
    let rating = $derived(item ? formatRating(item.rating_average) : null);
    let runtimeLabel = $derived(
        item?.runtime_seconds ? formatDuration(item.runtime_seconds) : null,
    );
    let backdropSrc = $derived(item ? backdropUrl(item.id, 'w1280') : null);
    let posterSrc = $derived(item ? posterUrl(item.id, 'w500') : null);
    let resumeMs = $derived(watchData?.resume_position_ms || 0);
    let progressPct = $derived(
        item && resumeMs > 0 && item.runtime_seconds
            ? Math.min(100, (resumeMs / (item.runtime_seconds * 1000)) * 100)
            : 0,
    );
</script>

<div class="media-detail">
    {#if loading}
        <div class="loading-state">
            <div class="loading-spinner"></div>
        </div>
    {:else if item}
    <div class="detail-backdrop">
        {#if backdropSrc && !backdropError}
            <img
                src={backdropSrc}
                alt=""
                class="backdrop-image"
                onerror={() => backdropError = true}
            />
        {/if}
        <div class="backdrop-overlay"></div>
    </div>

    <div class="detail-content">
        <div class="detail-header">
            <div class="poster-area">
                {#if posterSrc && !posterError}
                    <img
                        src={posterSrc}
                        alt={item.title}
                        class="poster"
                        onerror={() => posterError = true}
                    />
                {:else}
                    <div class="poster-placeholder">{item.title?.[0]?.toUpperCase()}</div>
                {/if}
            </div>

            <div class="info-area">
                <div class="info-top">
                    <h1 class="media-title">{item.title}</h1>
                    {#if item.type && item.type !== 'movie'}
                        <span class="type-badge">{MEDIA_TYPE_LABELS[item.type] || item.type}</span>
                    {/if}
                </div>

                <div class="meta-row">
                    {#if year}<span class="meta-item">{year}</span>{/if}
                    {#if rating}
                        <span class="meta-item rating">
                            <svg width="14" height="14" viewBox="0 0 24 24" fill="var(--color-accent)" stroke="none">
                                <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" />
                            </svg>
                            {rating}
                        </span>
                    {/if}
                    {#if runtimeLabel}<span class="meta-item">{runtimeLabel}</span>{/if}
                    {#if item.file_count}
                        <span class="meta-item">{item.file_count} {item.file_count === 1 ? m.routes_media_id_page_file() : m.routes_media_id_page_files_count()}</span>
                    {/if}
                </div>

                {#if item.overview}
                    <p class="overview">{item.overview}</p>
                {/if}

                <div class="action-row">
                    <button class="btn-play" onclick={handlePlay} disabled={!files.length}>
                        <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor" stroke="none">
                            <path d="M5 3l14 9-14 9V3z" />
                        </svg>
                        {progressPct > 0 ? m.routes_media_id_page_resume() : m.routes_media_id_page_play()}
                    </button>

                    <button
                        class="btn-icon"
                        class:active={isFavorite}
                        onclick={toggleFavorite}
                        aria-label={isFavorite ? m.routes_media_id_page_remove_from_favorites() : m.routes_media_id_page_add_to_favorites()}
                    >
                        {#if isFavorite}
                            <svg width="20" height="20" viewBox="0 0 24 24" fill="var(--color-error)" stroke="var(--color-error)" stroke-width="2">
                                <path d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z" />
                            </svg>
                        {:else}
                            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                <path d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z" />
                            </svg>
                        {/if}
                    </button>

                    <div class="rating-stars">
                        {#each Array(5) as _, i}
                            <button
                                class="star-btn"
                                class:filled={userRating >= (i + 1) * 2}
                                onclick={() => setRating((i + 1) * 2)}
                                aria-label={`${m.routes_media_id_page_rate()} ${(i + 1) * 2}/10`}
                            >
                                <svg width="16" height="16" viewBox="0 0 24 24" fill={userRating >= (i + 1) * 2 ? 'var(--color-accent)' : 'none'} stroke="currentColor" stroke-width="2">
                                    <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" />
                                </svg>
                            </button>
                        {/each}
                    </div>
                </div>

                {#if progressPct > 0}
                    <div class="resume-bar">
                        <div class="resume-info">
                            <span class="resume-label">{m.routes_media_id_page_resume_from()}</span>
                            <span class="resume-position">{Math.floor(resumeMs / 60000)}m</span>
                        </div>
                        <div class="progress-track">
                            <div class="progress-fill" style="width: {progressPct}%"></div>
                        </div>
                    </div>
                {/if}
            </div>
        </div>

        {#if files.length > 0}
            <section class="files-section">
                <h2 class="section-title">{m.routes_media_id_page_files()}</h2>
                <div class="files-list">
                    {#each files as file (file.id)}
                        <div class="file-row">
                            <div class="file-info">
                                <span class="file-name">{file.file_name || file.relative_path || m.routes_media_id_page_unknown()}</span>
                                <div class="file-meta">
                                    {#if file.video_codec}<span>{file.video_codec}</span>{/if}
                                    {#if file.video_resolution}<span>{file.video_resolution}</span>{/if}
                                    {#if file.container_format}<span>.{file.container_format}</span>{/if}
                                    {#if file.runtime_seconds}<span>{formatDuration(file.runtime_seconds)}</span>{/if}
                                </div>
                            </div>
                            <div class="file-actions">
                                {#if file.is_healthy === false}
                                    <span class="health-badge unhealthy">{m.routes_media_id_page_unhealthy()}</span>
                                {:else}
                                    <span class="health-badge healthy">{m.routes_media_id_page_healthy()}</span>
                                {/if}
                                <button class="btn-icon-small" onclick={() => goto(`/play/${itemId}?file=${file.id}`)} aria-label={m.routes_media_id_page_play_this_file()}>
                                    <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor" stroke="none">
                                        <path d="M5 3l14 9-14 9V3z" />
                                    </svg>
                                </button>
                            </div>
                        </div>
                    {/each}
                </div>
            </section>
        {/if}
    </div>
    {/if}
</div>

<style>
    .media-detail {
        position: relative;
    }

    .detail-backdrop {
        position: absolute;
        top: -1.5rem;
        left: -1.5rem;
        right: -1.5rem;
        height: 400px;
        overflow: hidden;
        z-index: 0;
    }

    .backdrop-image {
        width: 100%;
        height: 100%;
        object-fit: cover;
        filter: blur(4px);
        opacity: 0.3;
    }

    .backdrop-overlay {
        position: absolute;
        inset: 0;
        background: linear-gradient(
            to bottom,
            rgba(14, 15, 19, 0.4) 0%,
            rgba(14, 15, 19, 0.8) 60%,
            var(--color-bg-deep) 100%
        );
    }

    .detail-content {
        position: relative;
        z-index: 1;
        max-width: 1000px;
        margin: 0 auto;
    }

    .detail-header {
        display: flex;
        gap: 2rem;
        padding-top: 4rem;
    }

    .poster-area {
        flex-shrink: 0;
        width: 200px;
    }

    .poster {
        width: 200px;
        height: 300px;
        object-fit: cover;
        border-radius: var(--radius-md);
        box-shadow: var(--shadow-elevated);
    }

    .poster-placeholder {
        width: 200px;
        height: 300px;
        background: linear-gradient(135deg, var(--color-bg-surface), var(--color-bg-elevated));
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        display: flex;
        align-items: center;
        justify-content: center;
        font-size: 4rem;
        font-weight: 700;
        color: var(--color-text-muted);
    }

    .info-area {
        flex: 1;
        min-width: 0;
    }

    .info-top {
        display: flex;
        align-items: center;
        gap: 0.75rem;
        flex-wrap: wrap;
    }

    .media-title {
        font-size: 2rem;
        font-weight: 700;
        color: var(--color-text-primary);
    }

    .type-badge {
        font-size: 0.6875rem;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.05em;
        color: var(--color-accent);
        background-color: var(--color-accent-muted);
        padding: 0.25rem 0.625rem;
        border-radius: var(--radius-sm);
    }

    .meta-row {
        display: flex;
        align-items: center;
        gap: 1rem;
        margin-top: 0.75rem;
        flex-wrap: wrap;
    }

    .meta-item {
        font-size: 0.8125rem;
        color: var(--color-text-secondary);
        display: flex;
        align-items: center;
        gap: 0.25rem;
    }

    .meta-item.rating {
        color: var(--color-accent);
        font-weight: 600;
    }

    .overview {
        margin-top: 1rem;
        font-size: 0.875rem;
        line-height: 1.6;
        color: var(--color-text-secondary);
    }

    .action-row {
        display: flex;
        align-items: center;
        gap: 0.75rem;
        margin-top: 1.5rem;
    }

    .btn-play {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        padding: 0.75rem 1.75rem;
        background-color: var(--color-accent);
        color: var(--color-bg-deep);
        font-size: 0.9375rem;
        font-weight: 600;
        border-radius: var(--radius-sm);
        transition: background-color var(--transition-fast);
    }

    .btn-play:hover:not(:disabled) {
        background-color: var(--color-accent-hover);
    }

    .btn-play:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .btn-icon {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 40px;
        height: 40px;
        background-color: var(--color-bg-elevated);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        color: var(--color-text-secondary);
        transition: all var(--transition-fast);
    }

    .btn-icon:hover {
        border-color: var(--color-accent);
        color: var(--color-text-primary);
    }

    .btn-icon.active {
        color: var(--color-error);
    }

    .rating-stars {
        display: flex;
        gap: 2px;
    }

    .star-btn {
        display: flex;
        align-items: center;
        justify-content: center;
        padding: 4px;
        color: var(--color-text-muted);
        transition: color var(--transition-fast);
    }

    .star-btn:hover {
        color: var(--color-accent);
    }

    .star-btn.filled {
        color: var(--color-accent);
    }

    .resume-bar {
        margin-top: 1.5rem;
        display: flex;
        flex-direction: column;
        gap: 0.375rem;
        max-width: 300px;
    }

    .resume-info {
        display: flex;
        justify-content: space-between;
        font-size: 0.75rem;
        color: var(--color-text-muted);
    }

    .resume-position {
        color: var(--color-text-secondary);
        font-weight: 600;
    }

    .progress-track {
        height: 4px;
        background-color: var(--color-border);
        border-radius: 2px;
        overflow: hidden;
    }

    .progress-fill {
        height: 100%;
        background-color: var(--color-accent);
        border-radius: 2px;
    }

    .files-section {
        margin-top: 2.5rem;
    }

    .section-title {
        font-size: 1.125rem;
        font-weight: 600;
        color: var(--color-text-primary);
        margin-bottom: 1rem;
    }

    .files-list {
        display: flex;
        flex-direction: column;
        gap: 0.5rem;
    }

    .file-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 0.75rem 1rem;
        background-color: var(--color-bg-surface);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
    }

    .file-info {
        min-width: 0;
        flex: 1;
    }

    .file-name {
        font-size: 0.8125rem;
        color: var(--color-text-primary);
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
        display: block;
    }

    .file-meta {
        display: flex;
        gap: 0.75rem;
        margin-top: 0.25rem;
        font-size: 0.6875rem;
        color: var(--color-text-muted);
    }

    .file-actions {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        flex-shrink: 0;
    }

    .health-badge {
        font-size: 0.625rem;
        font-weight: 600;
        text-transform: uppercase;
        padding: 0.125rem 0.5rem;
        border-radius: var(--radius-sm);
    }

    .health-badge.healthy {
        color: var(--color-success);
        background-color: var(--color-success-bg);
    }

    .health-badge.unhealthy {
        color: var(--color-error);
        background-color: var(--color-error-bg);
    }

    .btn-icon-small {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 28px;
        height: 28px;
        background-color: var(--color-bg-elevated);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
        color: var(--color-text-secondary);
        transition: all var(--transition-fast);
    }

    .btn-icon-small:hover {
        color: var(--color-accent);
        border-color: var(--color-accent);
    }

    .loading-state {
        display: flex;
        align-items: center;
        justify-content: center;
        padding: 6rem 0;
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

    @media (max-width: 768px) {
        .detail-header {
            flex-direction: column;
            gap: 1.25rem;
            padding-top: 2rem;
        }

        .poster-area {
            width: 140px;
            align-self: center;
        }

        .poster {
            width: 140px;
            height: 210px;
        }

        .poster-placeholder {
            width: 140px;
            height: 210px;
            font-size: 3rem;
        }

        .info-area {
            text-align: left;
        }

        .media-title {
            font-size: 1.5rem;
        }

        .detail-backdrop {
            height: 280px;
        }

        .action-row {
            flex-wrap: wrap;
        }

        .file-row {
            flex-direction: column;
            align-items: flex-start;
            gap: 0.5rem;
        }

        .file-actions {
            width: 100%;
            justify-content: flex-end;
        }
    }
</style>
