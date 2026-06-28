<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { onMount } from 'svelte';
    import {
        listOverlays,
        createOverlay,
        updateOverlay,
        deleteOverlay,
        applyOverlays,
        previewOverlay,
        listTemplates,
        importTemplate,
    } from '$lib/api/overlays.js';
    import { hasCapability } from '$lib/stores/auth.js';
    import { notifications } from '$lib/stores/notifications.js';
    import ConditionBuilder from '$lib/components/ConditionBuilder.svelte';

    let loading = $state(true);
    let canManage = $state(false);
    let overlays = $state([]);
    let templates = $state([]);
    let view = $state('list');
    let editing = $state(null);
    let libraryFilter = $state('');
    let enabledFilter = $state('');
    let saving = $state(false);
    let applying = $state(false);
    let importing = $state(false);
    let templateJson = $state('');
    let previewUrl = $state(null);
    let previewing = $state(false);
    let previewMediaId = $state('');
    let textTemplateEl = $state(null);

    let form = $state(blankForm());
    let conditions = $state({ operator: 'and', rules: [] });

    const APPLIES_TO = ['poster', 'backdrop', 'season_poster', 'episode_thumb'];
    const APPLIES_LABELS = {
        poster: 'Poster',
        backdrop: 'Backdrop',
        season_poster: 'Season Poster',
        episode_thumb: 'Episode Thumb',
    };
    const OVERLAY_TYPES = ['image', 'text', 'backdrop'];
    const ALIGNS_H = ['left', 'center', 'right'];
    const ALIGNS_V = ['top', 'center', 'bottom'];
    const TEXT_VARIABLES = [
        '<<title>>', '<<year>>', '<<resolution>>', '<<video_codec>>',
        '<<audio_codec>>', '<<critic_rating>>', '<<critic_rating/>>',
        '<<audience_rating>>', '<<rating_vote_count>>', '<<audio_channels>>',
        '<<content_rating>>', '<<runtime>>', '<<runtimeH>>', '<<runtimeM>>',
        '<<edition>>', '<<video_dynamic_range>>', '<<container>>',
    ];

    $effect(() => {
        const unsub = hasCapability('can_manage_libraries').subscribe((v) => (canManage = v));
        return unsub;
    });

    function blankForm() {
        return {
            name: '',
            overlay_type: 'text',
            library_id: '',
            image_path: '',
            text_template: '',
            font_family: 'Inter',
            font_size: 63,
            font_color: '#FFFFFF',
            stroke_color: '',
            stroke_width: 0,
            back_color: '#00000099',
            back_width: '',
            back_height: '',
            back_radius: 0,
            back_padding: 0,
            horizontal_align: 'left',
            horizontal_offset: 0,
            vertical_align: 'top',
            vertical_offset: 0,
            scale_width: '',
            scale_height: '',
            group_name: '',
            weight: 0,
            queue_name: '',
            suppresses: '',
            applies_to: 'poster',
            is_enabled: true,
        };
    }

    onMount(async () => {
        await refresh();
        loading = false;
    });

    async function refresh() {
        try {
            const [list, tpls] = await Promise.all([
                listOverlays({ page: 1, page_size: 200 }),
                listTemplates(),
            ]);
            overlays = list.items || [];
            templates = tpls || [];
        } catch (err) {
            notifications.error(err.detail || 'Failed to load overlays');
        }
    }

    function overlaysByType(type) {
        return overlays.filter((o) => o.applies_to === type);
    }

    function startCreate() {
        editing = null;
        form = blankForm();
        conditions = { operator: 'and', rules: [] };
        previewUrl = null;
        view = 'editor';
    }

    function startEdit(o) {
        editing = o.id;
        form = {
            name: o.name,
            overlay_type: o.overlay_type,
            library_id: o.library_id || '',
            image_path: o.image_path || '',
            text_template: o.text_template || '',
            font_family: o.font_family || 'Inter',
            font_size: o.font_size || 63,
            font_color: o.font_color || '#FFFFFF',
            stroke_color: o.stroke_color || '',
            stroke_width: o.stroke_width ?? 0,
            back_color: o.back_color || '#00000099',
            back_width: o.back_width ?? '',
            back_height: o.back_height ?? '',
            back_radius: o.back_radius ?? 0,
            back_padding: o.back_padding ?? 0,
            horizontal_align: o.horizontal_align || 'left',
            horizontal_offset: o.horizontal_offset ?? 0,
            vertical_align: o.vertical_align || 'top',
            vertical_offset: o.vertical_offset ?? 0,
            scale_width: o.scale_width ?? '',
            scale_height: o.scale_height ?? '',
            group_name: o.group_name || '',
            weight: o.weight ?? 0,
            queue_name: o.queue_name || '',
            suppresses: (o.suppresses || []).join(', '),
            applies_to: o.applies_to || 'poster',
            is_enabled: o.is_enabled,
        };
        conditions = denormalizeConditions(o.conditions || {});
        previewUrl = null;
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
            overlay_type: form.overlay_type,
            library_id: trimmedLib || null,
            image_path: form.image_path.trim() || null,
            text_template: form.text_template.trim() || null,
            font_family: form.font_family.trim() || null,
            font_size: numOr(form.font_size, 63),
            font_color: form.font_color.trim() || null,
            stroke_color: form.stroke_color.trim() || null,
            stroke_width: numOr(form.stroke_width, 0),
            back_color: form.back_color.trim() || null,
            back_width: numOr(form.back_width, null),
            back_height: numOr(form.back_height, null),
            back_radius: numOr(form.back_radius, 0),
            back_padding: numOr(form.back_padding, 0),
            horizontal_align: form.horizontal_align,
            horizontal_offset: numOr(form.horizontal_offset, 0),
            vertical_align: form.vertical_align,
            vertical_offset: numOr(form.vertical_offset, 0),
            scale_width: numOr(form.scale_width, null),
            scale_height: numOr(form.scale_height, null),
            group_name: form.group_name.trim() || null,
            weight: numOr(form.weight, 0),
            queue_name: form.queue_name.trim() || null,
            suppresses: form.suppresses
                .split(',')
                .map((s) => s.trim())
                .filter(Boolean),
            applies_to: form.applies_to,
            is_enabled: form.is_enabled,
            conditions: normalizeConditions(conditions),
        };
        Object.keys(req).forEach((k) => {
            if (k === 'library_id') return;
            if (req[k] === null) delete req[k];
        });
        if (!editing) delete req.library_id;
        return req;
    }

    function numOr(v, fallback) {
        if (v === '' || v === null || v === undefined) return fallback;
        const n = Number(v);
        return Number.isFinite(n) ? n : fallback;
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
                await updateOverlay(editing, req);
                notifications.success('Overlay updated');
            } else {
                await createOverlay(req);
                notifications.success('Overlay created');
            }
            await refresh();
            view = 'list';
        } catch (err) {
            notifications.error(err.detail || 'Save failed');
        } finally {
            saving = false;
        }
    }

    async function handleDelete(o) {
        if (o.is_system) {
            notifications.warning('System overlays cannot be deleted — disable them instead');
            return;
        }
        if (!confirm(`Delete overlay "${o.name}"?`)) return;
        try {
            await deleteOverlay(o.id);
            notifications.success('Overlay deleted');
            await refresh();
        } catch (err) {
            notifications.error(err.detail || 'Delete failed');
        }
    }

    async function handleToggle(o) {
        try {
            await updateOverlay(o.id, { is_enabled: !o.is_enabled });
            o.is_enabled = !o.is_enabled;
        } catch (err) {
            notifications.error(err.detail || 'Toggle failed');
        }
    }

    async function handleApply(reapplyAll = false) {
        applying = true;
        try {
            const result = await applyOverlays({
                library_id: libraryFilter.trim() || null,
                reapply_all: reapplyAll,
            });
            notifications.success(`Overlay application queued for ${result.queued_items || 0} items`);
            await refresh();
        } catch (err) {
            notifications.error(err.detail || 'Apply failed');
        } finally {
            applying = false;
        }
    }

    async function handlePreview() {
        if (!previewMediaId.trim()) {
            notifications.warning('Enter a media item ID to preview against');
            return;
        }
        previewing = true;
        previewUrl = null;
        try {
            const req = {
                media_item_id: previewMediaId.trim(),
                artwork_type: form.applies_to,
            };
            if (editing) req.overlay_ids = [editing];
            const result = await previewOverlay(req);
            previewUrl = result.preview_url + '?t=' + Date.now();
        } catch (err) {
            notifications.error(err.detail || 'Preview failed');
        } finally {
            previewing = false;
        }
    }

    function insertVariable(v) {
        const el = textTemplateEl;
        if (!el) {
            form.text_template = (form.text_template || '') + v;
            return;
        }
        const start = el.selectionStart || 0;
        const end = el.selectionEnd || 0;
        const current = form.text_template || '';
        form.text_template = current.slice(0, start) + v + current.slice(end);
        requestAnimationFrame(() => {
            el.focus();
            const pos = start + v.length;
            el.setSelectionRange(pos, pos);
        });
    }

    async function handleImport() {
        let parsed;
        try {
            parsed = JSON.parse(templateJson);
        } catch {
            notifications.error('Invalid JSON');
            return;
        }
        if (!parsed.name || !Array.isArray(parsed.overlays) || parsed.overlays.length === 0) {
            notifications.error('Template must have a name and an overlays array');
            return;
        }
        importing = true;
        try {
            const result = await importTemplate(parsed);
            notifications.success(`Imported ${result.imported_count} overlay(s)`);
            templateJson = '';
            await refresh();
            view = 'list';
        } catch (err) {
            notifications.error(err.detail || 'Import failed');
        } finally {
            importing = false;
        }
    }

    const filteredOverlays = $derived(
        overlays.filter((o) => {
            if (enabledFilter === 'enabled' && !o.is_enabled) return false;
            if (enabledFilter === 'disabled' && o.is_enabled) return false;
            if (libraryFilter && String(o.library_id || '') !== libraryFilter) return false;
            return true;
        })
    );
