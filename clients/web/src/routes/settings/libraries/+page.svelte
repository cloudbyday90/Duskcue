<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import { onMount } from 'svelte';
    import { libraries, libraryList } from '$lib/stores/libraries.js';
    import { hasCapability } from '$lib/stores/auth.js';
    import { notifications } from '$lib/stores/notifications.js';
    import { MEDIA_TYPE_LABELS } from '$lib/utils/constants.js';
    import { isTauriDesktop, pickLibraryFolder } from '$lib/desktop/tauri.js';

    let loading = $state(true);
    let showCreate = $state(false);
    let newName = $state('');
    let newType = $state('movie');
    let newRootPath = $state('');
    let creating = $state(false);
    let canManage = $state(false);
    let desktopShell = $state(false);
    let expandedPaths = $state({});

    $effect(() => {
        const unsub = hasCapability('can_manage_libraries').subscribe((v) => (canManage = v));
        return unsub;
    });

    onMount(async () => {
        desktopShell = isTauriDesktop();
        await libraries.fetch();
        loading = false;
    });

    async function handleCreate() {
        if (!newName.trim() || !newRootPath.trim()) {
            notifications.error(m.routes_settings_libraries_page_name_and_root_path_are_required());
            return;
        }
        creating = true;
        try {
            const lib = await libraries.create({
                name: newName.trim(),
                media_type: newType,
                root_path: newRootPath.trim(),
            });
            notifications.success(`Library "${lib.name}" created`);
            showCreate = false;
            newName = '';
            newRootPath = '';
        } catch (err) {
            notifications.error(err.detail || err.message || m.routes_settings_libraries_page_failed_to_create_library());
        } finally {
            creating = false;
        }
    }

    async function handlePickRootPath() {
        try {
            const folder = await pickLibraryFolder();
            if (folder) {
                newRootPath = folder;
            }
        } catch (err) {
            notifications.error(err.message || 'Failed to open folder picker');
        }
    }

    async function handleScan(libraryId) {
        try {
            await libraries.scan(libraryId, 'full');
            notifications.success(m.routes_settings_libraries_page_scan_complete());
        } catch (err) {
            notifications.error(err.detail || err.message || m.routes_settings_libraries_page_scan_failed());
        }
    }

    async function handleDelete(libraryId, name) {
        if (!confirm(`Delete library "${name}"? This will soft-delete the library.`)) return;
        try {
            await libraries.remove(libraryId);
            notifications.success(m.routes_settings_libraries_page_library_deleted());
        } catch (err) {
            notifications.error(err.detail || m.routes_settings_libraries_page_failed_to_delete_library());
        }
    }

    async function togglePaths(libraryId) {
        if (expandedPaths[libraryId]) {
            expandedPaths = { ...expandedPaths, [libraryId]: false };
        } else {
            expandedPaths = { ...expandedPaths, [libraryId]: true };
            if (!libraries.getById(libraryId)?.paths) {
                await libraries.fetchPaths(libraryId);
            }
        }
    }

    function getPaths(libraryId) {
        let paths = [];
        const unsub = libraries.subscribe((s) => {
            paths = s.paths[libraryId] || [];
        });
        unsub();
        return paths;
    }
</script>

