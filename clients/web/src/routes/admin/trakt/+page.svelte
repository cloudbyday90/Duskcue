<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { hasCapability } from '$lib/stores/auth.js';
    import { notifications } from '$lib/stores/notifications.js';
    import {
        getTraktIntegrationSettings,
        updateTraktIntegrationSettings,
    } from '$lib/api/trakt.js';

    let canManage = $state(false);
    let loading = $state(true);
    let loadedOnce = $state(false);
    let loadError = $state('');
    let saveError = $state('');
    let saving = $state(false);
    let settings = $state(null);
    let form = $state({ client_id: '', client_secret: '', redirect_uri: '' });

    $effect(() => {
        const unsubscribe = hasCapability('can_manage_server').subscribe((value) => (canManage = value));
        return unsubscribe;
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
        loadError = '';
        try {
            settings = await getTraktIntegrationSettings();
            form = {
                client_id: settings.client_id || '',
                client_secret: '',
                redirect_uri: settings.redirect_uri || '',
            };
        } catch (err) {
            loadError = err.detail || err.message || 'Failed to load Trakt integration settings.';
        } finally {
            loading = false;
        }
    }

    async function save() {
        saving = true;
        saveError = '';
        try {
            const payload = {
                client_id: form.client_id.trim(),
                redirect_uri: form.redirect_uri.trim(),
            };
            if (form.client_secret.trim()) {
                payload.client_secret = form.client_secret.trim();
            }
            settings = await updateTraktIntegrationSettings(payload);
            form.client_secret = '';
            notifications.success('Trakt integration settings saved.');
        } catch (err) {
            saveError = err.detail || err.message || 'Failed to save Trakt integration settings.';
            notifications.error(saveError);
        } finally {
            saving = false;
        }
    }
</script>

<div class="trakt-admin">
    <header class="page-header">
        <div>
            <a href="/admin" class="back-link">Admin</a>
            <h1 class="page-title">Trakt Integration</h1>
            <p class="page-description">Set the server credentials used when people link their personal Trakt accounts.</p>
        </div>
    </header>

    {#if loading}
        <div class="loading-state"><div class="loading-spinner"></div></div>
    {:else if !canManage}
        <div class="empty-state">You do not have permission to manage Trakt credentials.</div>
    {:else if loadError}
        <div class="empty-state">
            <p class="error-text">{loadError}</p>
            <button class="btn-secondary" onclick={load}>Retry</button>
        </div>
    {:else}
        <section class="settings-card">
            <div class="card-header">
                <div>
                    <h2>Application credentials</h2>
                    <p>Register a Duskcue application in Trakt, then store its client ID, client secret, and matching redirect URI here.</p>
                </div>
                <span class:configured={settings?.is_configured} class="status-badge">
                    {settings?.is_configured ? 'Configured' : 'Needs credentials'}
                </span>
            </div>

            <form class="card-body" onsubmit={(event) => { event.preventDefault(); save(); }}>
                <label class="field">
                    <span>Client ID</span>
                    <input type="text" bind:value={form.client_id} autocomplete="off" maxlength="256" required />
                </label>
                <label class="field">
                    <span>Client secret</span>
                    <input
                        type="password"
                        bind:value={form.client_secret}
                        autocomplete="new-password"
                        maxlength="512"
                        placeholder={settings?.has_client_secret ? `${settings.client_secret_masked} — leave blank to keep` : 'Enter client secret'}
                    />
                    <small>{settings?.has_client_secret ? 'A secret is already stored securely.' : 'Required before users can link Trakt accounts.'}</small>
                </label>
                <label class="field">
                    <span>Redirect URI</span>
                    <input type="url" bind:value={form.redirect_uri} maxlength="512" required />
                    <small>This URI must exactly match the URI registered with Trakt.</small>
                </label>
                {#if saveError}<p class="error-text">{saveError}</p>{/if}
                <div class="form-actions">
                    <button class="btn-primary" type="submit" disabled={saving}>
                        {saving ? 'Saving…' : 'Save credentials'}
                    </button>
                </div>
            </form>
        </section>
    {/if}
</div>

<style>
    .trakt-admin { display: flex; flex-direction: column; gap: 1.5rem; max-width: 760px; }
    .page-header { display: flex; flex-direction: column; gap: 0.25rem; }
    .back-link, small { color: var(--color-text-muted); font-size: 0.8125rem; }
    .page-title { margin: 0; color: var(--color-text-primary); font-size: 1.5rem; }
    .page-description, .card-header p { margin: 0; color: var(--color-text-secondary); }
    .settings-card, .empty-state { border: 1px solid var(--color-border-subtle); border-radius: var(--radius-sm); background: var(--color-bg-surface); }
    .loading-state, .empty-state { display: grid; min-height: 160px; place-items: center; padding: 1rem; }
    .loading-spinner { width: 1.5rem; height: 1.5rem; border: 2px solid var(--color-border); border-top-color: var(--color-accent); border-radius: 50%; animation: spin 0.8s linear infinite; }
    .card-header { display: flex; align-items: flex-start; justify-content: space-between; gap: 1rem; padding: 1rem; border-bottom: 1px solid var(--color-border-subtle); }
    .card-header h2 { margin: 0 0 0.25rem; color: var(--color-text-primary); font-size: 1rem; }
    .status-badge { flex: 0 0 auto; border-radius: 999px; padding: 0.25rem 0.5rem; color: var(--color-text-muted); background: var(--color-bg-elevated); font-size: 0.75rem; }
    .status-badge.configured { color: var(--color-success); }
    .card-body { display: flex; flex-direction: column; gap: 1rem; padding: 1rem; }
    .field { display: flex; flex-direction: column; gap: 0.375rem; color: var(--color-text-primary); font-size: 0.875rem; font-weight: 600; }
    input { width: 100%; box-sizing: border-box; border: 1px solid var(--color-border); border-radius: var(--radius-sm); padding: 0.625rem 0.75rem; color: var(--color-text-primary); background: var(--color-bg-elevated); font: inherit; font-weight: 400; }
    .form-actions { display: flex; justify-content: flex-end; }
    .error-text { margin: 0; color: var(--color-error); }
    @keyframes spin { to { transform: rotate(360deg); } }
</style>
