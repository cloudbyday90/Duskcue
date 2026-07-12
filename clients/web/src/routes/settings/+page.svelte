<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import { onMount } from 'svelte';
    import { setLocale } from '$lib/paraglide/runtime.js';
    import { getUserPreferences, updateUserPreferences } from '$lib/api/users.js';
    import { auth, currentUser, userHasAnyCapability } from '$lib/stores/auth.js';
    import { notifications } from '$lib/stores/notifications.js';

    const ADMIN_CAPABILITIES = ['can_manage_server', 'can_manage_users', 'can_manage_libraries'];

    let preferencesLoading = $state(true);
    let preferences = $state(null);
    let selectedLocale = $state('en');
    let savingLocale = $state(false);
    let preferencesError = $state('');
    let canAccessAdmin = $derived(userHasAnyCapability($currentUser, ADMIN_CAPABILITIES));

    onMount(loadPreferences);

    async function loadPreferences() {
        preferencesLoading = true;
        preferencesError = '';
        try {
            preferences = await getUserPreferences();
            selectedLocale = preferences.locale || 'en';
        } catch {
            preferencesError = m.routes_settings_page_failed_to_load_preferences();
        } finally {
            preferencesLoading = false;
        }
    }

    async function handleLocaleChange(event) {
        const locale = event.currentTarget.value;
        const previousLocale = preferences?.locale || selectedLocale;
        selectedLocale = locale;
        savingLocale = true;
        preferencesError = '';

        try {
            preferences = await updateUserPreferences({ locale });
            selectedLocale = preferences.locale;
            if ($currentUser) {
                auth.setUser({ ...$currentUser, locale: preferences.locale });
            }
            setLocale(preferences.locale);
        } catch {
            selectedLocale = previousLocale;
            preferencesError = m.routes_settings_page_failed_to_save_language();
            notifications.error(preferencesError);
        } finally {
            savingLocale = false;
        }
    }
</script>

<div class="settings-page">
    <header class="page-header">
        <h1 class="page-title">{m.routes_settings_page_settings()}</h1>
        <p class="page-description">{m.routes_settings_page_manage_your_preferences()}</p>
    </header>

    <section class="settings-section" aria-labelledby="personal-heading">
        <h2 id="personal-heading" class="section-title">{m.routes_settings_page_personal()}</h2>
        <div class="settings-card preference-card">
            <div class="preference-copy">
                <span class="preference-label">{m.routes_settings_page_language()}</span>
                <span class="preference-description">{m.routes_settings_page_language_selector_reviewed_locales_only()}</span>
            </div>
            {#if preferencesLoading}
                <span class="state-copy">{m.routes_settings_page_loading_preferences()}</span>
            {:else if preferences}
                <select
                    class="language-select"
                    value={selectedLocale}
                    onchange={handleLocaleChange}
                    disabled={savingLocale || preferences.available_locales.length <= 1}
                    aria-label={m.routes_settings_page_language()}
                >
                    {#each preferences.available_locales as locale}
                        <option value={locale.tag}>{locale.name}</option>
                    {/each}
                </select>
            {:else}
                <button class="btn-secondary" onclick={loadPreferences}>
                    {m.routes_settings_page_retry()}
                </button>
            {/if}
        </div>
        {#if preferencesError}
            <p class="error-copy">{preferencesError}</p>
        {/if}
    </section>

    <section class="settings-section" aria-labelledby="notifications-heading">
        <h2 id="notifications-heading" class="section-title">{m.routes_settings_page_notifications()}</h2>
        <a href="/settings/notifications" class="settings-link">
            <div class="link-icon" aria-hidden="true">
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9M13.73 21a2 2 0 0 1-3.46 0" />
                </svg>
            </div>
            <div>
                <span class="link-label">{m.routes_settings_page_notifications()}</span>
                <span class="link-description">{m.routes_settings_page_notification_feed_preferences_and_push_devices()}</span>
            </div>
        </a>
    </section>

    {#if canAccessAdmin}
        <section class="settings-section" aria-labelledby="admin-heading">
            <h2 id="admin-heading" class="section-title">{m.routes_settings_page_admin()}</h2>
            <a href="/admin" class="settings-link">
                <div class="link-icon" aria-hidden="true">
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                        <path d="M4 7h16M4 12h16M4 17h16" />
                    </svg>
                </div>
                <div>
                    <span class="link-label">{m.routes_settings_page_admin()}</span>
                    <span class="link-description">{m.routes_settings_page_manage_server()}</span>
                </div>
            </a>
        </section>
    {/if}
</div>

<style>
    .settings-page {
        display: flex;
        flex-direction: column;
        gap: 2rem;
        max-width: 720px;
    }

    .page-header {
        display: flex;
        flex-direction: column;
        gap: 0.25rem;
    }

    .page-title {
        font-size: 1.5rem;
        font-weight: 700;
        color: var(--color-text-primary);
    }

    .page-description {
        color: var(--color-text-secondary);
    }

    .settings-section {
        display: flex;
        flex-direction: column;
        gap: 0.75rem;
    }

    .section-title {
        font-size: 0.75rem;
        font-weight: 600;
        letter-spacing: 0.05em;
        text-transform: uppercase;
        color: var(--color-text-secondary);
    }

    .settings-card,
    .settings-link {
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
        background: var(--color-bg-surface);
    }

    .preference-card {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 1rem;
        padding: 0.875rem 1rem;
    }

    .preference-copy {
        display: flex;
        min-width: 0;
        flex-direction: column;
        gap: 0.125rem;
    }

    .preference-label,
    .link-label {
        font-size: 0.875rem;
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .preference-description,
    .link-description,
    .state-copy {
        font-size: 0.75rem;
        color: var(--color-text-muted);
    }

    .language-select,
    .btn-secondary {
        flex: 0 0 auto;
        min-width: 180px;
        padding: 0.5rem 0.75rem;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
        background-color: var(--color-bg-elevated);
    }

    .language-select:disabled {
        opacity: 0.65;
    }

    .settings-link {
        display: flex;
        align-items: center;
        gap: 0.75rem;
        padding: 0.875rem 1rem;
        transition: border-color var(--transition-fast), background-color var(--transition-fast);
    }

    .settings-link:hover {
        border-color: var(--color-accent);
        background: var(--color-bg-elevated);
    }

    .link-icon {
        display: grid;
        flex: 0 0 auto;
        width: 2.25rem;
        height: 2.25rem;
        place-items: center;
        border-radius: var(--radius-sm);
        color: var(--color-text-secondary);
        background: var(--color-bg-elevated);
    }

    .settings-link:hover .link-icon {
        color: var(--color-accent);
    }

    .link-description {
        display: block;
        margin-top: 0.125rem;
    }

    .error-copy {
        font-size: 0.8125rem;
        color: var(--color-error);
    }

    @media (max-width: 640px) {
        .preference-card {
            align-items: stretch;
            flex-direction: column;
        }

        .language-select,
        .btn-secondary {
            width: 100%;
        }
    }
</style>