<div class="lib-settings">
    <div class="page-header">
        <div>
            <a href="/settings" class="back-link">{m.routes_settings_libraries_page_settings()}</a>
            <h1 class="page-title">{m.routes_settings_libraries_page_library_management()}</h1>
        </div>
        {#if canManage}
            <button class="btn-primary" onclick={() => (showCreate = !showCreate)}>
                {showCreate ? 'Cancel' : 'New Library'}
            </button>
        {/if}
    </div>

    {#if showCreate}
        <div class="create-form">
            <h3 class="form-title">{m.routes_settings_libraries_page_create_library()}</h3>
            <div class="form-grid">
                <label class="field">
                    <span class="field-label">{m.routes_settings_libraries_page_library_name()}</span>
                    <input type="text" bind:value={newName} placeholder={m.routes_settings_libraries_page_my_movies()} />
                </label>
                <label class="field">
                    <span class="field-label">{m.routes_settings_libraries_page_media_type()}</span>
                    <select bind:value={newType}>
                        <option value="movie">{m.routes_settings_libraries_page_movies()}</option>
                        <option value="series">{m.routes_settings_libraries_page_tv_shows()}</option>
                        <option value="music">{m.routes_settings_libraries_page_music()}</option>
                    </select>
                </label>
                <label class="field field-wide">
                    <span class="field-label">{m.routes_settings_libraries_page_root_path()}</span>
                    <div class="path-input-row">
                        <input type="text" bind:value={newRootPath} placeholder={m.routes_settings_libraries_page_media_movies()} />
                        {#if desktopShell}
                            <button type="button" class="btn-secondary-sm" onclick={handlePickRootPath}>Browse</button>
                        {/if}
                    </div>
                </label>
            </div>
            <button class="btn-primary" onclick={handleCreate} disabled={creating}>
                {creating ? 'Creating…' : 'Create Library'}
            </button>
        </div>
    {/if}

    {#if loading}
        <div class="loading-state">
            <div class="loading-spinner"></div>
        </div>
    {:else if $libraryList.length === 0}
        <div class="empty-state">
            <p>{m.routes_settings_libraries_page_no_libraries_configured()}</p>
            {#if canManage}
                <button class="btn-primary" onclick={() => (showCreate = true)}>{m.routes_settings_libraries_page_create_your_first_library()}</button>
            {/if}
        </div>
    {:else}
        <div class="library-list">
            {#each $libraryList as lib (lib.id)}
                <div class="library-item">
                    <div class="library-item-header">
                        <div class="library-item-info">
                            <a href="/libraries/{lib.id}" class="library-name">{lib.name}</a>
                            <div class="library-meta">
                                <span class="badge">{MEDIA_TYPE_LABELS[lib.media_type] || lib.media_type}</span>
                                <span class="item-count">{lib.item_count || 0} items</span>
                                {#if lib.scan_enabled === false}
                                    <span class="badge-inactive">{m.routes_settings_libraries_page_scanning_disabled()}</span>
                                {/if}
                            </div>
                        </div>
                        {#if canManage}
                            <div class="library-actions">
                                <button
                                    class="btn-secondary-sm"
                                    onclick={() => handleScan(lib.id)}
                                    disabled={libraries.isScanning(lib.id)}
                                >
                                    {libraries.isScanning(lib.id) ? 'Scanning…' : 'Scan'}
                                </button>
                                <button class="btn-secondary-sm" onclick={() => togglePaths(lib.id)}>
                                    {expandedPaths[lib.id] ? 'Hide Paths' : 'Paths'}
                                </button>
                                <button class="btn-danger-sm" onclick={() => handleDelete(lib.id, lib.name)}>
                                    Delete
                                </button>
                            </div>
                        {/if}
                    </div>

                    {#if expandedPaths[lib.id]}
                        <div class="paths-section">
                            {#if getPaths(lib.id).length > 0}
                                {#each getPaths(lib.id) as path (path.id)}
                                    <div class="path-row">
                                        <span class="path-text">{path.root_path}</span>
                                        <div class="path-flags">
                                            {#if path.is_default}<span class="flag-badge">{m.routes_settings_libraries_page_default()}</span>{/if}
                                            {#if path.scan_enabled}<span class="flag-badge flag-on">{m.routes_settings_libraries_page_scan_on()}</span>{:else}<span class="flag-badge flag-off">{m.routes_settings_libraries_page_scan_off()}</span>{/if}
                                        </div>
                                    </div>
                                {/each}
                            {:else}
                                <p class="paths-loading">{m.routes_settings_libraries_page_loading_paths()}</p>
                            {/if}
                        </div>
                    {/if}
                </div>
            {/each}
        </div>
    {/if}
</div>

<style>
    .lib-settings {
        display: flex;
        flex-direction: column;
        gap: 1.5rem;
        max-width: 900px;
    }

    .page-header {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 1rem;
    }

    .path-input-row {
        display: flex;
        gap: 0.5rem;
    }

    .path-input-row input {
        flex: 1;
    }

    .back-link {
        font-size: 0.8125rem;
        color: var(--color-text-muted);
    }

    .page-title {
        font-size: 1.5rem;
        font-weight: 700;
        color: var(--color-text-primary);
        margin-top: 0.25rem;
    }

    .create-form {
        display: flex;
        flex-direction: column;
        gap: 1rem;
        padding: 1.5rem;
        background-color: var(--color-bg-surface);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-md);
    }

    .form-title {
        font-size: 1rem;
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .form-grid {
        display: grid;
        grid-template-columns: 1fr 1fr;
        gap: 1rem;
    }

    .field {
        display: flex;
        flex-direction: column;
        gap: 0.25rem;
    }

    .field-wide {
        grid-column: 1 / -1;
    }

    .field-label {
        font-size: 0.75rem;
        font-weight: 500;
        color: var(--color-text-secondary);
    }

    input, select {
        padding: 0.5rem 0.625rem;
        background-color: var(--color-bg-elevated);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
        font-size: 0.8125rem;
    }

    input:focus, select:focus {
        outline: none;
        border-color: var(--color-accent);
    }

    .btn-primary {
        align-self: flex-start;
        padding: 0.625rem 1.25rem;
        background-color: var(--color-accent);
        color: var(--color-bg-deep);
        font-size: 0.8125rem;
        font-weight: 600;
        border-radius: var(--radius-sm);
        transition: background-color var(--transition-fast);
    }

    .btn-primary:hover:not(:disabled) {
        background-color: var(--color-accent-hover);
    }

    .btn-primary:disabled {
        opacity: 0.5;
    }

    .library-list {
        display: flex;
        flex-direction: column;
        gap: 0.75rem;
    }

    .library-item {
        background-color: var(--color-bg-surface);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-md);
        overflow: hidden;
    }

    .library-item-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 1rem;
        padding: 1rem 1.25rem;
    }

    .library-item-info {
        min-width: 0;
    }

    .library-name {
        font-size: 1rem;
        font-weight: 600;
        color: var(--color-text-primary);
        transition: color var(--transition-fast);
    }

    .library-name:hover {
        color: var(--color-accent);
    }

    .library-meta {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        margin-top: 0.25rem;
    }

    .badge {
        font-size: 0.625rem;
        font-weight: 600;
        text-transform: uppercase;
        color: var(--color-accent);
        background-color: var(--color-accent-muted);
        padding: 0.125rem 0.5rem;
        border-radius: var(--radius-sm);
    }

    .badge-inactive {
        font-size: 0.625rem;
        font-weight: 600;
        color: var(--color-text-muted);
        background-color: var(--color-info-bg);
        padding: 0.125rem 0.5rem;
        border-radius: var(--radius-sm);
    }

    .item-count {
        font-size: 0.75rem;
        color: var(--color-text-muted);
    }

    .library-actions {
        display: flex;
        gap: 0.375rem;
        flex-shrink: 0;
    }

    .btn-secondary-sm {
        padding: 0.375rem 0.75rem;
        font-size: 0.75rem;
        color: var(--color-text-secondary);
        background-color: var(--color-bg-elevated);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        transition: all var(--transition-fast);
    }

    .btn-secondary-sm:hover:not(:disabled) {
        border-color: var(--color-accent);
        color: var(--color-text-primary);
    }

    .btn-secondary-sm:disabled {
        opacity: 0.5;
    }

    .btn-danger-sm {
        padding: 0.375rem 0.75rem;
        font-size: 0.75rem;
        color: var(--color-error);
        background-color: transparent;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        transition: all var(--transition-fast);
    }

    .btn-danger-sm:hover {
        background-color: var(--color-error-bg);
        border-color: var(--color-error);
    }

    .paths-section {
        border-top: 1px solid var(--color-border-subtle);
        padding: 0.75rem 1.25rem;
        background-color: var(--color-bg-deep);
    }

    .path-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 0.375rem 0;
    }

    .path-text {
        font-family: monospace;
        font-size: 0.75rem;
        color: var(--color-text-secondary);
    }

    .path-flags {
        display: flex;
        gap: 0.375rem;
    }

    .flag-badge {
        font-size: 0.5625rem;
        font-weight: 600;
        text-transform: uppercase;
        padding: 0.0625rem 0.375rem;
        border-radius: 3px;
        color: var(--color-text-muted);
        background-color: var(--color-info-bg);
    }

    .flag-on {
        color: var(--color-success);
        background-color: var(--color-success-bg);
    }

    .flag-off {
        color: var(--color-text-muted);
    }

    .paths-loading {
        font-size: 0.75rem;
        color: var(--color-text-muted);
    }

    .empty-state {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: 1rem;
        padding: 3rem 1rem;
        text-align: center;
        color: var(--color-text-muted);
        font-size: 0.875rem;
    }

    .loading-state {
        display: flex;
        align-items: center;
        justify-content: center;
        padding: 4rem 0;
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
        .form-grid {
            grid-template-columns: 1fr;
        }

        .library-item-header {
            flex-direction: column;
            align-items: flex-start;
            gap: 0.75rem;
        }

        .library-actions {
            width: 100%;
            flex-wrap: wrap;
        }

        .page-header {
            flex-direction: column;
            gap: 0.75rem;
        }

        .page-header .btn-primary {
            align-self: flex-start;
        }

        .path-row {
            flex-direction: column;
            align-items: flex-start;
            gap: 0.375rem;
        }
    }
</style>
