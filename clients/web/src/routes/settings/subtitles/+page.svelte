<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import { subtitleSettings } from '$lib/stores/subtitles.js';
    import { hasCapability } from '$lib/stores/auth.js';
    import { notifications } from '$lib/stores/notifications.js';

    let loading = $state(true);
    let canManage = $state(false);
    let loadError = $state(null);
    let loadedOnce = $state(false);

    let form = $state(defaultForm());
    let original = $state(snapshotForm(defaultForm()));

    let savingBehavior = $state(false);
    let savingProviders = $state(false);

    $effect(() => {
        const unsub = hasCapability('can_manage_server').subscribe((v) => (canManage = v));
        return unsub;
    });

    $effect(() => {
        if (!canManage) {
            loading = false;
            return;
        }
        if (loadedOnce) return;
        loadedOnce = true;
        load();
    });

    async function load() {
        loading = true;
        loadError = null;
        try {
            const settings = await subtitleSettings.fetch();
            hydrateForm(settings);
        } catch (err) {
            loadError = err.detail || err.message || m.routes_settings_subtitles_page_failed_to_load_subtitle_settings();
        } finally {
            loading = false;
        }
    }

    function defaultForm() {
        return {
            ocr_enabled: true,
            ocr_engine: 'paddleocr',
            ocr_confidence_threshold: 0.8,
            voice_activity_analysis: false,
            voice_activity_schedule: '0 5 * * *',
            default_subtitle_mode: 'default',
            default_subtitle_language: 'en',
            auto_fetch_enabled: false,
            auto_fetch_languages: '',
            subdl: {
                enabled: false,
                api_key: '',
                auto_fetch_enabled: false,
                auto_fetch_languages: '',
                prefer_hearing_impaired: false,
                has_api_key: false,
            },
            opensubtitles: {
                enabled: false,
                api_key: '',
                api_token: '',
                auto_fetch_enabled: false,
                auto_fetch_languages: '',
                prefer_hearing_impaired: false,
                has_api_key: false,
                has_api_token: false,
            },
        };
    }

    function snapshotForm(f) {
        return JSON.parse(JSON.stringify(f));
    }

    function hydrateForm(settings) {
        form = {
            ocr_enabled: settings.ocr_enabled,
            ocr_engine: settings.ocr_engine,
            ocr_confidence_threshold: settings.ocr_confidence_threshold,
            voice_activity_analysis: settings.voice_activity_analysis,
            voice_activity_schedule: settings.voice_activity_schedule,
            default_subtitle_mode: settings.default_subtitle_mode,
            default_subtitle_language: settings.default_subtitle_language,
            auto_fetch_enabled: settings.auto_fetch_enabled,
            auto_fetch_languages: (settings.auto_fetch_languages || []).join(', '),
            subdl: {
                enabled: settings.providers.subdl.enabled,
                api_key: '',
                auto_fetch_enabled: settings.providers.subdl.auto_fetch_enabled,
                auto_fetch_languages: (settings.providers.subdl.auto_fetch_languages || []).join(', '),
                prefer_hearing_impaired: settings.providers.subdl.prefer_hearing_impaired,
                has_api_key: settings.providers.subdl.has_api_key,
            },
            opensubtitles: {
                enabled: settings.providers.opensubtitles.enabled,
                api_key: '',
                api_token: '',
                auto_fetch_enabled: settings.providers.opensubtitles.auto_fetch_enabled,
                auto_fetch_languages: (settings.providers.opensubtitles.auto_fetch_languages || []).join(', '),
                prefer_hearing_impaired: settings.providers.opensubtitles.prefer_hearing_impaired,
                has_api_key: settings.providers.opensubtitles.has_api_key,
                has_api_token: settings.providers.opensubtitles.has_api_token,
            },
        };
        original = snapshotForm(form);
    }

    let behaviorDirty = $derived(!shallowEqual(stripProviders(form), stripProviders(original)));
    let providersDirty = $derived(
        !shallowEqual(form.subdl, original.subdl) ||
            !shallowEqual(form.opensubtitles, original.opensubtitles),
    );

    function stripProviders(f) {
        const { subdl, opensubtitles, ...rest } = f;
        return rest;
    }

    function shallowEqual(a, b) {
        const ak = Object.keys(a);
        const bk = Object.keys(b);
        if (ak.length !== bk.length) return false;
        for (const k of ak) {
            if (a[k] !== b[k]) return false;
        }
        return true;
    }

    function parseLanguages(str) {
        return (str || '')
            .split(',')
            .map((s) => s.trim())
            .filter((s) => s.length > 0);
    }

    async function saveBehavior() {
        savingBehavior = true;
        try {
            const settings = await subtitleSettings.saveSettings({
                ocr_enabled: form.ocr_enabled,
                ocr_engine: form.ocr_engine,
                ocr_confidence_threshold: form.ocr_confidence_threshold,
                voice_activity_analysis: form.voice_activity_analysis,
                voice_activity_schedule: form.voice_activity_schedule,
                default_subtitle_mode: form.default_subtitle_mode,
                default_subtitle_language: form.default_subtitle_language.trim(),
                auto_fetch_enabled: form.auto_fetch_enabled,
                auto_fetch_languages: parseLanguages(form.auto_fetch_languages),
            });
            hydrateForm(settings);
            notifications.success(m.routes_settings_subtitles_page_subtitle_settings_saved());
        } catch (err) {
            notifications.error(err.detail || err.message || m.routes_settings_subtitles_page_failed_to_save_subtitle_settings());
        } finally {
            savingBehavior = false;
        }
    }

    async function saveProviders() {
        savingProviders = true;
        try {
            const payload = {
                subdl: {
                    enabled: form.subdl.enabled,
                    api_key: form.subdl.api_key.length > 0 ? form.subdl.api_key.trim() : null,
                    auto_fetch_enabled: form.subdl.auto_fetch_enabled,
                    auto_fetch_languages: parseLanguages(form.subdl.auto_fetch_languages),
                    prefer_hearing_impaired: form.subdl.prefer_hearing_impaired,
                },
                opensubtitles: {
                    enabled: form.opensubtitles.enabled,
                    api_key: form.opensubtitles.api_key.length > 0 ? form.opensubtitles.api_key.trim() : null,
                    api_token: form.opensubtitles.api_token.length > 0 ? form.opensubtitles.api_token.trim() : null,
                    auto_fetch_enabled: form.opensubtitles.auto_fetch_enabled,
                    auto_fetch_languages: parseLanguages(form.opensubtitles.auto_fetch_languages),
                    prefer_hearing_impaired: form.opensubtitles.prefer_hearing_impaired,
                },
            };
            const settings = await subtitleSettings.saveProviders(payload);
            hydrateForm(settings);
            notifications.success(m.routes_settings_subtitles_page_subtitle_provider_settings_saved());
        } catch (err) {
            notifications.error(err.detail || err.message || m.routes_settings_subtitles_page_failed_to_save_provider_settings());
        } finally {
            savingProviders = false;
        }
    }
