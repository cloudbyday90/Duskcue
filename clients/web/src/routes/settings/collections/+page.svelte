<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { onMount } from 'svelte';
    import {
        listCollections,
        createCollection,
        updateCollection,
        deleteCollection,
        syncAllCollections,
        syncCollection,
        listTemplates,
        importTemplate,
    } from '$lib/api/collections.js';
    import { hasCapability } from '$lib/stores/auth.js';
    import { notifications } from '$lib/stores/notifications.js';
    import ConditionBuilder from '$lib/components/ConditionBuilder.svelte';

    let loading = $state(true);
    let canManage = $state(false);
    let collections = $state([]);
    let templates = $state([]);
    let view = $state('list');
    let editing = $state(null);
    let typeFilter = $state('');
    let enabledFilter = $state('');
    let libraryFilter = $state('');
    let saving = $state(false);
    let syncing = $state(false);
    let importing = $state(false);
    let templateJson = $state('');

    let form = $state(blankForm());
    let conditions = $state({ operator: 'and', rules: [] });

    const COLLECTION_TYPES = ['static', 'dynamic', 'smart'];
    const TYPE_LABELS = {
        static: 'Static',
        dynamic: 'Dynamic',
        smart: 'Smart',
    };
    const VISIBILITIES = ['visible', 'hidden', 'featured'];
    const SYNC_MODES = ['sync', 'append'];
    const SORT_OPTIONS = [
        'title.asc',
        'title.desc',
        'rating_average.desc',
        'rating_average.asc',
        'year.desc',
        'year.asc',
        'added.desc',
    ];

    const INTERNAL_BUILDERS = [
        'genre', 'country', 'decade', 'content_rating', 'actor', 'director',
        'studio', 'network', 'franchise', 'original_language', 'year',
        'resolution', 'audio_codec', 'streaming_service',
    ];
    const EXTERNAL_BUILDERS = [
        'tmdb_popular', 'tmdb_top_rated', 'tmdb_trending', 'tmdb_now_playing',
        'tmdb_upcoming', 'tmdb_collection', 'trakt_trending', 'trakt_popular',
        'trakt_recommended', 'trakt_user_lists', 'imdb_top_250', 'custom_url',
    ];
    const ALL_BUILDERS = [...INTERNAL_BUILDERS, ...EXTERNAL_BUILDERS];

    $effect(() => {
        const unsub = hasCapability('can_manage_libraries').subscribe((v) => (canManage = v));
        return unsub;
    });

    function blankForm() {
        return {
            name: '',
            description: '',
            library_id: '',
            collection_type: 'static',
            visibility: 'visible',
            is_enabled: true,
            sync_mode: 'sync',
            schedule: '0 6 * * *',
            sort_order: 0,
            sort_by: 'title.asc',
            builder_type: 'genre',
            limit: 100,
            minimum_items: 1,
            title_format: '',
            include: '',
            exclude: '',
        };
    }

    onMount(async () => {
        await refresh();
        loading = false;
    });

    async function refresh() {
        try {
            const [list, tpls] = await Promise.all([
                listCollections({ page: 1, page_size: 200 }),
                listTemplates(),
            ]);
            collections = list.items || [];
            templates = tpls || [];
        } catch (err) {
            notifications.error(err.detail || 'Failed to load collections');
        }
    }

    function startCreate() {
        editing = null;
        form = blankForm();
        conditions = { operator: 'and', rules: [] };
        view = 'editor';
    }

    function startEdit(c) {
        editing = c.id;
        const dynConfig = c.dynamic_config || {};
        form = {
            name: c.name,
            description: c.description || '',
            library_id: c.library_id || '',
            collection_type: c.collection_type,
            visibility: c.visibility,
            is_enabled: c.is_enabled,
            sync_mode: c.sync_mode,
            schedule: c.schedule,
            sort_order: c.sort_order,
            sort_by: c.sort_by,
            builder_type: dynConfig.builder_type || 'genre',
            limit: dynConfig.limit || 100,
            minimum_items: dynConfig.minimum_items || 1,
            title_format: dynConfig.title_format || '',
            include: Array.isArray(dynConfig.include) ? dynConfig.include.join(', ') : '',
            exclude: Array.isArray(dynConfig.exclude) ? dynConfig.exclude.join(', ') : '',
        };
        conditions = denormalizeConditions(c.smart_filter || {});
        view = 'editor';
    }

    function denormalizeConditions(node) {
        if (!node || typeof node !== 'object' || !node.operator) {
            return { operator: 'and', rules: [] };
        }
        return {
            operator: node.operator,
            rules: (node.rules || []).map(denormalizeRule),
        };
    }

    function denormalizeRule(rule) {
        if (rule.operator) {
            return {
                operator: rule.operator,
                rules: (rule.rules || []).map(denormalizeRule),
            };
        }
        const out = { field: rule.field, op: rule.op };
        if (rule.op === 'in') {
            out.value = Array.isArray(rule.values) ? rule.values.join(', ') : rule.value || '';
        } else {
            out.value = rule.value;
        }
        return out;
    }

    function normalizeConditions(node) {
        if (!node.rules || node.rules.length === 0) return {};
        return {
            operator: node.operator,
            rules: node.rules.map(normalizeRule),
        };
    }

    function normalizeRule(rule) {
        if (rule.operator) {
            return {
                operator: rule.operator,
                rules: (rule.rules || []).map(normalizeRule),
            };
        }
        if (rule.op === 'in') {
            const values = String(rule.value || '')
                .split(',')
                .map((s) => s.trim())
                .filter(Boolean);
            return { field: rule.field, op: 'in', values };
        }
        return { field: rule.field, op: rule.op, value: rule.value };
    }

    function buildRequest() {
        const trimmedLib = form.library_id.trim();
        const req = {
            name: form.name.trim(),
            description: form.description.trim() || null,
            library_id: trimmedLib || null,
            collection_type: form.collection_type,
            visibility: form.visibility,
            is_enabled: form.is_enabled,
            sync_mode: form.sync_mode,
            schedule: form.schedule,
            sort_order: numOr(form.sort_order, 0),
            sort_by: form.sort_by,
        };

        if (form.collection_type === 'dynamic') {
            const dynConfig = {
                builder_type: form.builder_type,
                limit: numOr(form.limit, 100),
                minimum_items: numOr(form.minimum_items, 1),
                sort_by: form.sort_by,
            };
            if (form.title_format.trim()) dynConfig.title_format = form.title_format.trim();
            const include = listFromCsv(form.include);
            if (include.length) dynConfig.include = include;
            const exclude = listFromCsv(form.exclude);
            if (exclude.length) dynConfig.exclude = exclude;
            req.dynamic_config = dynConfig;
        }

        if (form.collection_type === 'smart') {
            req.smart_filter = normalizeConditions(conditions);
        }

        Object.keys(req).forEach((k) => {
            if (req[k] === null && k !== 'library_id') delete req[k];
        });
        if (!editing) delete req.library_id;
        return req;
    }

    function numOr(v, fallback) {
        if (v === '' || v === null || v === undefined) return fallback;
        const n = Number(v);
        return Number.isFinite(n) ? n : fallback;
    }

    function listFromCsv(str) {
        return String(str || '')
            .split(',')
            .map((s) => s.trim())
            .filter(Boolean);
    }

    async function handleSave() {
        if (!form.name.trim()) {
            notifications.error('Name is required');
            return;
        }
        saving = true;
        try {
            const req = buildRequest();
            if (editing) {
                await updateCollection(editing, req);
                notifications.success('Collection updated');
            } else {
                await createCollection(req);
                notifications.success('Collection created');
            }
            await refresh();
            view = 'list';
        } catch (err) {
            notifications.error(err.detail || 'Save failed');
        } finally {
            saving = false;
        }
    }

    async function handleDelete(c) {
        if (c.is_system) {
            notifications.warning('System collections cannot be deleted — disable them instead');
            return;
        }
        if (!confirm(`Delete collection "${c.name}"?`)) return;
        try {
            await deleteCollection(c.id);
            notifications.success('Collection deleted');
            await refresh();
        } catch (err) {
            notifications.error(err.detail || 'Delete failed');
        }
    }

    async function handleToggle(c) {
        try {
            await updateCollection(c.id, { is_enabled: !c.is_enabled });
            c.is_enabled = !c.is_enabled;
        } catch (err) {
            notifications.error(err.detail || 'Toggle failed');
        }
    }

    async function handleSyncOne(c) {
        syncing = true;
        try {
            const result = await syncCollection(c.id, { include_external: true });
            notifications.success(`Synced ${result.queued_collections} collection(s)`);
            await refresh();
        } catch (err) {
            notifications.error(err.detail || 'Sync failed');
        } finally {
            syncing = false;
        }
    }

    async function handleSyncAll() {
        syncing = true;
        try {
            const result = await syncAllCollections({ include_external: true });
            notifications.success(`Synced ${result.queued_collections} collection(s)`);
            await refresh();
        } catch (err) {
            notifications.error(err.detail || 'Sync failed');
        } finally {
            syncing = false;
        }
    }

    async function handleImport() {
        let parsed;
        try {
            parsed = JSON.parse(templateJson);
        } catch {
            notifications.error('Invalid JSON');
            return;
        }
        if (!parsed.name || !parsed.template_type) {
            notifications.error('Template must have a name and template_type');
            return;
        }
        importing = true;
        try {
            await importTemplate(parsed);
            notifications.success('Template imported');
            templateJson = '';
            await refresh();
            view = 'list';
        } catch (err) {
            notifications.error(err.detail || 'Import failed');
        } finally {
            importing = false;
        }
    }

    function formatSyncResult(c) {
        if (!c.last_synced_at) return 'never';
        const r = c.last_sync_result || {};
        const parts = [];
        if (r.added != null) parts.push(`+${r.added}`);
        if (r.removed != null) parts.push(`-${r.removed}`);
        if (r.missing != null) parts.push(`${r.missing} missing`);
        return parts.length ? parts.join(', ') : 'no changes';
    }

    const filteredCollections = $derived(
        collections.filter((c) => {
            if (typeFilter && c.collection_type !== typeFilter) return false;
            if (enabledFilter === 'enabled' && !c.is_enabled) return false;
            if (enabledFilter === 'disabled' && c.is_enabled) return false;
            if (libraryFilter && String(c.library_id || '') !== libraryFilter) return false;
            return true;
        })
    );
