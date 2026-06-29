<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import {
        getMigrationReviewCsvUrl,
        getMigrationReviewItems,
        listMigrationSources,
        resolveMigrationReviewItem,
    } from '$lib/api/migrations.js';
    import { listMediaItems } from '$lib/api/media.js';
    import { hasCapability } from '$lib/stores/auth.js';

    let loading = $state(true);
    let loadedOnce = $state(false);
    let canManage = $state(false);
    let loadError = $state(null);
    let reviewError = $state(null);
    let actionMessage = $state(null);
    let migrations = $state([]);
    let selectedMigrationId = $state('');
    let selectedSource = $state('jellyfin');
    let reviewFilter = $state('needs_review');
    let reviewLoading = $state(false);
    let reviewItems = $state([]);
    let reviewTotal = $state(0);
    let mediaCandidates = $state({ movie: [], episode: [] });
    let manualMediaIds = $state({});
    let resolvingIds = $state({});

    const sources = [
        { id: 'jellyfin', label: 'Jellyfin', detail: 'REST API' },
        { id: 'emby', label: 'Emby', detail: 'REST API' },
        { id: 'plex', label: 'Plex', detail: 'SQLite upload' },
    ];

    const steps = ['Source', 'Connect', 'Preflight', 'Users', 'Review', 'Import'];
    const reviewFilters = [
        { id: 'needs_review', label: 'Needs Review' },
        { id: 'unmatched', label: 'Unmatched' },
        { id: 'low_confidence', label: 'Low Confidence' },
        { id: 'all', label: 'All Decisions' },
    ];

    let selectedMigration = $derived(
        migrations.find((migration) => migration.id === selectedMigrationId),
    );

    $effect(() => {
        const unsub = hasCapability('can_manage_users').subscribe((value) => {
            canManage = value;
        });
        return unsub;
    });

    $effect(() => {
        if (canManage && !loadedOnce) {
            loadedOnce = true;
            load();
        } else if (!canManage) {
            loading = false;
        }
    });

    async function load() {
        loading = true;
        loadError = null;
        actionMessage = null;
        try {
            const response = await listMigrationSources({ page: 1, page_size: 25 });
            migrations = response.items || [];
            if (!selectedMigrationId || !migrations.some((item) => item.id === selectedMigrationId)) {
                selectedMigrationId = migrations[0]?.id || '';
            }
            await loadCandidates();
            await loadReview();
        } catch (err) {
            loadError = err.detail || err.message || 'Failed to load migrations';
        } finally {
            loading = false;
        }
    }

    async function loadCandidates() {
        try {
            const [movies, episodes] = await Promise.all([
                listMediaItems({ type: 'movie', limit: 100, order: 'desc' }),
                listMediaItems({ type: 'episode', limit: 100, order: 'desc' }),
            ]);
            mediaCandidates = {
                movie: movies.items || [],
                episode: episodes.items || [],
            };
        } catch {
            mediaCandidates = { movie: [], episode: [] };
        }
    }

    async function loadReview() {
        if (!selectedMigrationId) {
            reviewItems = [];
            reviewTotal = 0;
            return;
        }
        reviewLoading = true;
        reviewError = null;
        try {
            const response = await getMigrationReviewItems(selectedMigrationId, {
                status: reviewFilter,
                page: 1,
                page_size: 50,
            });
            reviewItems = response.items || [];
            reviewTotal = response.total || 0;
            const nextManualIds = { ...manualMediaIds };
            for (const item of reviewItems) {
                if (!nextManualIds[item.id] && item.matched_media_item_id) {
                    nextManualIds[item.id] = item.matched_media_item_id;
                }
            }
            manualMediaIds = nextManualIds;
        } catch (err) {
            reviewError = err.detail || err.message || 'Failed to load review items';
        } finally {
            reviewLoading = false;
        }
    }

    async function selectMigration(id) {
        selectedMigrationId = id;
        actionMessage = null;
        await loadReview();
    }

    async function setReviewFilter(value) {
        reviewFilter = value;
        actionMessage = null;
        await loadReview();
    }

    function setManualMediaId(itemId, value) {
        manualMediaIds = { ...manualMediaIds, [itemId]: value };
    }

    async function resolveItem(item, action) {
        if (!selectedMigrationId || resolvingIds[item.id]) return;
        const mediaItemId = manualMediaIds[item.id]?.trim();
        if (action === 'match' && !mediaItemId) {
            reviewError = 'Choose or enter a media item ID before matching.';
            return;
        }

        resolvingIds = { ...resolvingIds, [item.id]: true };
        reviewError = null;
        actionMessage = null;
        try {
            const response = await resolveMigrationReviewItem(selectedMigrationId, item.id, {
                action,
                media_item_id: action === 'match' ? mediaItemId : null,
            });
            actionMessage = response.message;
            await loadReview();
        } catch (err) {
            reviewError = err.detail || err.message || 'Failed to save review decision';
        } finally {
            const next = { ...resolvingIds };
            delete next[item.id];
            resolvingIds = next;
        }
    }

    function exportCsv() {
        if (!selectedMigrationId || typeof window === 'undefined') return;
        window.location.href = getMigrationReviewCsvUrl(selectedMigrationId, {
            status: reviewFilter,
        });
    }

    function formatDate(value) {
        if (!value) return 'Not run';
        return new Intl.DateTimeFormat(undefined, {
            dateStyle: 'medium',
            timeStyle: 'short',
        }).format(new Date(value));
    }

    function mediaYear(item) {
        if (!item?.premiere_date) return '';
        return new Date(item.premiere_date).getUTCFullYear();
    }

    function candidateOptions(type) {
        return mediaCandidates[type] || [];
    }

    function providerSummary(providerIds) {
        if (!providerIds) return 'No provider IDs';
        const parts = [];
        if (providerIds.tmdb) parts.push(`TMDb ${providerIds.tmdb}`);
        if (providerIds.imdb) parts.push(`IMDb ${providerIds.imdb}`);
        if (providerIds.tvdb) parts.push(`TVDb ${providerIds.tvdb}`);
        return parts.length ? parts.join(' · ') : 'No provider IDs';
    }