</script>

<div class="overlays-page">
    <div class="page-header">
        <div>
            <a href="/settings" class="back-link">← Settings</a>
            <h1 class="page-title">Overlays</h1>
            <p class="page-subtitle">Artwork overlay compositing engine and poster management</p>
        </div>
        {#if canManage && view === 'list'}
            <div class="header-actions">
                <button class="btn-secondary" onclick={() => (view = 'templates')}>Templates</button>
                <button class="btn-secondary" onclick={() => handleApply(false)} disabled={applying}>
                    {applying ? 'Applying…' : 'Apply Now'}
                </button>
                <button class="btn-primary" onclick={startCreate}>New Overlay</button>
            </div>
        {/if}
    </div>

    {#if loading}
        <div class="loading-state"><div class="loading-spinner"></div></div>
    {:else if view === 'editor'}
        <div class="editor-pane">
            <div class="editor-toolbar">
                <button class="btn-ghost" onclick={() => (view = 'list')}>← Back to list</button>
                <span class="editor-mode">{editing ? 'Edit overlay' : 'New overlay'}</span>
            </div>

            <div class="editor-grid">
                <div class="editor-form">
                    <h3 class="form-section">Basics</h3>
                    <div class="form-grid">
                        <label class="field field-wide">
                            <span class="field-label">Name</span>
                            <input type="text" bind:value={form.name} placeholder="Resolution Badge" />
                        </label>
                        <label class="field">
                            <span class="field-label">Type</span>
                            <select bind:value={form.overlay_type}>
                                {#each OVERLAY_TYPES as t}<option value={t}>{t}</option>{/each}
                            </select>
                        </label>
                        <label class="field">
                            <span class="field-label">Applies To</span>
                            <select bind:value={form.applies_to}>
                                {#each APPLIES_TO as a}<option value={a}>{APPLIES_LABELS[a]}</option>{/each}
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

                    {#if form.overlay_type === 'image'}
                        <h3 class="form-section">Image</h3>
                        <div class="form-grid">
                            <label class="field field-wide">
                                <span class="field-label">Image Path</span>
                                <input type="text" bind:value={form.image_path} placeholder="/data/overlays/4k.png" />
                            </label>
                            <label class="field">
                                <span class="field-label">Scale Width</span>
                                <input type="number" bind:value={form.scale_width} placeholder="auto" />
                            </label>
                            <label class="field">
                                <span class="field-label">Scale Height</span>
                                <input type="number" bind:value={form.scale_height} placeholder="auto" />
                            </label>
                        </div>
                    {:else if form.overlay_type === 'text'}
                        <h3 class="form-section">Text</h3>
                        <div class="form-grid">
                            <label class="field field-wide">
                                <span class="field-label">Template</span>
                                <textarea
                                    bind:this={textTemplateEl}
                                    bind:value={form.text_template}
                                    placeholder="<<critic_rating>>/10"
                                    rows="2"
                                ></textarea>
                            </label>
                            <div class="var-inserter field-wide">
                                <span class="field-label">Insert variable:</span>
                                <select onchange={(e) => { if (e.currentTarget.value) insertVariable(e.currentTarget.value); e.currentTarget.value=''; }}>
                                    <option value="">Choose…</option>
                                    {#each TEXT_VARIABLES as v}<option value={v}>{v}</option>{/each}
                                </select>
                            </div>
                            <label class="field">
                                <span class="field-label">Font Family</span>
                                <input type="text" bind:value={form.font_family} placeholder="Inter" />
                            </label>
                            <label class="field">
                                <span class="field-label">Font Size</span>
                                <input type="number" min="1" max="500" bind:value={form.font_size} />
                            </label>
                            <label class="field">
                                <span class="field-label">Font Color</span>
                                <div class="color-field">
                                    <input type="color" value={form.font_color.slice(0, 7)} oninput={(e) => (form.font_color = e.currentTarget.value)} />
                                    <input type="text" class="color-text" bind:value={form.font_color} />
                                </div>
                            </label>
                            <label class="field">
                                <span class="field-label">Stroke Color</span>
                                <div class="color-field">
                                    <input type="color" value={(form.stroke_color || '#000000').slice(0, 7)} oninput={(e) => (form.stroke_color = e.currentTarget.value)} />
                                    <input type="text" class="color-text" bind:value={form.stroke_color} placeholder="none" />
                                </div>
                            </label>
                            <label class="field">
                                <span class="field-label">Stroke Width</span>
                                <input type="number" min="0" max="50" bind:value={form.stroke_width} />
                            </label>
                        </div>
                    {/if}

                    {#if form.overlay_type !== 'image'}
                        <h3 class="form-section">Backdrop (optional)</h3>
                        <div class="form-grid">
                            <label class="field">
                                <span class="field-label">Back Color</span>
                                <div class="color-field">
                                    <input type="color" value={(form.back_color || '#000000').slice(0, 7)} oninput={(e) => (form.back_color = e.currentTarget.value)} />
                                    <input type="text" class="color-text" bind:value={form.back_color} placeholder="#00000099" />
                                </div>
                            </label>
                            <label class="field">
                                <span class="field-label">Back Width</span>
                                <input type="number" bind:value={form.back_width} placeholder="auto" />
                            </label>
                            <label class="field">
                                <span class="field-label">Back Height</span>
                                <input type="number" bind:value={form.back_height} placeholder="auto" />
                            </label>
                            <label class="field">
                                <span class="field-label">Corner Radius</span>
                                <input type="number" min="0" bind:value={form.back_radius} />
                            </label>
                            <label class="field">
                                <span class="field-label">Padding</span>
                                <input type="number" min="0" bind:value={form.back_padding} />
                            </label>
                        </div>
                    {/if}

                    <h3 class="form-section">Positioning</h3>
                    <div class="form-grid">
                        <label class="field">
                            <span class="field-label">Horizontal Align</span>
                            <select bind:value={form.horizontal_align}>
                                {#each ALIGNS_H as a}<option value={a}>{a}</option>{/each}
                            </select>
                        </label>
                        <label class="field">
                            <span class="field-label">Horizontal Offset</span>
                            <input type="number" min="0" max="1500" bind:value={form.horizontal_offset} />
                        </label>
                        <label class="field">
                            <span class="field-label">Vertical Align</span>
                            <select bind:value={form.vertical_align}>
                                {#each ALIGNS_V as a}<option value={a}>{a}</option>{/each}
                            </select>
                        </label>
                        <label class="field">
                            <span class="field-label">Vertical Offset</span>
                            <input type="number" min="0" max="1500" bind:value={form.vertical_offset} />
                        </label>
                    </div>

                    <h3 class="form-section">Group, Queue & Suppression</h3>
                    <div class="form-grid">
                        <label class="field">
                            <span class="field-label">Group</span>
                            <input type="text" bind:value={form.group_name} placeholder="resolution" />
                        </label>
                        <label class="field">
                            <span class="field-label">Weight</span>
                            <input type="number" min="0" bind:value={form.weight} />
                        </label>
                        <label class="field">
                            <span class="field-label">Queue</span>
                            <input type="text" bind:value={form.queue_name} placeholder="bottom_right" />
                        </label>
                        <label class="field field-wide">
                            <span class="field-label">Suppresses (comma-separated slugs)</span>
                            <input type="text" bind:value={form.suppresses} placeholder="4k_badge, hdr_badge" />
                        </label>
                    </div>

                    <h3 class="form-section">Conditions</h3>
                    <p class="field-hint">When this overlay applies. Empty = all items.</p>
                    <ConditionBuilder node={conditions} onchange={(n) => (conditions = n)} />
                    <details class="raw-json">
                        <summary>Raw JSON</summary>
                        <pre>{JSON.stringify(normalizeConditions(conditions), null, 2)}</pre>
                    </details>

                    <div class="form-actions">
                        <button class="btn-primary" onclick={handleSave} disabled={saving}>
                            {saving ? 'Saving…' : editing ? 'Save Changes' : 'Create Overlay'}
                        </button>
                        <button class="btn-ghost" onclick={() => (view = 'list')}>Cancel</button>
                    </div>
                </div>

                <div class="preview-pane">
                    <h3 class="form-section">Live Preview</h3>
                    <label class="field">
                        <span class="field-label">Media Item ID</span>
                        <input type="text" bind:value={previewMediaId} placeholder="paste a media item UUID" />
                    </label>
                    <button class="btn-secondary" onclick={handlePreview} disabled={previewing}>
                        {previewing ? 'Rendering…' : 'Render Preview'}
                    </button>
                    {#if previewUrl}
                        <div class="preview-image-wrap">
                            <img src={previewUrl} alt="Overlay preview" />
                        </div>
                    {:else}
                        <div class="preview-empty">Enter a media item ID and render to see the composited result.</div>
                    {/if}
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
                                <span class="badge">v{t.version}</span>
                                <span class="item-count">{t.overlay_count} overlays</span>
                            </div>
                        {/each}
                    </div>
                {/if}
            </section>

            <section class="card">
                <h3 class="form-section">Import Community Template</h3>
                <p class="field-hint">Paste a template JSON ({'{"name":"…","overlays":[…]}'}) to import a set of overlay definitions.</p>
                <textarea
                    class="json-input"
                    bind:value={templateJson}
                    placeholder={'{\n  "name": "My Rating Badges",\n  "version": 1,\n  "overlays": [\n    { "name": "IMDb Rating", "overlay_type": "text", "text_template": "<<critic_rating>>" }\n  ]\n}'}
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
                <span class="field-label">Library</span>
                <input type="text" bind:value={libraryFilter} placeholder="UUID or blank" />
            </label>
            <label class="field">
                <span class="field-label">State</span>
                <select bind:value={enabledFilter}>
                    <option value="">All</option>
                    <option value="enabled">Enabled</option>
                    <option value="disabled">Disabled</option>
                </select>
            </label>
            {#if canManage}
                <button class="btn-secondary" onclick={() => handleApply(true)} disabled={applying}>
                    Re-apply All
                </button>
            {/if}
        </div>

        {#if filteredOverlays.length === 0}
            <div class="empty-state">
                <p>No overlay definitions configured.</p>
                {#if canManage}
                    <button class="btn-primary" onclick={startCreate}>Create your first overlay</button>
                {/if}
            </div>
        {:else}
            {#each APPLIES_TO as type}
                {@const group = filteredOverlays.filter((o) => o.applies_to === type)}
                {#if group.length > 0}
                    <section class="card">
                        <h3 class="form-section">{APPLIES_LABELS[type]} <span class="count-pill">{group.length}</span></h3>
                        <div class="overlay-list">
                            {#each group as o (o.id)}
                                <div class="overlay-row" class:disabled={!o.is_enabled}>
                                    <div class="overlay-info">
                                        <span class="overlay-name">{o.name}</span>
                                        <div class="overlay-meta">
                                            <span class="type-badge type-{o.overlay_type}">{o.overlay_type}</span>
                                            {#if o.group_name}<span class="meta-tag">group: {o.group_name}</span>{/if}
                                            {#if o.queue_name}<span class="meta-tag">queue: {o.queue_name}</span>{/if}
                                            <span class="meta-tag">weight: {o.weight}</span>
                                            {#if o.is_system}<span class="system-badge">system</span>{/if}
                                        </div>
                                    </div>
                                    {#if canManage}
                                        <div class="overlay-actions">
                                            <label class="toggle">
                                                <input type="checkbox" checked={o.is_enabled} onchange={() => handleToggle(o)} />
                                                <span>{o.is_enabled ? 'on' : 'off'}</span>
                                            </label>
                                            <button class="btn-secondary-sm" onclick={() => startEdit(o)}>Edit</button>
                                            <button class="btn-danger-sm" onclick={() => handleDelete(o)} disabled={o.is_system}>Delete</button>
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
    .overlays-page {
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

    .color-field {
        display: flex;
        gap: 0.375rem;
        align-items: center;
    }

    .color-field input[type='color'] {
        width: 36px;
        height: 32px;
        padding: 2px;
        cursor: pointer;
    }

    .color-text {
        flex: 1;
        font-family: monospace;
    }

    .var-inserter {
        display: flex;
        align-items: center;
        gap: 0.5rem;
    }

    .var-inserter select {
        flex: 1;
    }

    .editor-grid {
        display: grid;
        grid-template-columns: 1fr 320px;
        gap: 1.5rem;
        align-items: flex-start;
    }

    .editor-form, .preview-pane {
        background-color: var(--color-bg-surface);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-md);
        padding: 1.25rem;
        display: flex;
        flex-direction: column;
        gap: 1rem;
        position: sticky;
        top: 1rem;
    }

    .preview-pane {
        position: static;
    }

    .preview-image-wrap {
        background-color: var(--color-bg-deep);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
        padding: 0.5rem;
        text-align: center;
    }

    .preview-image-wrap img {
        max-width: 100%;
        border-radius: var(--radius-sm);
    }

    .preview-empty {
        font-size: 0.75rem;
        color: var(--color-text-muted);
        padding: 2rem 1rem;
        text-align: center;
        border: 1px dashed var(--color-border);
        border-radius: var(--radius-sm);
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

    .overlay-list {
        display: flex;
        flex-direction: column;
        gap: 0.5rem;
    }

    .overlay-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 1rem;
        padding: 0.75rem 0.875rem;
        background-color: var(--color-bg-elevated);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
    }

    .overlay-row.disabled {
        opacity: 0.55;
    }

    .overlay-name {
        font-size: 0.875rem;
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .overlay-meta {
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

    .type-image { color: #6abf69; background-color: var(--color-success-bg); }
    .type-text { color: var(--color-accent); background-color: var(--color-accent-muted); }
    .type-backdrop { color: var(--color-warning); background-color: var(--color-warning-bg); }

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

    .overlay-actions {
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

    @media (max-width: 900px) {
        .editor-grid {
            grid-template-columns: 1fr;
        }
    }

    @media (max-width: 768px) {
        .form-grid {
            grid-template-columns: 1fr;
        }

        .page-header, .overlay-row {
            flex-direction: column;
            align-items: flex-start;
        }

        .header-actions {
            width: 100%;
            flex-wrap: wrap;
        }
    }
</style>
