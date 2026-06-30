<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import { onMount } from 'svelte';
    import {
        cancelMigration,
        createMigrationSource,
        deleteMigrationSource,
        discoverMigrationSource,
        getMigrationProgress,
        getMigrationReviewCsvUrl,
        getMigrationReviewItems,
        getMigrationRollbackStatus,
        getMigrationUserMappingOptions,
        listMigrationSources,
        matchMigrationItems,
        rollbackMigrationImport,
        runMigrationPreflight,
        saveMigrationUserMappings,
        startMigration,
        testMigrationConnection,
        uploadPlexMigrationDatabase,
        resolveMigrationReviewItem,
    } from '$lib/api/migrations.js';
    import { listMediaItems } from '$lib/api/media.js';
    import { hasCapability } from '$lib/stores/auth.js';
    import { events } from '$lib/stores/events.js';
    import { notifications } from '$lib/stores/notifications.js';

    let loading = $state(true);
    let loadedOnce = $state(false);
    let canManage = $state(false);
    let loadError = $state(null);
    let reviewError = $state(null);
    let actionMessage = $state(null);
    let actionError = $state(null);
    let migrations = $state([]);
    let selectedMigrationId = $state('');
    let selectedSource = $state('jellyfin');
    let wizardStep = $state('source');
    let operation = $state(null);
    let sourceForm = $state({
        name: '',
        base_url: '',
        api_key: '',
        plex_file: null,
    });
    let credentialApiKey = $state('');
    let preflightReport = $state(null);
    let mappingOptions = $state(null);
    let mappingRows = $state([]);
    let lastDiscovery = $state(null);
    let progress = $state(null);
    let reviewFilter = $state('needs_review');
    let reviewLoading = $state(false);
    let reviewItems = $state([]);
    let reviewTotal = $state(0);
    let rollbackLoading = $state(false);
    let rollbackRunning = $state(false);
    let rollbackStatus = $state(null);
    let rollbackError = $state(null);
    let mediaCandidates = $state({ movie: [], episode: [] });
    let manualMediaIds = $state({});
    let resolvingIds = $state({});

    const sources = [
        { id: 'jellyfin', label: m.routes_settings_migration_page_jellyfin(), detail: 'REST API' },
        { id: 'emby', label: m.routes_settings_migration_page_emby(), detail: 'REST API' },
        { id: 'plex', label: m.routes_settings_migration_page_plex(), detail: 'SQLite upload' },
    ];

    const steps = [
        { id: 'source', label: m.routes_settings_migration_page_source() },
        { id: 'connect', label: m.routes_settings_migration_page_connect() },
        { id: 'preflight', label: m.routes_settings_migration_page_preflight() },
        { id: 'users', label: m.routes_settings_migration_page_users() },
        { id: 'review', label: m.routes_settings_migration_page_review() },
        { id: 'import', label: m.routes_settings_migration_page_import() },
        { id: 'results', label: m.routes_settings_migration_page_results() },
    ];

    const reviewFilters = [
        { id: 'needs_review', label: m.routes_settings_migration_page_needs_review() },
        { id: 'unmatched', label: m.routes_settings_migration_page_unmatched() },
        { id: 'low_confidence', label: m.routes_settings_migration_page_low_confidence() },
        { id: 'all', label: m.routes_settings_migration_page_all_decisions() },
    ];

    let selectedMigration = $derived(
        migrations.find((migration) => migration.id === selectedMigrationId),
    );
    let selectedPlatform = $derived(selectedMigration?.platform || selectedSource);
    let selectedSourceMeta = $derived(sources.find((source) => source.id === selectedPlatform));
    let activeStepIndex = $derived(steps.findIndex((step) => step.id === wizardStep));
    let displayProgress = $derived(progress || progressFromSource(selectedMigration));
    let isActiveMigration = $derived(
        ['discovering', 'matching', 'importing'].includes(displayProgress?.status),
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

    onMount(() => {
        events.connect();
        const offMigrationProgress = events.on('migration_progress', async (payload) => {
            if (!payload || payload.migration_source_id !== selectedMigrationId) return;
            progress = {
                migration_source_id: payload.migration_source_id,
                status: payload.status,
                percent_complete: payload.percent_complete,
                items_discovered: payload.items_discovered,
                items_matched: payload.items_matched,
                items_unmatched: payload.items_unmatched,
                items_imported: payload.items_imported,
                items_skipped: payload.items_skipped,
                items_error: payload.items_error,
                items_processed: payload.items_processed,
            };
            if (['completed', 'failed', 'cancelled'].includes(payload.phase)) {
                await load(selectedMigrationId, { quiet: true });
                wizardStep = 'results';
            }
        });

        const timer = window.setInterval(() => {
            if (selectedMigrationId && isActiveMigration) {
                loadProgress({ quiet: true });
            }
        }, 5000);

        return () => {
            offMigrationProgress();
            window.clearInterval(timer);
        };
    });

    async function load(preferredId = selectedMigrationId, options = {}) {
        if (!options.quiet) {
            loading = true;
        }
        loadError = null;
        actionError = null;
        try {
            const response = await listMigrationSources({ page: 1, page_size: 50 });
            migrations = response.items || [];
            if (preferredId && migrations.some((item) => item.id === preferredId)) {
                selectedMigrationId = preferredId;
            } else if (!selectedMigrationId || !migrations.some((item) => item.id === selectedMigrationId)) {
                selectedMigrationId = migrations[0]?.id || '';
            }
            const currentSelection = migrations.find((item) => item.id === selectedMigrationId);
            if (currentSelection) {
                selectedSource = currentSelection.platform;
            }
            await loadCandidates();
            await refreshSelectedDetails({ quiet: true });
        } catch (err) {
            loadError = errorText(err, 'Failed to load migrations');
        } finally {
            loading = false;
        }
    }

    async function refreshSelectedDetails(options = {}) {
        await Promise.all([
            loadProgress(options),
            loadMappingOptions(options),
            loadReview(options),
            loadRollbackStatus(options),
        ]);
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

    async function loadProgress(options = {}) {
        if (!selectedMigrationId) {
            progress = null;
            return;
        }
        try {
            progress = await getMigrationProgress(selectedMigrationId);
        } catch (err) {
            if (!options.quiet) {
                actionError = errorText(err, 'Failed to load migration progress');
            }
        }
    }

    async function loadMappingOptions(options = {}) {
        if (!selectedMigrationId) {
            mappingOptions = null;
            mappingRows = [];
            return;
        }
        try {
            const response = await getMigrationUserMappingOptions(selectedMigrationId);
            mappingOptions = response;
            if (response.saved_mappings?.length) {
                mappingRows = rowsFromSavedMappings(response.saved_mappings);
            } else if (!mappingRows.length && lastDiscovery?.source_users?.length) {
                mappingRows = rowsFromSourceUsers(lastDiscovery.source_users);
            }
        } catch (err) {
            if (!options.quiet) {
                actionError = errorText(err, 'Failed to load user mapping options');
            }
        }
    }

    async function loadReview(options = {}) {
        if (!selectedMigrationId) {
            reviewItems = [];
            reviewTotal = 0;
            return;
        }
        reviewLoading = !options.quiet;
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
            reviewError = errorText(err, 'Failed to load review items');
        } finally {
            reviewLoading = false;
        }
    }

    async function loadRollbackStatus(options = {}) {
        if (!selectedMigrationId) {
            rollbackStatus = null;
            rollbackError = null;
            return;
        }
        rollbackLoading = !options.quiet;
        rollbackError = null;
        try {
            rollbackStatus = await getMigrationRollbackStatus(selectedMigrationId);
        } catch (err) {
            rollbackStatus = null;
            if (!options.quiet) {
                rollbackError = errorText(err, 'Failed to load rollback status');
            }
        } finally {
            rollbackLoading = false;
        }
    }

    async function selectMigration(id) {
        selectedMigrationId = id;
        const migration = migrations.find((item) => item.id === id);
        if (migration) {
            selectedSource = migration.platform;
        }
        actionMessage = null;
        actionError = null;
        preflightReport = null;
        lastDiscovery = null;
        mappingRows = [];
        await refreshSelectedDetails();
    }

    function selectSource(id) {
        selectedSource = id;
        selectedMigrationId = '';
        preflightReport = null;
        lastDiscovery = null;
        progress = null;
        mappingOptions = null;
        mappingRows = [];
    }

    async function createSource() {
        actionMessage = null;
        actionError = null;
        const name = sourceForm.name.trim();
        if (!name) {
            actionError = 'Source name is required.';
            return;
        }

        if (selectedSource === 'plex' && !sourceForm.plex_file) {
            actionError = 'Choose a Plex database file.';
            return;
        }

        if (selectedSource !== 'plex' && (!sourceForm.base_url.trim() || !sourceForm.api_key.trim())) {
            actionError = 'Base URL and API key are required.';
            return;
        }

        operation = 'create-source';
        try {
            const payload =
                selectedSource === 'plex'
                    ? {
                          platform: 'plex',
                          name,
                          connection_config: {
                              method: 'sqlite_upload',
                              original_filename: sourceForm.plex_file.name,
                              file_size_bytes: sourceForm.plex_file.size,
                          },
                      }
                    : {
                          platform: selectedSource,
                          name,
                          connection_config: {
                              method: 'api',
                              base_url: sourceForm.base_url,
                              api_key: sourceForm.api_key,
                          },
                      };

            const created = await createMigrationSource(payload);
            selectedMigrationId = created.id;
            selectedSource = created.platform;
            credentialApiKey = selectedSource === 'plex' ? '' : sourceForm.api_key;

            if (selectedSource === 'plex') {
                const uploadResponse = await uploadPlexMigrationDatabase(created.id, sourceForm.plex_file);
                actionMessage = uploadResponse.message;
            } else {
                actionMessage = 'Migration source created.';
            }

            sourceForm = { name: '', base_url: '', api_key: '', plex_file: null };
            await load(created.id);
            wizardStep = 'connect';
        } catch (err) {
            actionError = errorText(err, 'Failed to create migration source');
        } finally {
            operation = null;
        }
    }

    async function uploadPlexFile() {
        if (!selectedMigrationId || !sourceForm.plex_file) return;
        operation = 'upload';
        actionMessage = null;
        actionError = null;
        try {
            const response = await uploadPlexMigrationDatabase(selectedMigrationId, sourceForm.plex_file);
            actionMessage = response.message;
            await load(selectedMigrationId);
        } catch (err) {
            actionError = errorText(err, 'Failed to upload Plex database');
        } finally {
            operation = null;
        }
    }

    async function testConnection() {
        if (!selectedMigrationId) return;
        operation = 'connect';
        actionMessage = null;
        actionError = null;
        try {
            const response = await testMigrationConnection(selectedMigrationId, credentialPayload());
            actionMessage = response.message;
            await load(selectedMigrationId);
        } catch (err) {
            actionError = errorText(err, 'Failed to test source connection');
        } finally {
            operation = null;
        }
    }

    async function discoverSource() {
        if (!selectedMigrationId) return;
        operation = 'discover';
        actionMessage = null;
        actionError = null;
        try {
            const response = await discoverMigrationSource(selectedMigrationId, credentialPayload());
            lastDiscovery = response;
            actionMessage = response.message;
            if (response.source_users?.length) {
                mappingRows = rowsFromSourceUsers(response.source_users);
            }
            await load(selectedMigrationId, { quiet: true });
            wizardStep = response.users_mapped > 0 ? 'preflight' : 'users';
        } catch (err) {
            actionError = errorText(err, 'Failed to discover source data');
        } finally {
            operation = null;
        }
    }

    async function runPreflight() {
        if (!selectedMigrationId) return;
        operation = 'preflight';
        actionMessage = null;
        actionError = null;
        try {
            preflightReport = await runMigrationPreflight(selectedMigrationId);
            actionMessage = preflightReport.is_ready
                ? 'Preflight passed.'
                : `${preflightReport.blockers.length} blocker(s) found.`;
        } catch (err) {
            actionError = errorText(err, 'Failed to run preflight');
        } finally {
            operation = null;
        }
    }

    async function saveMappings({ extract = false } = {}) {
        if (!selectedMigrationId || !mappingRows.length) return;
        operation = extract ? 'save-extract' : 'save-mappings';
        actionMessage = null;
        actionError = null;
        try {
            await saveMigrationUserMappings(selectedMigrationId, {
                mappings: mappingRows.map((row) => ({
                    source_user_id: row.source_user_id,
                    source_user_name: row.source_user_name,
                    platform_user_id: row.skip ? null : row.platform_user_id || null,
                    skip: row.skip,
                })),
            });

            if (extract) {
                const response = await discoverMigrationSource(selectedMigrationId, credentialPayload());
                lastDiscovery = response;
                actionMessage = response.message;
                wizardStep = 'preflight';
            } else {
                actionMessage = 'User mappings saved.';
            }
            await load(selectedMigrationId, { quiet: true });
        } catch (err) {
            actionError = errorText(err, 'Failed to save user mappings');
        } finally {
            operation = null;
        }
    }

    async function runMatching() {
        if (!selectedMigrationId) return;
        operation = 'match';
        actionMessage = null;
        actionError = null;
        try {
            const response = await matchMigrationItems(selectedMigrationId);
            actionMessage = response.message;
            await Promise.all([loadReview(), loadProgress()]);
            wizardStep = 'review';
        } catch (err) {
            actionError = errorText(err, 'Failed to match migration items');
        } finally {
            operation = null;
        }
    }

    async function runDryRun() {
        await startSelectedMigration(true);
    }

    async function runImport() {
        await startSelectedMigration(false);
    }

    async function startSelectedMigration(dryRun) {
        if (!selectedMigrationId) return;
        operation = dryRun ? 'dry-run' : 'start-import';
        actionMessage = null;
        actionError = null;
        try {
            const response = await startMigration(selectedMigrationId, { dry_run: dryRun });
            actionMessage = response.message;
            await loadProgress();
            if (!dryRun) {
                wizardStep = 'import';
            }
        } catch (err) {
            actionError = errorText(err, dryRun ? 'Dry run failed' : 'Failed to start import');
        } finally {
            operation = null;
        }
    }

    async function cancelSelectedMigration() {
        if (!selectedMigrationId) return;
        operation = 'cancel';
        actionMessage = null;
        actionError = null;
        try {
            const response = await cancelMigration(selectedMigrationId);
            actionMessage = response.message;
            await load(selectedMigrationId, { quiet: true });
        } catch (err) {
            actionError = errorText(err, 'Failed to cancel migration');
        } finally {
            operation = null;
        }
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
            await Promise.all([loadReview(), loadProgress()]);
        } catch (err) {
            reviewError = errorText(err, 'Failed to save review decision');
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

    async function rollbackImport() {
        if (!selectedMigrationId || rollbackRunning) return;
        if (
            typeof window !== 'undefined' &&
            !window.confirm('Rollback imported watch state for this migration?')
        ) {
            return;
        }

        rollbackRunning = true;
        rollbackError = null;
        actionMessage = null;
        try {
            const response = await rollbackMigrationImport(selectedMigrationId);
            actionMessage = response.message;
            await Promise.all([loadReview(), loadRollbackStatus(), loadProgress()]);
        } catch (err) {
            rollbackError = errorText(err, 'Failed to rollback migration import');
        } finally {
            rollbackRunning = false;
        }
    }

    async function cleanupSelectedSource() {
        if (!selectedMigrationId) return;
        if (
            typeof window !== 'undefined' &&
            !window.confirm('Delete this migration source and its saved import logs?')
        ) {
            return;
        }

        operation = 'cleanup-source';
        actionMessage = null;
        actionError = null;
        try {
            await deleteMigrationSource(selectedMigrationId);
            notifications.success(m.routes_settings_migration_page_migration_source_deleted());
            selectedMigrationId = '';
            preflightReport = null;
            progress = null;
            mappingOptions = null;
            mappingRows = [];
            await load('', { quiet: true });
            wizardStep = 'source';
        } catch (err) {
            actionError = errorText(err, 'Failed to delete migration source');
        } finally {
            operation = null;
        }
    }

    function updateMapping(row, patch) {
        mappingRows = mappingRows.map((item) =>
            item.source_user_id === row.source_user_id ? { ...item, ...patch } : item,
        );
    }

    function credentialPayload() {
        if (selectedPlatform === 'plex') return {};
        const apiKey = credentialApiKey.trim() || sourceForm.api_key.trim();
        return apiKey ? { api_key: apiKey } : {};
    }

    function rowsFromSourceUsers(sourceUsers) {
        const saved = new Map((mappingOptions?.saved_mappings || []).map((item) => [item.source_user_id, item]));
        return sourceUsers.map((user) => {
            const existing = saved.get(user.source_user_id);
            return {
                source_user_id: user.source_user_id,
                source_user_name: user.source_user_name,
                platform_user_id: existing?.platform_user_id || '',
                skip: existing?.is_skipped || false,
            };
        });
    }

    function rowsFromSavedMappings(savedMappings) {
        return savedMappings.map((mapping) => ({
            source_user_id: mapping.source_user_id,
            source_user_name: mapping.source_user_name,
            platform_user_id: mapping.platform_user_id || '',
            skip: mapping.is_skipped || false,
        }));
    }

    function progressFromSource(source) {
        if (!source) return null;
        return {
            migration_source_id: source.id,
            status: source.status,
            percent_complete: source.status === 'completed' ? 100 : 0,
            items_discovered: 0,
            items_matched: 0,
            items_unmatched: 0,
            items_imported: 0,
            items_skipped: 0,
        };
    }

    function statusClass(status) {
        if (status === 'completed' || status === 'imported' || status === 'available') return 'ok';
        if (status === 'failed' || status === 'error' || status === 'cancelled') return 'bad';
        if (['discovering', 'matching', 'importing', 'blocked_by_newer_progress'].includes(status)) {
            return 'warn';
        }
        return '';
    }

    function stepState(index) {
        if (index < activeStepIndex) return 'done';
        if (index === activeStepIndex) return 'active';
        return '';
    }

    function isBusy(name) {
        return operation === name;
    }

    function activeSourceLabel() {
        return selectedSourceMeta?.label || selectedPlatform || 'Source';
    }

    function platformUsers() {
        return mappingOptions?.platform_users || [];
    }

    function formatDate(value) {
        if (!value) return 'Not run';
        return new Intl.DateTimeFormat(undefined, {
            dateStyle: 'medium',
            timeStyle: 'short',
        }).format(new Date(value));
    }

    function formatBytes(bytes) {
        if (!bytes && bytes !== 0) return '—';
        const units = ['B', 'KB', 'MB', 'GB', 'TB'];
        let value = Number(bytes);
        let index = 0;
        while (value >= 1024 && index < units.length - 1) {
            value /= 1024;
            index += 1;
        }
        return `${value.toFixed(index === 0 ? 0 : 1)} ${units[index]}`;
    }

    function formatPercent(value) {
        if (!value && value !== 0) return '0%';
        return `${Math.round(value)}%`;
    }

    function mediaYear(item) {
        if (!item?.premiere_date) return '';
        return new Date(item.premiere_date).getUTCFullYear();
    }

    function candidateOptions(type) {
        return mediaCandidates[type] || [];
    }

    function formatRollbackStatus(status) {
        const labels = {
            available: 'Available',
            blocked_by_newer_progress: 'Needs Review',
            rolled_back: 'Rolled Back',
            unavailable: 'Unavailable',
        };
        return labels[status] || status || 'Unavailable';
    }

    function providerSummary(providerIds) {
        if (!providerIds) return 'No provider IDs';
        const parts = [];
        if (providerIds.tmdb) parts.push(`TMDb ${providerIds.tmdb}`);
        if (providerIds.imdb) parts.push(`IMDb ${providerIds.imdb}`);
        if (providerIds.tvdb) parts.push(`TVDb ${providerIds.tvdb}`);
        return parts.length ? parts.join(' · ') : 'No provider IDs';
    }

    function errorText(err, fallback) {
        return err?.detail || err?.message || fallback;
    }
</script>

<div class="migration-settings">
    <div class="page-header">
        <div>
            <a href="/settings" class="back-link">{m.routes_settings_migration_page_settings()}</a>
            <h1 class="page-title">{m.routes_settings_migration_page_platform_migration()}</h1>
        </div>
        {#if !loading && canManage}
            <button class="btn-secondary" onclick={() => load()} disabled={!!operation}>{m.routes_settings_migration_page_refresh()}</button>
        {/if}
    </div>

    {#if loading}
        <div class="loading-state"><div class="loading-spinner"></div></div>
    {:else if !canManage}
        <div class="empty-state">{m.routes_settings_migration_page_you_do_not_have_permission_to_manage_users()}</div>
    {:else if loadError}
        <div class="empty-state stacked">
            <p class="error-text">{loadError}</p>
            <button class="btn-secondary" onclick={() => load()}>{m.routes_settings_migration_page_retry()}</button>
        </div>
    {:else}
        {#if actionMessage}
            <div class="notice success">{actionMessage}</div>
        {/if}
        {#if actionError}
            <div class="notice error">{actionError}</div>
        {/if}

        <section class="wizard-shell">
            <div class="step-strip">
                {#each steps as step, index}
                    <button
                        class="step"
                        class:active={stepState(index) === 'active'}
                        class:done={stepState(index) === 'done'}
                        onclick={() => (wizardStep = step.id)}
                    >
                        <span>{index + 1}</span>
                        <strong>{step.label}</strong>
                    </button>
                {/each}
            </div>

            {#if wizardStep === 'source'}
                <div class="wizard-panel">
                    <div class="source-grid">
                        {#each sources as source}
                            <button
                                class="source-tile"
                                class:selected={selectedSource === source.id && !selectedMigrationId}
                                onclick={() => selectSource(source.id)}
                            >
                                <span>{source.label}</span>
                                <strong>{source.detail}</strong>
                            </button>
                        {/each}
                    </div>

                    <div class="form-grid">
                        <label>
                            <span>{m.routes_settings_migration_page_name()}</span>
                            <input
                                type="text"
                                value={sourceForm.name}
                                oninput={(event) => (sourceForm.name = event.currentTarget.value)}
                                placeholder={`${activeSourceLabel()} migration`}
                            />
                        </label>
                        {#if selectedSource === 'plex'}
                            <label>
                                <span>{m.routes_settings_migration_page_plex_database()}</span>
                                <input
                                    type="file"
                                    accept=".db,.sqlite,application/vnd.sqlite3,application/octet-stream"
                                    onchange={(event) =>
                                        (sourceForm.plex_file = event.currentTarget.files?.[0] || null)}
                                />
                            </label>
                        {:else}
                            <label>
                                <span>{m.routes_settings_migration_page_base_url()}</span>
                                <input
                                    type="url"
                                    value={sourceForm.base_url}
                                    oninput={(event) =>
                                        (sourceForm.base_url = event.currentTarget.value)}
                                    placeholder={m.routes_settings_migration_page_https_media_example_test()}
                                />
                            </label>
                            <label>
                                <span>{m.routes_settings_migration_page_api_key()}</span>
                                <input
                                    type="password"
                                    value={sourceForm.api_key}
                                    oninput={(event) =>
                                        (sourceForm.api_key = event.currentTarget.value)}
                                    autocomplete="off"
                                />
                            </label>
                        {/if}
                    </div>

                    <div class="panel-actions">
                        <button
                            class="btn-primary"
                            disabled={!!operation}
                            onclick={createSource}
                        >
                            {isBusy('create-source') ? 'Creating…' : 'Create Source'}
                        </button>
                    </div>
                </div>
            {:else if wizardStep === 'connect'}
                <div class="wizard-panel">
                    <div class="selected-summary">
                        <div>
                            <span>{m.routes_settings_migration_page_selected()}</span>
                            <strong>{selectedMigration?.name || 'No source selected'}</strong>
                        </div>
                        <div>
                            <span>{m.routes_settings_migration_page_platform()}</span>
                            <strong>{activeSourceLabel()}</strong>
                        </div>
                        <div>
                            <span>{m.routes_settings_migration_page_status()}</span>
                            <strong class={statusClass(displayProgress?.status)}>
                                {displayProgress?.status || 'pending'}
                            </strong>
                        </div>
                    </div>

                    {#if selectedPlatform === 'plex'}
                        <div class="form-grid">
                            <label>
                                <span>{m.routes_settings_migration_page_plex_database()}</span>
                                <input
                                    type="file"
                                    accept=".db,.sqlite,application/vnd.sqlite3,application/octet-stream"
                                    onchange={(event) =>
                                        (sourceForm.plex_file = event.currentTarget.files?.[0] || null)}
                                />
                            </label>
                        </div>
                        <div class="panel-actions">
                            <button
                                class="btn-secondary"
                                disabled={!selectedMigrationId || !sourceForm.plex_file || !!operation}
                                onclick={uploadPlexFile}
                            >
                                {isBusy('upload') ? 'Uploading…' : 'Upload Database'}
                            </button>
                            <button
                                class="btn-primary"
                                disabled={!selectedMigrationId || !!operation}
                                onclick={discoverSource}
                            >
                                {isBusy('discover') ? 'Discovering…' : 'Discover Users'}
                            </button>
                        </div>
                    {:else}
                        <div class="form-grid">
                            <label>
                                <span>{m.routes_settings_migration_page_session_api_key()}</span>
                                <input
                                    type="password"
                                    value={credentialApiKey}
                                    oninput={(event) =>
                                        (credentialApiKey = event.currentTarget.value)}
                                    autocomplete="off"
                                />
                            </label>
                        </div>
                        <div class="panel-actions">
                            <button
                                class="btn-secondary"
                                disabled={!selectedMigrationId || !!operation}
                                onclick={testConnection}
                            >
                                {isBusy('connect') ? 'Testing…' : 'Test Connection'}
                            </button>
                            <button
                                class="btn-primary"
                                disabled={!selectedMigrationId || !!operation}
                                onclick={discoverSource}
                            >
                                {isBusy('discover') ? 'Discovering…' : 'Discover Users'}
                            </button>
                        </div>
                    {/if}
                </div>
            {:else if wizardStep === 'preflight'}
                <div class="wizard-panel">
                    <div class="panel-actions align-start">
                        <button
                            class="btn-primary"
                            disabled={!selectedMigrationId || !!operation}
                            onclick={runPreflight}
                        >
                            {isBusy('preflight') ? 'Running…' : 'Run Preflight'}
                        </button>
                        <button class="btn-secondary" onclick={() => (wizardStep = 'users')}>
                            User Mapping
                        </button>
                    </div>

                    {#if preflightReport}
                        <div class="preflight-grid">
                            <div class="summary-card">
                                <span>{m.routes_settings_migration_page_readiness()}</span>
                                <strong class={preflightReport.is_ready ? 'ok' : 'bad'}>
                                    {preflightReport.is_ready ? 'Ready' : 'Blocked'}
                                </strong>
                            </div>
                            <div class="summary-card">
                                <span>{m.routes_settings_migration_page_mappings()}</span>
                                <strong>{preflightReport.user_mapping_readiness.valid_mappings}</strong>
                            </div>
                            <div class="summary-card">
                                <span>{m.routes_settings_migration_page_estimated_matches()}</span>
                                <strong>{preflightReport.estimated_counts.estimated_matches}</strong>
                            </div>
                            <div class="summary-card">
                                <span>{m.routes_settings_migration_page_match_rate()}</span>
                                <strong>
                                    {formatPercent(preflightReport.estimated_counts.estimated_match_rate_percent)}
                                </strong>
                            </div>
                        </div>

                        <div class="check-list">
                            {#each preflightReport.checks as check}
                                <div class="check-row">
                                    <span class={statusClass(check.status)}>{check.status}</span>
                                    <strong>{check.name}</strong>
                                    <p>{check.message}</p>
                                </div>
                            {/each}
                        </div>

                        {#if preflightReport.blockers.length}
                            <div class="finding-list error">
                                {#each preflightReport.blockers as finding}
                                    <p>{finding.message}</p>
                                {/each}
                            </div>
                        {/if}
                        {#if preflightReport.warnings.length}
                            <div class="finding-list warn">
                                {#each preflightReport.warnings as finding}
                                    <p>{finding.message}</p>
                                {/each}
                            </div>
                        {/if}
                    {:else}
                        <div class="empty-state">{m.routes_settings_migration_page_no_preflight_report_has_been_run()}</div>
                    {/if}
                </div>
            {:else if wizardStep === 'users'}
                <div class="wizard-panel">
                    <div class="section-header compact">
                        <div>
                            <h2>{m.routes_settings_migration_page_user_mapping()}</h2>
                            <p>{mappingRows.length} source user{mappingRows.length === 1 ? '' : 's'}</p>
                        </div>
                        <button
                            class="btn-secondary"
                            disabled={!selectedMigrationId || !!operation}
                            onclick={discoverSource}
                        >
                            Refresh Users
                        </button>
                    </div>

                    {#if mappingRows.length === 0}
                        <div class="empty-state">{m.routes_settings_migration_page_no_source_users_discovered()}</div>
                    {:else}
                        <div class="mapping-list">
                            {#each mappingRows as row}
                                <div class="mapping-row">
                                    <div>
                                        <strong>{row.source_user_name}</strong>
                                        <span>{row.source_user_id}</span>
                                    </div>
                                    <select
                                        disabled={row.skip}
                                        value={row.platform_user_id}
                                        onchange={(event) =>
                                            updateMapping(row, {
                                                platform_user_id: event.currentTarget.value,
                                            })}
                                    >
                                        <option value="">{m.routes_settings_migration_page_choose_duskcue_user()}</option>
                                        {#each platformUsers() as user}
                                            <option value={user.platform_user_id}>{user.label}</option>
                                        {/each}
                                    </select>
                                    <label class="checkbox-label">
                                        <input
                                            type="checkbox"
                                            checked={row.skip}
                                            onchange={(event) =>
                                                updateMapping(row, {
                                                    skip: event.currentTarget.checked,
                                                })}
                                        />
                                        <span>{m.routes_settings_migration_page_skip()}</span>
                                    </label>
                                </div>
                            {/each}
                        </div>

                        <div class="panel-actions">
                            <button
                                class="btn-secondary"
                                disabled={!!operation}
                                onclick={() => saveMappings()}
                            >
                                {isBusy('save-mappings') ? 'Saving…' : 'Save Mappings'}
                            </button>
                            <button
                                class="btn-primary"
                                disabled={!!operation}
                                onclick={() => saveMappings({ extract: true })}
                            >
                                {isBusy('save-extract') ? 'Extracting…' : 'Save & Extract'}
                            </button>
                        </div>
                    {/if}
                </div>
            {:else if wizardStep === 'review'}
                <div class="wizard-panel">
                    <div class="section-header compact">
                        <div>
                            <h2>{m.routes_settings_migration_page_match_review()}</h2>
                            <p>{selectedMigration?.name || 'Select a migration source'}</p>
                        </div>
                        <div class="review-actions">
                            <span>{reviewTotal} item{reviewTotal === 1 ? '' : 's'}</span>
                            <button
                                class="btn-secondary"
                                disabled={!selectedMigrationId || !!operation}
                                onclick={runMatching}
                            >
                                {isBusy('match') ? 'Matching…' : 'Run Match'}
                            </button>
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

                    {#if reviewError}
                        <div class="notice error">{reviewError}</div>
                    {/if}

                    {#if reviewLoading}
                        <div class="loading-state small"><div class="loading-spinner"></div></div>
                    {:else if !selectedMigrationId}
                        <div class="empty-state">{m.routes_settings_migration_page_select_a_migration_source_to_review_matches()}</div>
                    {:else if reviewItems.length === 0}
                        <div class="empty-state">{m.routes_settings_migration_page_no_review_items_match_this_filter()}</div>
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
                                            placeholder={m.routes_settings_migration_page_media_item_id()}
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
                </div>
            {:else if wizardStep === 'import'}
                <div class="wizard-panel">
                    <div class="progress-panel">
                        <div class="progress-head">
                            <div>
                                <span>{m.routes_settings_migration_page_status()}</span>
                                <strong class={statusClass(displayProgress?.status)}>
                                    {displayProgress?.status || 'pending'}
                                </strong>
                            </div>
                            <strong>{formatPercent(displayProgress?.percent_complete)}</strong>
                        </div>
                        <div class="progress-track">
                            <div style={`width: ${displayProgress?.percent_complete || 0}%`}></div>
                        </div>
                        <div class="progress-grid">
                            <div><span>{m.routes_settings_migration_page_discovered()}</span><strong>{displayProgress?.items_discovered || 0}</strong></div>
                            <div><span>{m.routes_settings_migration_page_matched()}</span><strong>{displayProgress?.items_matched || 0}</strong></div>
                            <div><span>{m.routes_settings_migration_page_unmatched()}</span><strong>{displayProgress?.items_unmatched || 0}</strong></div>
                            <div><span>{m.routes_settings_migration_page_imported()}</span><strong>{displayProgress?.items_imported || 0}</strong></div>
                            <div><span>{m.routes_settings_migration_page_skipped()}</span><strong>{displayProgress?.items_skipped || 0}</strong></div>
                        </div>
                    </div>

                    <div class="panel-actions">
                        <button
                            class="btn-secondary"
                            disabled={!selectedMigrationId || !!operation}
                            onclick={runDryRun}
                        >
                            {isBusy('dry-run') ? 'Checking…' : 'Dry Run'}
                        </button>
                        <button
                            class="btn-primary"
                            disabled={!selectedMigrationId || !!operation || isActiveMigration}
                            onclick={runImport}
                        >
                            {isBusy('start-import') ? 'Starting…' : 'Start Import'}
                        </button>
                        <button
                            class="btn-secondary"
                            disabled={!selectedMigrationId || !!operation || !isActiveMigration}
                            onclick={cancelSelectedMigration}
                        >
                            {isBusy('cancel') ? 'Cancelling…' : 'Cancel'}
                        </button>
                    </div>
                </div>
            {:else if wizardStep === 'results'}
                <div class="wizard-panel">
                    <div class="result-grid">
                        <div class="summary-card">
                            <span>{m.routes_settings_migration_page_status()}</span>
                            <strong class={statusClass(displayProgress?.status)}>
                                {displayProgress?.status || 'pending'}
                            </strong>
                        </div>
                        <div class="summary-card">
                            <span>{m.routes_settings_migration_page_imported()}</span>
                            <strong>{displayProgress?.items_imported || 0}</strong>
                        </div>
                        <div class="summary-card">
                            <span>{m.routes_settings_migration_page_unmatched()}</span>
                            <strong>{displayProgress?.items_unmatched || 0}</strong>
                        </div>
                        <div class="summary-card">
                            <span>{m.routes_settings_migration_page_last_run()}</span>
                            <strong>{formatDate(selectedMigration?.last_run_at)}</strong>
                        </div>
                    </div>

                    <div class="rollback-grid">
                        <div>
                            <span>{m.routes_settings_migration_page_rollback()}</span>
                            <strong>{formatRollbackStatus(rollbackStatus?.status)}</strong>
                        </div>
                        <div>
                            <span>{m.routes_settings_migration_page_imported()}</span>
                            <strong>{rollbackStatus?.imported_count || 0}</strong>
                        </div>
                        <div>
                            <span>{m.routes_settings_migration_page_available()}</span>
                            <strong>{rollbackStatus?.rollback_available_count || 0}</strong>
                        </div>
                        <div>
                            <span>{m.routes_settings_migration_page_rolled_back()}</span>
                            <strong>{rollbackStatus?.rolled_back_count || 0}</strong>
                        </div>
                    </div>

                    {#if rollbackError}
                        <div class="notice error">{rollbackError}</div>
                    {/if}

                    <div class="panel-actions">
                        <button
                            class="btn-secondary"
                            disabled={!selectedMigrationId || rollbackLoading || rollbackRunning}
                            onclick={loadRollbackStatus}
                        >
                            Refresh Rollback
                        </button>
                        <button
                            class="btn-primary"
                            disabled={
                                rollbackRunning ||
                                !rollbackStatus ||
                                rollbackStatus.rollback_available_count <= 0
                            }
                            onclick={rollbackImport}
                        >
                            {rollbackRunning ? 'Rolling Back…' : 'Rollback Import'}
                        </button>
                        <button
                            class="btn-secondary danger"
                            disabled={!selectedMigrationId || !!operation || isActiveMigration}
                            onclick={cleanupSelectedSource}
                        >
                            {isBusy('cleanup-source') ? 'Deleting…' : 'Delete Source'}
                        </button>
                    </div>
                </div>
            {/if}
        </section>

        <section class="migration-list">
            <div class="section-header">
                <h2>{m.routes_settings_migration_page_migration_sources()}</h2>
                <span>{migrations.length}</span>
            </div>

            {#if migrations.length === 0}
                <div class="empty-state">{m.routes_settings_migration_page_no_migration_sources_have_been_created()}</div>
            {:else}
                <div class="table">
                    <div class="table-row table-head">
                        <span>{m.routes_settings_migration_page_name()}</span>
                        <span>{m.routes_settings_migration_page_platform()}</span>
                        <span>{m.routes_settings_migration_page_status()}</span>
                        <span>{m.routes_settings_migration_page_last_run()}</span>
                    </div>
                    {#each migrations as migration}
                        <button
                            class="table-row source-row"
                            class:selected={selectedMigrationId === migration.id}
                            onclick={() => selectMigration(migration.id)}
                        >
                            <span>{migration.name}</span>
                            <span>{migration.platform}</span>
                            <span class={statusClass(migration.status)}>{migration.status}</span>
                            <span>{formatDate(migration.last_run_at)}</span>
                        </button>
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
        max-width: 1180px;
    }

    .page-header,
    .section-header,
    .progress-head,
    .review-actions,
    .review-main,
    .manual-row,
    .decision-row,
    .panel-actions {
        display: flex;
        gap: 1rem;
    }

    .page-header,
    .section-header,
    .review-main,
    .progress-head {
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
    .migration-list {
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        background: var(--color-surface);
        padding: 1rem;
    }

    .step-strip {
        display: grid;
        grid-template-columns: repeat(7, minmax(0, 1fr));
        gap: 0.5rem;
        margin-bottom: 1rem;
    }

    .step {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        min-width: 0;
        min-height: 2.25rem;
        border: 0;
        background: transparent;
        color: var(--color-text-muted);
        font-size: 0.8125rem;
        text-align: start;
        cursor: pointer;
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

    .step.active,
    .step.done {
        color: var(--color-text-primary);
    }

    .step.active span {
        border-color: var(--color-accent);
        background: var(--color-accent-muted);
        color: var(--color-accent);
    }

    .step.done span {
        border-color: var(--color-success);
        color: var(--color-success);
    }

    .wizard-panel {
        display: grid;
        gap: 1rem;
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

    .form-grid {
        display: grid;
        grid-template-columns: repeat(3, minmax(0, 1fr));
        gap: 0.75rem;
    }

    label,
    .selected-summary div,
    .summary-card,
    .rollback-grid div,
    .progress-grid div {
        display: flex;
        flex-direction: column;
        gap: 0.35rem;
        min-width: 0;
    }

    label span,
    .selected-summary span,
    .summary-card span,
    .rollback-grid span,
    .progress-grid span,
    .progress-head span {
        color: var(--color-text-muted);
        font-size: 0.75rem;
        font-weight: 700;
        text-transform: uppercase;
    }

    input,
    select {
        min-width: 0;
        height: 2.5rem;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-background);
        color: var(--color-text-primary);
        padding: 0 0.75rem;
    }

    input[type='file'] {
        padding: 0.5rem 0.75rem;
    }

    .selected-summary,
    .preflight-grid,
    .result-grid {
        display: grid;
        grid-template-columns: repeat(4, minmax(0, 1fr));
        gap: 0.75rem;
    }

    .selected-summary div,
    .summary-card,
    .rollback-grid div,
    .progress-grid div {
        padding: 0.75rem;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-background);
    }

    .selected-summary strong,
    .summary-card strong,
    .rollback-grid strong,
    .progress-grid strong,
    .progress-head strong {
        overflow: hidden;
        color: var(--color-text-primary);
        font-size: 0.9375rem;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .panel-actions {
        align-items: center;
        flex-wrap: wrap;
        justify-content: flex-end;
        padding-top: 1rem;
        border-top: 1px solid var(--color-border);
    }

    .panel-actions.align-start {
        justify-content: flex-start;
        padding-top: 0;
        border-top: 0;
    }

    .section-header {
        align-items: center;
        margin-bottom: 0.75rem;
    }

    .section-header.compact {
        margin-bottom: 0;
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
        text-align: start;
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
    }

    .check-list,
    .mapping-list,
    .review-list {
        display: grid;
        gap: 0.75rem;
    }

    .check-row,
    .mapping-row,
    .review-item {
        display: grid;
        gap: 0.75rem;
        padding: 1rem;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-background);
    }

    .check-row {
        grid-template-columns: 6rem 10rem 1fr;
        align-items: center;
    }

    .check-row p,
    .mapping-row span,
    .review-main p {
        color: var(--color-text-muted);
        font-size: 0.8125rem;
    }

    .mapping-row {
        grid-template-columns: minmax(12rem, 1fr) minmax(14rem, 1fr) auto;
        align-items: center;
    }

    .checkbox-label {
        flex-direction: row;
        align-items: center;
        justify-content: flex-end;
    }

    .checkbox-label input {
        width: 1rem;
        height: 1rem;
    }

    .filter-strip {
        display: flex;
        flex-wrap: wrap;
        gap: 0.5rem;
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

    .review-main h3 {
        font-size: 1rem;
        font-weight: 700;
        color: var(--color-text-primary);
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

    .manual-row select {
        flex: 1 1 18rem;
    }

    .manual-row input {
        flex: 1 1 20rem;
        font-family: var(--font-mono);
    }

    .decision-row,
    .review-actions {
        align-items: center;
        flex-wrap: wrap;
        justify-content: flex-end;
    }

    .rollback-grid,
    .progress-grid {
        display: grid;
        grid-template-columns: repeat(4, minmax(0, 1fr));
        gap: 0.75rem;
    }

    .progress-grid {
        grid-template-columns: repeat(5, minmax(0, 1fr));
    }

    .progress-panel {
        display: grid;
        gap: 0.75rem;
    }

    .progress-track {
        height: 0.75rem;
        overflow: hidden;
        border-radius: 999px;
        background: var(--color-background);
        border: 1px solid var(--color-border);
    }

    .progress-track div {
        height: 100%;
        max-width: 100%;
        background: var(--color-accent);
    }

    .finding-list {
        display: grid;
        gap: 0.35rem;
        padding: 0.75rem;
        border-radius: var(--radius-sm);
        font-size: 0.875rem;
    }

    .finding-list.error {
        border: 1px solid var(--color-danger);
        color: var(--color-danger);
    }

    .finding-list.warn {
        border: 1px solid var(--color-warning);
        color: var(--color-warning);
    }

    .notice {
        padding: 0.75rem 1rem;
        border-radius: var(--radius-sm);
        font-size: 0.875rem;
    }

    .notice.success {
        border: 1px solid var(--color-success);
        color: var(--color-success);
    }

    .notice.error {
        border: 1px solid var(--color-danger);
        color: var(--color-danger);
    }

    .ok {
        color: var(--color-success) !important;
    }

    .warn {
        color: var(--color-warning) !important;
    }

    .bad {
        color: var(--color-danger) !important;
    }

    .danger {
        color: var(--color-danger);
    }

    .loading-state {
        display: grid;
        min-height: 10rem;
        place-items: center;
    }

    .loading-state.small {
        min-height: 4rem;
    }

    .loading-spinner {
        width: 2rem;
        height: 2rem;
        border: 2px solid var(--color-border);
        border-top-color: var(--color-accent);
        border-radius: 50%;
        animation: spin 0.8s linear infinite;
    }

    .empty-state {
        padding: 1.5rem;
        border: 1px dashed var(--color-border);
        border-radius: var(--radius-sm);
        color: var(--color-text-muted);
        text-align: center;
    }

    .empty-state.stacked {
        display: grid;
        gap: 1rem;
        justify-items: center;
    }

    .error-text {
        color: var(--color-danger);
    }

    @keyframes spin {
        to {
            transform: rotate(360deg);
        }
    }

    @media (max-width: 900px) {
        .step-strip,
        .source-grid,
        .form-grid,
        .selected-summary,
        .preflight-grid,
        .result-grid,
        .rollback-grid,
        .progress-grid {
            grid-template-columns: repeat(2, minmax(0, 1fr));
        }

        .mapping-row,
        .check-row,
        .table-row {
            grid-template-columns: 1fr;
        }

        .manual-row {
            flex-direction: column;
            align-items: stretch;
        }
    }

    @media (max-width: 640px) {
        .step-strip,
        .source-grid,
        .form-grid,
        .selected-summary,
        .preflight-grid,
        .result-grid,
        .rollback-grid,
        .progress-grid {
            grid-template-columns: 1fr;
        }

        .page-header,
        .section-header,
        .panel-actions {
            flex-direction: column;
            align-items: stretch;
        }

        .review-actions,
        .decision-row {
            justify-content: flex-start;
        }
    }
</style>