</script>

<div class="sub-settings">
    <div class="page-header">
        <div>
            <a href="/admin" class="back-link">{m.routes_admin_page_admin()}</a>
            <h1 class="page-title">{m.routes_settings_subtitles_page_subtitles()}</h1>
        </div>
    </div>

    {#if loading}
        <div class="loading-state">
            <div class="loading-spinner"></div>
        </div>
    {:else if !canManage}
        <div class="empty-state">
            <p>{m.routes_settings_subtitles_page_you_do_not_have_permission_to_manage_subtitle_se()}</p>
        </div>
    {:else if loadError}
        <div class="empty-state">
            <p class="error-text">{loadError}</p>
            <button class="btn-secondary" onclick={load}>{m.routes_settings_subtitles_page_retry()}</button>
        </div>
    {:else}
        <section class="settings-card">
            <div class="card-header">
                <h2 class="card-title">{m.routes_settings_subtitles_page_subtitle_behavior()}</h2>
                <button
                    class="btn-primary"
                    onclick={saveBehavior}
                    disabled={!behaviorDirty || savingBehavior}
                >
                    {savingBehavior ? 'Saving…' : 'Save'}
                </button>
            </div>

            <div class="card-body">
                <div class="form-grid">
                    <label class="field">
                        <span class="field-label">{m.routes_settings_subtitles_page_default_subtitle_mode()}</span>
                        <select bind:value={form.default_subtitle_mode}>
                            <option value="default">{m.routes_settings_subtitles_page_auto_only_if_audio_differs()}</option>
                            <option value="always">{m.routes_settings_subtitles_page_always_on()}</option>
                            <option value="forced_only">{m.routes_settings_subtitles_page_forced_only()}</option>
                            <option value="none">{m.routes_settings_subtitles_page_off()}</option>
                        </select>
                    </label>
                    <label class="field">
                        <span class="field-label">{m.routes_settings_subtitles_page_default_language()}</span>
                        <input type="text" bind:value={form.default_subtitle_language} placeholder={m.routes_settings_subtitles_page_en()} />
                    </label>
                    <label class="field field-wide">
                        <span class="field-label">{m.routes_settings_subtitles_page_auto_fetch_languages()}</span>
                        <input
                            type="text"
                            bind:value={form.auto_fetch_languages}
                            placeholder={m.routes_settings_subtitles_page_en_es()}
                        />
                        <span class="field-hint">
                            Comma-separated language codes. Subtitles are fetched for these languages
                            when {form.auto_fetch_enabled ? 'enabled' : 'auto-fetch is off'}.
                        </span>
                    </label>
                </div>

                <label class="toggle-row">
                    <input type="checkbox" bind:checked={form.auto_fetch_enabled} />
                    <span class="toggle-text">
                        <span class="toggle-title">{m.routes_settings_subtitles_page_auto_fetch_subtitles()}</span>
                        <span class="toggle-desc">
                            Automatically download missing subtitles during scan.
                        </span>
                    </span>
                </label>
            </div>
        </section>

        <section class="settings-card">
            <div class="card-header">
                <h2 class="card-title">{m.routes_settings_subtitles_page_ocr_image_subtitles()}</h2>
            </div>
            <div class="card-body">
                <label class="toggle-row">
                    <input type="checkbox" bind:checked={form.ocr_enabled} />
                    <span class="toggle-text">
                        <span class="toggle-title">{m.routes_settings_subtitles_page_enable_ocr()}</span>
                        <span class="toggle-desc">
                            Convert PGS/VobSub image subtitles to text (SRT) during scan.
                            Requires PaddleOCR or Tesseract installed on the server.
                        </span>
                    </span>
                </label>

                <div class="form-grid">
                    <label class="field">
                        <span class="field-label">{m.routes_settings_subtitles_page_ocr_engine()}</span>
                        <select bind:value={form.ocr_engine} disabled={!form.ocr_enabled}>
                            <option value="paddleocr">{m.routes_settings_subtitles_page_paddleocr()}</option>
                            <option value="tesseract">{m.routes_settings_subtitles_page_tesseract()}</option>
                        </select>
                    </label>
                    <label class="field">
                        <span class="field-label">Confidence Threshold ({form.ocr_confidence_threshold.toFixed(2)})</span>
                        <input
                            type="range"
                            min="0.5"
                            max="1"
                            step="0.05"
                            bind:value={form.ocr_confidence_threshold}
                            disabled={!form.ocr_enabled}
                        />
                        <span class="field-hint">{m.routes_settings_subtitles_page_below_this_confidence_results_are_flagged_for_re()}</span>
                    </label>
                </div>

                <label class="toggle-row">
                    <input type="checkbox" bind:checked={form.voice_activity_analysis} />
                    <span class="toggle-text">
                        <span class="toggle-title">{m.routes_settings_subtitles_page_voice_activity_sync()}</span>
                        <span class="toggle-desc">
                            Align external subtitles to audio via voice activity detection. CPU-intensive
                            background task.
                        </span>
                    </span>
                </label>
                <label class="field">
                    <span class="field-label">{m.routes_settings_subtitles_page_voice_activity_schedule_cron()}</span>
                    <input
                        type="text"
                        bind:value={form.voice_activity_schedule}
                        disabled={!form.voice_activity_analysis}
                        placeholder="0 5 * * *"
                    />
                </label>
            </div>
        </section>

        <section class="settings-card">
            <div class="card-header">
                <h2 class="card-title">{m.routes_settings_subtitles_page_subtitle_providers()}</h2>
                <button
                    class="btn-primary"
                    onclick={saveProviders}
                    disabled={!providersDirty || savingProviders}
                >
                    {savingProviders ? 'Saving…' : 'Save Providers'}
                </button>
            </div>
            <div class="card-body">
                <p class="section-note">
                    API keys are encrypted at rest. Leave the key field blank to keep the existing key.
                </p>

                <div class="provider-grid">
                    <div class="provider-card">
                        <div class="provider-head">
                            <label class="toggle-inline">
                                <input type="checkbox" bind:checked={form.subdl.enabled} />
                                <span class="provider-name">{m.routes_settings_subtitles_page_subdl()}</span>
                            </label>
                            {#if form.subdl.has_api_key}
                                <span class="badge badge-on">{m.routes_settings_subtitles_page_key_set()}</span>
                            {/if}
                        </div>
                        <p class="provider-desc">
                            Primary provider. Free tier: 2,000 requests/day, 300 downloads/day.
                        </p>
                        <label class="field">
                            <span class="field-label">{m.routes_settings_subtitles_page_api_key()}</span>
                            <input
                                type="password"
                                bind:value={form.subdl.api_key}
                                placeholder={form.subdl.has_api_key ? '••••• (leave blank to keep)' : 'Enter API key'}
                            />
                        </label>
                        <label class="field">
                            <span class="field-label">{m.routes_settings_subtitles_page_auto_fetch_languages()}</span>
                            <input
                                type="text"
                                bind:value={form.subdl.auto_fetch_languages}
                                placeholder={m.routes_settings_subtitles_page_en()}
                                disabled={!form.subdl.enabled}
                            />
                        </label>
                        <label class="toggle-row">
                            <input type="checkbox" bind:checked={form.subdl.auto_fetch_enabled} disabled={!form.subdl.enabled} />
                            <span class="toggle-text">
                                <span class="toggle-title">{m.routes_settings_subtitles_page_auto_fetch_enabled()}</span>
                            </span>
                        </label>
                        <label class="toggle-row">
                            <input type="checkbox" bind:checked={form.subdl.prefer_hearing_impaired} disabled={!form.subdl.enabled} />
                            <span class="toggle-text">
                                <span class="toggle-title">{m.routes_settings_subtitles_page_prefer_hearing_impaired()}</span>
                            </span>
                        </label>
                    </div>

                    <div class="provider-card">
                        <div class="provider-head">
                            <label class="toggle-inline">
                                <input type="checkbox" bind:checked={form.opensubtitles.enabled} />
                                <span class="provider-name">{m.routes_settings_subtitles_page_opensubtitles()}</span>
                            </label>
                            {#if form.opensubtitles.has_api_key}
                                <span class="badge badge-on">{m.routes_settings_subtitles_page_key_set()}</span>
                            {/if}
                        </div>
                        <p class="provider-desc">
                            Secondary provider. Largest library, hash-based matching. VIP for meaningful downloads.
                        </p>
                        <label class="field">
                            <span class="field-label">{m.routes_settings_subtitles_page_api_key()}</span>
                            <input
                                type="password"
                                bind:value={form.opensubtitles.api_key}
                                placeholder={form.opensubtitles.has_api_key ? '••••• (leave blank to keep)' : 'Enter API key'}
                            />
                        </label>
                        <label class="field">
                            <span class="field-label">{m.routes_settings_subtitles_page_api_token()}</span>
                            <input
                                type="password"
                                bind:value={form.opensubtitles.api_token}
                                placeholder={form.opensubtitles.has_api_token ? '••••• (leave blank to keep)' : 'Enter API token'}
                            />
                        </label>
                        <label class="field">
                            <span class="field-label">{m.routes_settings_subtitles_page_auto_fetch_languages()}</span>
                            <input
                                type="text"
                                bind:value={form.opensubtitles.auto_fetch_languages}
                                placeholder={m.routes_settings_subtitles_page_en()}
                                disabled={!form.opensubtitles.enabled}
                            />
                        </label>
                        <label class="toggle-row">
                            <input type="checkbox" bind:checked={form.opensubtitles.auto_fetch_enabled} disabled={!form.opensubtitles.enabled} />
                            <span class="toggle-text">
                                <span class="toggle-title">{m.routes_settings_subtitles_page_auto_fetch_enabled()}</span>
                            </span>
                        </label>
                        <label class="toggle-row">
                            <input type="checkbox" bind:checked={form.opensubtitles.prefer_hearing_impaired} disabled={!form.opensubtitles.enabled} />
                            <span class="toggle-text">
                                <span class="toggle-title">{m.routes_settings_subtitles_page_prefer_hearing_impaired()}</span>
                            </span>
                        </label>
                    </div>
                </div>
            </div>
        </section>
    {/if}
</div>

<style>
    .sub-settings {
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

    .settings-card {
        background-color: var(--color-bg-surface);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-md);
        overflow: hidden;
    }

    .card-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 1rem;
        padding: 1rem 1.25rem;
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .card-title {
        font-size: 1rem;
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .card-body {
        display: flex;
        flex-direction: column;
        gap: 1.25rem;
        padding: 1.25rem;
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

    .field-hint {
        font-size: 0.6875rem;
        color: var(--color-text-muted);
    }

    input,
    select {
        padding: 0.5rem 0.625rem;
        background-color: var(--color-bg-elevated);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
        font-size: 0.8125rem;
    }

    input[type='range'] {
        padding: 0;
        border: none;
        background-color: transparent;
    }

    input:focus,
    select:focus {
        outline: none;
        border-color: var(--color-accent);
    }

    input:disabled,
    select:disabled {
        opacity: 0.5;
    }

    .toggle-row {
        display: flex;
        align-items: flex-start;
        gap: 0.625rem;
        cursor: pointer;
    }

    .toggle-row input[type='checkbox'] {
        margin-top: 0.125rem;
        width: auto;
    }

    .toggle-text {
        display: flex;
        flex-direction: column;
        gap: 0.125rem;
    }

    .toggle-title {
        font-size: 0.8125rem;
        font-weight: 500;
        color: var(--color-text-primary);
    }

    .toggle-desc {
        font-size: 0.75rem;
        color: var(--color-text-muted);
    }

    .toggle-inline {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        cursor: pointer;
    }

    .toggle-inline input[type='checkbox'] {
        width: auto;
    }

    .btn-primary {
        padding: 0.5rem 1.25rem;
        background-color: var(--color-accent);
        color: var(--color-bg-deep);
        font-size: 0.8125rem;
        font-weight: 600;
        border-radius: var(--radius-sm);
        transition: background-color var(--transition-fast);
        white-space: nowrap;
    }

    .btn-primary:hover:not(:disabled) {
        background-color: var(--color-accent-hover);
    }

    .btn-primary:disabled {
        opacity: 0.5;
    }

    .btn-secondary {
        padding: 0.5rem 1.25rem;
        background-color: var(--color-bg-elevated);
        color: var(--color-text-secondary);
        font-size: 0.8125rem;
        font-weight: 600;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        transition: all var(--transition-fast);
    }

    .btn-secondary:hover {
        border-color: var(--color-accent);
        color: var(--color-text-primary);
    }

    .section-note {
        font-size: 0.75rem;
        color: var(--color-text-muted);
        margin: 0;
    }

    .provider-grid {
        display: grid;
        grid-template-columns: 1fr 1fr;
        gap: 1rem;
    }

    .provider-card {
        display: flex;
        flex-direction: column;
        gap: 0.875rem;
        padding: 1rem;
        background-color: var(--color-bg-deep);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
    }

    .provider-head {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 0.5rem;
    }

    .provider-name {
        font-size: 0.9375rem;
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .provider-desc {
        font-size: 0.75rem;
        color: var(--color-text-muted);
        margin: 0;
    }

    .badge {
        font-size: 0.625rem;
        font-weight: 600;
        text-transform: uppercase;
        padding: 0.125rem 0.5rem;
        border-radius: var(--radius-sm);
        color: var(--color-text-muted);
        background-color: var(--color-info-bg);
    }

    .badge-on {
        color: var(--color-success);
        background-color: var(--color-success-bg);
    }

    .error-text {
        color: var(--color-error);
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
        .form-grid,
        .provider-grid {
            grid-template-columns: 1fr;
        }

        .page-header {
            flex-direction: column;
            gap: 0.75rem;
        }

        .card-header {
            flex-direction: column;
            align-items: flex-start;
        }
    }
</style>