</script>

<div class="collections-page">
    <div class="page-header">
        <div>
            <a href="/settings" class="back-link">← Settings</a>
            <h1 class="page-title">Collections</h1>
            <p class="page-subtitle">Static, dynamic, and smart media collections</p>
        </div>
        {#if canManage && view === 'list'}
            <div class="header-actions">
                <button class="btn-secondary" onclick={() => (view = 'templates')}>Templates</button>
                <button class="btn-secondary" onclick={handleSyncAll} disabled={syncing}>
                    {syncing ? 'Syncing…' : 'Sync All'}
                </button>
                <button class="btn-primary" onclick={startCreate}>New Collection</button>
            </div>
        {/if}
    </div>

    {#if loading}
        <div class="loading-state"><div class="loading-spinner"></div></div>
    {:else if view === 'editor'}
        <div class="editor-pane">
            <div class="editor-toolbar">
                <button class="btn-ghost" onclick={() => (view = 'list')}>← Back to list</button>
                <span class="editor-mode">{editing ? 'Edit collection' : 'New collection'}</span>
            </div>

            <div class="editor-form">
                <h3 class="form-section">Basics</h3>
                <div class="form-grid">
                    <label class="field field-wide">
                        <span class="field-label">Name</span>
                        <input type="text" bind:value={form.name} placeholder="Christmas Movies" />
                    </label>
                    <label class="field field-wide">
                        <span class="field-label">Description</span>
                        <input type="text" bind:value={form.description} placeholder="Optional description" />
                    </label>
                    <label class="field">
                        <span class="field-label">Type</span>
                        <select bind:value={form.collection_type}>
                            {#each COLLECTION_TYPES as t}<option value={t}>{TYPE_LABELS[t]}</option>{/each}
                        </select>
                    </label>
                    <label class="field">
                        <span class="field-label">Visibility</span>
                        <select bind:value={form.visibility}>
                            {#each VISIBILITIES as v}<option value={v}>{v}</option>{/each}
                        </select>
                    </label>
                    <label class="field">
                        <span class="field-label">Library (blank = all)</span>
                        <input type="text" bind:value={form.library_id} placeholder="UUID or blank" />
                    </label>
                    <label class="field-check">
                        <input type="checkbox" bind:checked={form.is_enabled} />
                        <span class="field-label">Enabled</span>
                    </label>
                </div>

                {#if form.collection_type === 'dynamic'}
                    <h3 class="form-section">Builder Configuration</h3>
                    <div class="form-grid">
                        <label class="field field-wide">
                            <span class="field-label">Builder</span>
                            <select bind:value={form.builder_type}>
                                <optgroup label="Internal (library metadata)">
                                    {#each INTERNAL_BUILDERS as b}<option value={b}>{b}</option>{/each}
                                </optgroup>
                                <optgroup label="External (API sources)">
                                    {#each EXTERNAL_BUILDERS as b}<option value={b}>{b}</option>{/each}
                                </optgroup>
                            </select>
                        </label>
                        <label class="field">
                            <span class="field-label">Max Items</span>
                            <input type="number" min="1" max="500" bind:value={form.limit} />
                        </label>
                        <label class="field">
                            <span class="field-label">Minimum Items</span>
                            <input type="number" min="1" max="500" bind:value={form.minimum_items} />
                        </label>
                        <label class="field">
                            <span class="field-label">Sync Mode</span>
                            <select bind:value={form.sync_mode}>
                                {#each SYNC_MODES as m}<option value={m}>{m}</option>{/each}
                            </select>
                        </label>
                        <label class="field">
                            <span class="field-label">Schedule (cron)</span>
                            <input type="text" bind:value={form.schedule} placeholder="0 6 * * *" />
                        </label>
                        <label class="field field-wide">
                            <span class="field-label">Title Format (optional)</span>
                            <input type="text" bind:value={form.title_format} placeholder="Top &lt;&lt;key_name&gt;&gt; <<library_type>>s" />
                        </label>
                        <label class="field">
                            <span class="field-label">Include (comma-separated keys)</span>
                            <input type="text" bind:value={form.include} placeholder="Action, Comedy" />
                        </label>
                        <label class="field">
                            <span class="field-label">Exclude (comma-separated keys)</span>
                            <input type="text" bind:value={form.exclude} placeholder="Talk Show" />
                        </label>
                    </div>
                    <p class="field-hint">
                        Template variables: <code>&lt;&lt;key_name&gt;&gt;</code>, <code>&lt;&lt;library_type&gt;&gt;</code>, <code>&lt;&lt;limit&gt;&gt;</code>.
                        External builders require the provider to be configured in system settings.
                    </p>
                {/if}

                {#if form.collection_type === 'smart'}
                    <h3 class="form-section">Smart Filter</h3>
                    <p class="field-hint">Items matching these rules are included at query time (no stored items).</p>
                    <ConditionBuilder node={conditions} onchange={(n) => (conditions = n)} />
                    <details class="raw-json">
                        <summary>Raw JSON</summary>
                        <pre>{JSON.stringify(normalizeConditions(conditions), null, 2)}</pre>
                    </details>
                {/if}

                <h3 class="form-section">Display</h3>
                <div class="form-grid">
                    <label class="field">
                        <span class="field-label">Sort Order</span>
                        <input type="number" bind:value={form.sort_order} />
                    </label>
                    <label class="field">
                        <span class="field-label">Sort Items By</span>
                        <select bind:value={form.sort_by}>
                            {#each SORT_OPTIONS as s}<option value={s}>{s}</option>{/each}
                        </select>
                    </label>
                </div>

                <div class="form-actions">
                    <button class="btn-primary" onclick={handleSave} disabled={saving}>
                        {saving ? 'Saving…' : editing ? 'Save Changes' : 'Create Collection'}
                    </button>
                    <button class="btn-ghost" onclick={() => (view = 'list')}>Cancel</button>
                </div>
            </div>
        </div>
    {:else if view === 'templates'}
        <div class="templates-pane">
            <div class="editor-toolbar">
                <button class="btn-ghost" onclick={() => (view = 'list')}>← Back to list</button>
                <span class="editor-mode">Templates</span>
            </div>

            <section class="card">
                <h3 class="form-section">Installed Templates</h3>
                {#if templates.length === 0}
                    <p class="empty-inline">No imported templates yet.</p>
                {:else}
                    <div class="template-list">
                        {#each templates as t}
                            <div class="template-row">
                                <span class="template-name">{t.name}</span>
                                <span class="badge">{t.template_type}</span>
                                {#if t.author}<span class="item-count">by {t.author}</span>{/if}
                                {#if t.is_system}<span class="system-badge">system</span>{/if}
                            </div>
                        {/each}
                    </div>
                {/if}
            </section>

            <section class="card">
                <h3 class="form-section">Import Template</h3>
                <p class="field-hint">Paste a template JSON to save a reusable collection definition.</p>
                <textarea
                    class="json-input"
                    bind:value={templateJson}
                    placeholder={'{\n  "name": "Award Winners",\n  "template_type": "multi",\n  "template_json": {\n    "builder_type": "custom_url"\n  }\n}'}
                    rows="10"
                ></textarea>
                <button class="btn-primary" onclick={handleImport} disabled={importing}>
                    {importing ? 'Importing…' : 'Import Template'}
                </button>
            </section>
        </div>
    {:else}
        <div class="filters">
            <label class="field">
                <span class="field-label">Type</span>
                <select bind:value={typeFilter}>
                    <option value="">All</option>
                    {#each COLLECTION_TYPES as t}<option value={t}>{TYPE_LABELS[t]}</option>{/each}
                </select>
            </label>
            <label class="field">
                <span class="field-label">State</span>
                <select bind:value={enabledFilter}>
                    <option value="">All</option>
                    <option value="enabled">Enabled</option>
                    <option value="disabled">Disabled</option>
                </select>
            </label>
            <label class="field">
                <span class="field-label">Library</span>
                <input type="text" bind:value={libraryFilter} placeholder="UUID or blank" />
            </label>
        </div>

        {#if filteredCollections.length === 0}
            <div class="empty-state">
                <p>No collections configured.</p>
                {#if canManage}
                    <button class="btn-primary" onclick={startCreate}>Create your first collection</button>
                {/if}
            </div>
        {:else}
            {#each COLLECTION_TYPES as type}
                {@const group = filteredCollections.filter((c) => c.collection_type === type)}
                {#if group.length > 0}
                    <section class="card">
                        <h3 class="form-section">{TYPE_LABELS[type]} <span class="count-pill">{group.length}</span></h3>
                        <div class="collection-list">
                            {#each group as c (c.id)}
                                <div class="collection-row" class:disabled={!c.is_enabled}>
                                    <div class="collection-info">
                                        <span class="collection-name">{c.name}</span>
                                        <div class="collection-meta">
                                            {#if c.is_dynamic}
                                                {@const cfg = c.dynamic_config || {}}
                                                <span class="type-badge">{cfg.builder_type || 'dynamic'}</span>
                                                <span class="meta-tag">{c.item_count} items</span>
                                                <span class="meta-tag">last sync: {formatSyncResult(c)}</span>
                                            {:else if c.is_smart}
                                                <span class="type-badge type-smart">smart filter</span>
                                            {:else}
                                                <span class="type-badge type-static">{c.item_count} items</span>
                                            {/if}
                                            <span class="meta-tag">{c.visibility}</span>
                                            {#if c.is_system}<span class="system-badge">system</span>{/if}
                                        </div>
                                    </div>
                                    {#if canManage}
                                        <div class="collection-actions">
                                            <label class="toggle">
                                                <input type="checkbox" checked={c.is_enabled} onchange={() => handleToggle(c)} />
                                                <span>{c.is_enabled ? 'on' : 'off'}</span>
                                            </label>
                                            {#if c.is_dynamic}
                                                <button class="btn-secondary-sm" onclick={() => handleSyncOne(c)} disabled={syncing}>Sync</button>
                                            {/if}
                                            <button class="btn-secondary-sm" onclick={() => startEdit(c)}>Edit</button>
                                            <button class="btn-danger-sm" onclick={() => handleDelete(c)} disabled={c.is_system}>Delete</button>
                                        </div>
                                    {/if}
                                </div>
                            {/each}
                        </div>
                    </section>
                {/if}
            {/each}
        {/if}
    {/if}
</div>

<style>
    .collections-page {
        display: flex;
        flex-direction: column;
        gap: 1.5rem;
        max-width: 1000px;
    }

    .page-header {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 1rem;
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

    .page-subtitle {
        font-size: 0.8125rem;
        color: var(--color-text-muted);
        margin-top: 0.125rem;
    }

    .header-actions {
        display: flex;
        gap: 0.5rem;
        flex-shrink: 0;
    }

    .filters {
        display: flex;
        gap: 1rem;
        align-items: flex-end;
        flex-wrap: wrap;
    }

    .card {
        background-color: var(--color-bg-surface);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-md);
        padding: 1.25rem;
        display: flex;
        flex-direction: column;
        gap: 0.75rem;
    }

    .form-section {
        font-size: 0.75rem;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.05em;
        color: var(--color-text-secondary);
        display: flex;
        align-items: center;
        gap: 0.5rem;
    }

    .count-pill {
        font-size: 0.625rem;
        color: var(--color-text-muted);
        background-color: var(--color-info-bg);
        padding: 0.0625rem 0.4rem;
        border-radius: 999px;
    }

    .form-grid {
        display: grid;
        grid-template-columns: 1fr 1fr;
        gap: 0.75rem;
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
        font-size: 0.6875rem;
        font-weight: 500;
        color: var(--color-text-muted);
    }

    .field-hint {
        font-size: 0.6875rem;
        color: var(--color-text-muted);
    }

    .field-hint code {
        font-family: monospace;
        background-color: var(--color-bg-elevated);
        padding: 0.0625rem 0.25rem;
        border-radius: 3px;
    }

    .field-check {
        display: flex;
        align-items: center;
        gap: 0.375rem;
    }

    input, select, textarea {
        padding: 0.4rem 0.5rem;
        background-color: var(--color-bg-elevated);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
        font-size: 0.8125rem;
        font-family: inherit;
    }

    input:focus, select:focus, textarea:focus {
        outline: none;
        border-color: var(--color-accent);
    }

    textarea {
        resize: vertical;
        font-family: monospace;
    }

    .editor-form {
        background-color: var(--color-bg-surface);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-md);
        padding: 1.25rem;
        display: flex;
        flex-direction: column;
        gap: 1rem;
    }

    .raw-json {
        font-size: 0.6875rem;
    }

    .raw-json summary {
        cursor: pointer;
        color: var(--color-text-muted);
    }

    .raw-json pre {
        margin-top: 0.5rem;
        padding: 0.5rem;
        background-color: var(--color-bg-deep);
        border-radius: var(--radius-sm);
        overflow-x: auto;
        color: var(--color-text-secondary);
    }

    .form-actions, .editor-toolbar {
        display: flex;
        align-items: center;
        gap: 0.75rem;
    }

    .editor-mode {
        font-size: 0.8125rem;
        color: var(--color-text-secondary);
        font-weight: 600;
    }

    .btn-primary {
        padding: 0.5rem 1rem;
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

    .btn-secondary, .btn-secondary-sm {
        padding: 0.5rem 0.875rem;
        font-size: 0.8125rem;
        color: var(--color-text-secondary);
        background-color: var(--color-bg-elevated);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        transition: all var(--transition-fast);
    }

    .btn-secondary:hover:not(:disabled), .btn-secondary-sm:hover:not(:disabled) {
        border-color: var(--color-accent);
        color: var(--color-text-primary);
    }

    .btn-secondary:disabled, .btn-secondary-sm:disabled {
        opacity: 0.5;
    }

    .btn-secondary-sm {
        padding: 0.3rem 0.625rem;
        font-size: 0.75rem;
    }

    .btn-danger-sm {
        padding: 0.3rem 0.625rem;
        font-size: 0.75rem;
        color: var(--color-error);
        background-color: transparent;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
    }

    .btn-danger-sm:hover:not(:disabled) {
        background-color: var(--color-error-bg);
        border-color: var(--color-error);
    }

    .btn-danger-sm:disabled {
        opacity: 0.4;
        cursor: default;
    }

    .btn-ghost {
        padding: 0.375rem 0.625rem;
        font-size: 0.8125rem;
        color: var(--color-text-muted);
    }

    .btn-ghost:hover {
        color: var(--color-text-primary);
    }

    .collection-list {
        display: flex;
        flex-direction: column;
        gap: 0.5rem;
    }

    .collection-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 1rem;
        padding: 0.75rem 0.875rem;
        background-color: var(--color-bg-elevated);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
    }

    .collection-row.disabled {
        opacity: 0.55;
    }

    .collection-name {
        font-size: 0.875rem;
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .collection-meta {
        display: flex;
        align-items: center;
        gap: 0.375rem;
        margin-top: 0.2rem;
        flex-wrap: wrap;
    }

    .type-badge {
        font-size: 0.5625rem;
        font-weight: 600;
        text-transform: uppercase;
        padding: 0.0625rem 0.375rem;
        border-radius: 3px;
        color: var(--color-accent);
        background-color: var(--color-accent-muted);
    }

    .type-static { color: #6abf69; background-color: var(--color-success-bg); }
    .type-smart { color: var(--color-warning); background-color: var(--color-warning-bg); }

    .meta-tag {
        font-size: 0.625rem;
        color: var(--color-text-muted);
        background-color: var(--color-info-bg);
        padding: 0.0625rem 0.375rem;
        border-radius: 3px;
    }

    .system-badge {
        font-size: 0.5625rem;
        font-weight: 600;
        text-transform: uppercase;
        color: var(--color-text-primary);
        background-color: var(--color-bg-hover);
        padding: 0.0625rem 0.375rem;
        border-radius: 3px;
    }

    .collection-actions {
        display: flex;
        align-items: center;
        gap: 0.375rem;
        flex-shrink: 0;
    }

    .toggle {
        display: flex;
        align-items: center;
        gap: 0.25rem;
        font-size: 0.6875rem;
        color: var(--color-text-muted);
    }

    .template-list {
        display: flex;
        flex-direction: column;
        gap: 0.375rem;
    }

    .template-row {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        padding: 0.5rem 0.625rem;
        background-color: var(--color-bg-elevated);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
    }

    .template-name {
        font-size: 0.8125rem;
        font-weight: 600;
        color: var(--color-text-primary);
        flex: 1;
    }

    .badge {
        font-size: 0.5625rem;
        font-weight: 600;
        color: var(--color-accent);
        background-color: var(--color-accent-muted);
        padding: 0.0625rem 0.375rem;
        border-radius: 3px;
    }

    .item-count {
        font-size: 0.6875rem;
        color: var(--color-text-muted);
    }

    .empty-inline {
        font-size: 0.8125rem;
        color: var(--color-text-muted);
    }

    .json-input {
        font-family: monospace;
        font-size: 0.75rem;
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

        .page-header, .collection-row {
            flex-direction: column;
            align-items: flex-start;
        }

        .header-actions {
            width: 100%;
            flex-wrap: wrap;
        }
    }
</style>