</script>

<div class="migration-settings">
    <div class="page-header">
        <div>
            <a href="/settings" class="back-link">← Settings</a>
            <h1 class="page-title">Platform Migration</h1>
        </div>
        {#if !loading && canManage}
            <button class="btn-secondary" onclick={load}>Refresh</button>
        {/if}
    </div>

    {#if loading}
        <div class="loading-state"><div class="loading-spinner"></div></div>
    {:else if !canManage}
        <div class="empty-state">You do not have permission to manage users.</div>
    {:else if loadError}
        <div class="empty-state stacked">
            <p class="error-text">{loadError}</p>
            <button class="btn-secondary" onclick={load}>Retry</button>
        </div>
    {:else}
        <section class="wizard-shell">
            <div class="step-strip">
                {#each steps as step, index}
                    <div class="step" class:active={step === 'Review'}>
                        <span>{index + 1}</span>
                        <strong>{step}</strong>
                    </div>
                {/each}
            </div>

            <div class="source-grid">
                {#each sources as source}
                    <button
                        class="source-tile"
                        class:selected={selectedSource === source.id}
                        onclick={() => (selectedSource = source.id)}
                    >
                        <span>{source.label}</span>
                        <strong>{source.detail}</strong>
                    </button>
                {/each}
            </div>

            <div class="connect-panel">
                <div>
                    <span class="panel-label">Selected Source</span>
                    <strong>{sources.find((source) => source.id === selectedSource)?.label}</strong>
                </div>
                <button class="btn-primary" disabled>Create Source</button>
            </div>
        </section>

        <section class="migration-list">
            <div class="section-header">
                <h2>Migration Sources</h2>
                <span>{migrations.length}</span>
            </div>

            {#if migrations.length === 0}
                <div class="empty-state">No migration sources have been created.</div>
            {:else}
                <div class="table">
                    <div class="table-row table-head">
                        <span>Name</span>
                        <span>Platform</span>
                        <span>Status</span>
                        <span>Last Run</span>
                    </div>
                    {#each migrations as migration}
                        <button
                            class="table-row source-row"
                            class:selected={selectedMigrationId === migration.id}
                            onclick={() => selectMigration(migration.id)}
                        >
                            <span>{migration.name}</span>
                            <span>{migration.platform}</span>
                            <span>{migration.status}</span>
                            <span>{formatDate(migration.last_run_at)}</span>
                        </button>
                    {/each}
                </div>
            {/if}
        </section>

        <section class="review-panel">
            <div class="section-header">
                <div>
                    <h2>Match Review</h2>
                    <p>{selectedMigration?.name || 'Select a migration source'}</p>
                </div>
                <div class="review-actions">
                    <span>{reviewTotal} item{reviewTotal === 1 ? '' : 's'}</span>
                    <button class="btn-secondary" disabled={!selectedMigrationId} onclick={exportCsv}>
                        Export CSV
                    </button>
                </div>
            </div>

            <div class="filter-strip">
                {#each reviewFilters as filter}
                    <button
                        class:active={reviewFilter === filter.id}
                        onclick={() => setReviewFilter(filter.id)}
                    >
                        {filter.label}
                    </button>
                {/each}
            </div>

            {#if actionMessage}
                <div class="notice success">{actionMessage}</div>
            {/if}
            {#if reviewError}
                <div class="notice error">{reviewError}</div>
            {/if}

            {#if reviewLoading}
                <div class="loading-state small"><div class="loading-spinner"></div></div>
            {:else if !selectedMigrationId}
                <div class="empty-state">Select a migration source to review matches.</div>
            {:else if reviewItems.length === 0}
                <div class="empty-state">No review items match this filter.</div>
            {:else}
                <div class="review-list">
                    {#each reviewItems as item}
                        <article class="review-item">
                            <div class="review-main">
                                <div>
                                    <h3>{item.source_item_title}</h3>
                                    <p>
                                        {item.source_item_type}
                                        {#if item.source_item_year}
                                            · {item.source_item_year}
                                        {/if}
                                        · {providerSummary(item.source_provider_ids)}
                                    </p>
                                </div>
                                <div class="badges">
                                    <span>{item.status}</span>
                                    <span>{item.match_confidence || 'unknown'}</span>
                                </div>
                            </div>

                            {#if item.error_detail}
                                <p class="review-detail">{item.error_detail}</p>
                            {:else if item.matched_media_title}
                                <p class="review-detail">
                                    Current match: {item.matched_media_title}
                                    {#if item.matched_media_year}
                                        ({item.matched_media_year})
                                    {/if}
                                </p>
                            {/if}

                            <div class="manual-row">
                                <select
                                    value={manualMediaIds[item.id] || item.matched_media_item_id || ''}
                                    onchange={(event) =>
                                        setManualMediaId(item.id, event.currentTarget.value)}
                                >
                                    <option value="">Choose recent {item.source_item_type}</option>
                                    {#each candidateOptions(item.source_item_type) as candidate}
                                        <option value={candidate.id}>
                                            {candidate.title}{mediaYear(candidate) ? ` (${mediaYear(candidate)})` : ''}
                                        </option>
                                    {/each}
                                </select>
                                <input
                                    type="text"
                                    placeholder="media_item_id"
                                    value={manualMediaIds[item.id] || item.matched_media_item_id || ''}
                                    oninput={(event) =>
                                        setManualMediaId(item.id, event.currentTarget.value)}
                                />
                            </div>

                            <div class="decision-row">
                                <button
                                    class="btn-primary"
                                    disabled={resolvingIds[item.id]}
                                    onclick={() => resolveItem(item, 'match')}
                                >
                                    Match
                                </button>
                                <button
                                    class="btn-secondary"
                                    disabled={resolvingIds[item.id]}
                                    onclick={() => resolveItem(item, 'skip')}
                                >
                                    Skip
                                </button>
                                <button
                                    class="btn-secondary"
                                    disabled={resolvingIds[item.id]}
                                    onclick={() => resolveItem(item, 'ignore')}
                                >
                                    Ignore
                                </button>
                            </div>
                        </article>
                    {/each}
                </div>
            {/if}
        </section>
    {/if}
</div>

<style>
    .migration-settings {
        display: flex;
        flex-direction: column;
        gap: 1.5rem;
        max-width: 1100px;
    }

    .page-header,
    .connect-panel,
    .section-header,
    .review-actions,
    .review-main,
    .manual-row,
    .decision-row {
        display: flex;
        gap: 1rem;
    }

    .page-header,
    .connect-panel,
    .section-header,
    .review-main {
        align-items: flex-start;
        justify-content: space-between;
    }

    .back-link {
        font-size: 0.8125rem;
        color: var(--color-text-muted);
    }

    .back-link:hover {
        color: var(--color-text-secondary);
    }

    .page-title {
        margin-top: 0.5rem;
        font-size: 1.75rem;
        font-weight: 700;
        color: var(--color-text-primary);
    }

    .wizard-shell,
    .migration-list,
    .review-panel {
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        background: var(--color-surface);
        padding: 1rem;
    }

    .step-strip {
        display: grid;
        grid-template-columns: repeat(6, minmax(0, 1fr));
        gap: 0.5rem;
        margin-bottom: 1rem;
    }

    .step {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        min-width: 0;
        color: var(--color-text-muted);
        font-size: 0.8125rem;
    }

    .step span {
        display: grid;
        place-items: center;
        width: 1.5rem;
        height: 1.5rem;
        flex: 0 0 auto;
        border-radius: 50%;
        border: 1px solid var(--color-border);
        font-size: 0.75rem;
    }

    .step strong {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .step.active {
        color: var(--color-text-primary);
    }

    .step.active span {
        border-color: var(--color-accent);
        background: var(--color-accent-muted);
        color: var(--color-accent);
    }

    .source-grid {
        display: grid;
        grid-template-columns: repeat(3, minmax(0, 1fr));
        gap: 0.75rem;
    }

    .source-tile {
        display: flex;
        flex-direction: column;
        align-items: flex-start;
        gap: 0.35rem;
        min-height: 5rem;
        padding: 1rem;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-background);
        color: var(--color-text-primary);
        cursor: pointer;
    }

    .source-tile.selected,
    .source-row.selected {
        border-color: var(--color-accent);
        box-shadow: 0 0 0 1px var(--color-accent);
    }

    .source-tile span {
        font-size: 1rem;
        font-weight: 700;
    }

    .source-tile strong,
    .section-header p,
    .review-detail {
        font-size: 0.8125rem;
        color: var(--color-text-secondary);
        font-weight: 500;
    }

    .connect-panel {
        align-items: center;
        margin-top: 1rem;
        padding-top: 1rem;
        border-top: 1px solid var(--color-border);
    }

    .connect-panel div {
        display: flex;
        flex-direction: column;
        gap: 0.25rem;
    }

    .panel-label {
        font-size: 0.75rem;
        color: var(--color-text-muted);
        text-transform: uppercase;
        letter-spacing: 0.04em;
    }

    .section-header {
        align-items: center;
        margin-bottom: 0.75rem;
    }

    .section-header h2 {
        font-size: 1rem;
        font-weight: 700;
        color: var(--color-text-primary);
    }

    .section-header span {
        font-size: 0.8125rem;
        color: var(--color-text-muted);
    }

    .table {
        display: grid;
        gap: 0.25rem;
    }

    .table-row {
        display: grid;
        grid-template-columns: 2fr 1fr 1fr 1.25fr;
        gap: 1rem;
        align-items: center;
        min-height: 2.5rem;
        padding: 0.5rem 0.75rem;
        border-radius: var(--radius-sm);
        background: var(--color-background);
        color: var(--color-text-secondary);
        font-size: 0.875rem;
        text-align: left;
    }

    .source-row {
        width: 100%;
        border: 1px solid transparent;
        cursor: pointer;
    }

    .table-head {
        background: transparent;
        color: var(--color-text-muted);
        font-size: 0.75rem;
        font-weight: 700;
        text-transform: uppercase;
        letter-spacing: 0.04em;
    }

    .filter-strip {
        display: flex;
        flex-wrap: wrap;
        gap: 0.5rem;
        margin-bottom: 1rem;
    }

    .filter-strip button {
        min-height: 2rem;
        padding: 0 0.75rem;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-background);
        color: var(--color-text-secondary);
        cursor: pointer;
    }

    .filter-strip button.active {
        border-color: var(--color-accent);
        color: var(--color-accent);
    }

    .review-list {
        display: grid;
        gap: 0.75rem;
    }

    .review-item {
        display: grid;
        gap: 0.75rem;
        padding: 1rem;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-background);
    }

    .review-main h3 {
        font-size: 1rem;
        font-weight: 700;
        color: var(--color-text-primary);
    }

    .review-main p {
        margin-top: 0.25rem;
        font-size: 0.8125rem;
        color: var(--color-text-muted);
    }

    .badges {
        display: flex;
        flex-wrap: wrap;
        gap: 0.35rem;
        justify-content: flex-end;
    }

    .badges span {
        padding: 0.2rem 0.5rem;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        color: var(--color-text-secondary);
        font-size: 0.75rem;
        text-transform: uppercase;
    }

    .manual-row {
        align-items: center;
    }

    .manual-row select,
    .manual-row input {
        min-width: 0;
        height: 2.5rem;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-surface);
        color: var(--color-text-primary);
        padding: 0 0.75rem;
    }

    .manual-row select {
        flex: 1 1 18rem;
    }

    .manual-row input {
        flex: 1 1 20rem;
        font-family: var(--font-mono);
    }

    .decision-row {
        flex-wrap: wrap;
    }

    .review-actions {
        align-items: center;
        flex-wrap: wrap;
        justify-content: flex-end;
    }

    .notice {
        margin-bottom: 0.75rem;
        padding: 0.75rem;
        border-radius: var(--radius-sm);
        font-size: 0.875rem;
    }

    .notice.success {
        color: var(--color-success);
        background: color-mix(in srgb, var(--color-success) 12%, transparent);
    }

    .notice.error,
    .error-text {
        color: var(--color-danger);
    }

    .empty-state,
    .loading-state {
        display: flex;
        min-height: 8rem;
        align-items: center;
        justify-content: center;
        text-align: center;
        color: var(--color-text-muted);
    }

    .loading-state.small {
        min-height: 5rem;
    }

    .stacked {
        flex-direction: column;
        gap: 1rem;
    }

    @media (max-width: 760px) {
        .page-header,
        .connect-panel,
        .section-header,
        .review-main,
        .manual-row {
            flex-direction: column;
            align-items: stretch;
        }

        .step-strip,
        .source-grid,
        .table-row {
            grid-template-columns: 1fr;
        }

        .table-head {
            display: none;
        }

        .badges,
        .review-actions {
            justify-content: flex-start;
        }
    }
</style>
