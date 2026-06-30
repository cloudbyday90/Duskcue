<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import { onMount } from 'svelte';
    import { getHealth } from '$lib/api/settings.js';
    import { notifications } from '$lib/stores/notifications.js';

    let loading = $state(true);
    let health = $state(null);

    onMount(async () => {
        try {
            health = await getHealth();
        } catch {
        } finally {
            loading = false;
        }
    });

    const settingsLinks = [
        { href: '/settings/system', label: m.routes_settings_page_system(), icon: 'M4 7h16M4 12h16M4 17h16', desc: m.routes_settings_page_server_configuration_and_operations() },
        { href: '/settings/users', label: m.routes_settings_page_users(), icon: 'M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2M9 7a4 4 0 1 0 0 .01', desc: m.routes_settings_page_manage_user_accounts_and_invitations() },
        { href: '/settings/libraries', label: m.routes_settings_page_libraries(), icon: 'M2 3h20v18H2zM2 8h20', desc: m.routes_settings_page_configure_media_libraries_and_scan_paths() },
        { href: '/settings/quality', label: m.routes_settings_page_quality(), icon: 'M3 3v18h18', desc: m.routes_settings_page_streaming_quality_and_transcoding(), soon: true },
        { href: '/settings/subtitles', label: m.routes_settings_page_subtitles(), icon: 'M4 4h16v16H4z', desc: m.routes_settings_page_subtitle_preferences_and_providers() },
        { href: '/settings/overlays', label: m.routes_settings_page_overlays(), icon: 'M3 3h18v18H3z', desc: m.routes_settings_page_artwork_overlays_and_posters() },
        { href: '/settings/collections', label: m.routes_settings_page_collections(), icon: 'M3 3h18v18H3z', desc: m.routes_settings_page_collection_management() },
        { href: '/settings/notifications', label: m.routes_settings_page_notifications(), icon: 'M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9M13.73 21a2 2 0 0 1-3.46 0', desc: m.routes_settings_page_notification_feed_preferences_and_push_devices() },
        { href: '/settings/backups', label: m.routes_settings_page_backups(), icon: 'M21 8v13H3V8M1 3h22v5H1z', desc: m.routes_settings_page_backup_and_recovery() },
        { href: '/settings/migration', label: m.routes_settings_page_migration(), icon: 'M3 12h18M3 6h18M3 18h18', desc: m.routes_settings_page_import_from_other_platforms() },
        { href: '/settings/security', label: m.routes_settings_page_security(), icon: 'M12 2l8 4v6c0 5-3.5 9-8 10-4.5-1-8-5-8-10V6z', desc: m.routes_settings_page_security_settings(), soon: true },
        { href: '/settings/storage', label: m.routes_settings_page_storage(), icon: 'M3 3h18v18H3z', desc: m.routes_settings_page_cache_and_storage_management(), soon: true },
    ];
</script>

<div class="settings-page">
    <h1 class="page-title">{m.routes_settings_page_settings()}</h1>

    <div class="settings-grid">
        <section class="settings-section">
            <h2 class="section-title">{m.routes_settings_page_server_status()}</h2>
            {#if loading}
                <div class="status-loading">{m.routes_settings_page_checking_server_health()}</div>
            {:else if health}
                <div class="status-grid">
                    <div class="status-item">
                        <span class="status-label">{m.routes_settings_page_status()}</span>
                        <span class="status-value status-{$health.status || 'unknown'}">
                            {$health.status || 'Unknown'}
                        </span>
                    </div>
                    <div class="status-item">
                        <span class="status-label">{m.routes_settings_page_version()}</span>
                        <span class="status-value">{$health.version || '—'}</span>
                    </div>
                    <div class="status-item">
                        <span class="status-label">{m.routes_settings_page_database()}</span>
                        <span class="status-value">{$health.database || '—'}</span>
                    </div>
                    <div class="status-item">
                        <span class="status-label">{m.routes_settings_page_uptime()}</span>
                        <span class="status-value">
                            {$health.uptime_seconds
                                ? Math.floor($health.uptime_seconds / 3600) + 'h ' +
                                  Math.floor(($health.uptime_seconds % 3600) / 60) + 'm'
                                : '—'}
                        </span>
                    </div>
                </div>
                {#if health.hardware_acceleration}
                    <div class="hw-accel">
                        <span class="status-label">{m.routes_settings_page_hardware_acceleration()}</span>
                        <div class="hw-badge">{$health.hardware_acceleration.method}</div>
                    </div>
                {/if}
            {:else}
                <div class="status-error">{m.routes_settings_page_unable_to_fetch_server_status()}</div>
            {/if}
        </section>

        <section class="settings-section">
            <h2 class="section-title">{m.routes_settings_page_management()}</h2>
            <div class="links-grid">
                {#each settingsLinks as link}
                    <a
                        href={link.soon ? undefined : link.href}
                        class="settings-link"
                        class:disabled={link.soon}
                    >
                        <div class="link-icon">
                            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                                <path d={link.icon} />
                            </svg>
                        </div>
                        <div class="link-text">
                            <span class="link-label">
                                {link.label}
                                {#if link.soon}<span class="soon-tag">{m.routes_settings_page_soon()}</span>{/if}
                            </span>
                            <span class="link-desc">{link.desc}</span>
                        </div>
                    </a>
                {/each}
            </div>
        </section>
    </div>
</div>

<style>
    .settings-page {
        display: flex;
        flex-direction: column;
        gap: 2rem;
        max-width: 900px;
    }

    .page-title {
        font-size: 1.5rem;
        font-weight: 700;
        color: var(--color-text-primary);
    }

    .settings-grid {
        display: flex;
        flex-direction: column;
        gap: 2rem;
    }

    .settings-section {
        display: flex;
        flex-direction: column;
        gap: 1rem;
    }

    .section-title {
        font-size: 1rem;
        font-weight: 600;
        color: var(--color-text-secondary);
        text-transform: uppercase;
        font-size: 0.75rem;
        letter-spacing: 0.05em;
    }

    .status-grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
        gap: 0.75rem;
    }

    .status-item {
        display: flex;
        flex-direction: column;
        gap: 0.25rem;
        padding: 0.875rem 1rem;
        background-color: var(--color-bg-surface);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
    }

    .status-label {
        font-size: 0.6875rem;
        font-weight: 500;
        text-transform: uppercase;
        letter-spacing: 0.05em;
        color: var(--color-text-muted);
    }

    .status-value {
        font-size: 0.9375rem;
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .status-healthy {
        color: var(--color-success);
    }

    .status-degraded {
        color: var(--color-warning);
    }

    .status-loading,
    .status-error {
        font-size: 0.8125rem;
        color: var(--color-text-muted);
        padding: 0.875rem 1rem;
    }

    .status-error {
        color: var(--color-error);
    }

    .hw-accel {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 0.875rem 1rem;
        background-color: var(--color-bg-surface);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
    }

    .hw-badge {
        font-size: 0.75rem;
        font-weight: 600;
        text-transform: uppercase;
        color: var(--color-accent);
        background-color: var(--color-accent-muted);
        padding: 0.25rem 0.625rem;
        border-radius: var(--radius-sm);
    }

    .links-grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
        gap: 0.75rem;
    }

    .settings-link {
        display: flex;
        align-items: center;
        gap: 0.75rem;
        padding: 0.875rem 1rem;
        background-color: var(--color-bg-surface);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
        transition: border-color var(--transition-fast), background-color var(--transition-fast);
    }

    .settings-link:hover:not(.disabled) {
        border-color: var(--color-accent);
        background-color: var(--color-bg-elevated);
    }

    .settings-link.disabled {
        opacity: 0.5;
        cursor: default;
    }

    .link-icon {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 36px;
        height: 36px;
        background-color: var(--color-bg-elevated);
        border-radius: var(--radius-sm);
        color: var(--color-text-secondary);
        flex-shrink: 0;
    }

    .settings-link:hover:not(.disabled) .link-icon {
        color: var(--color-accent);
    }

    .link-text {
        display: flex;
        flex-direction: column;
        gap: 0.125rem;
        min-width: 0;
    }

    .link-label {
        font-size: 0.875rem;
        font-weight: 600;
        color: var(--color-text-primary);
        display: flex;
        align-items: center;
        gap: 0.5rem;
    }

    .soon-tag {
        font-size: 0.5625rem;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.05em;
        color: var(--color-text-muted);
        background-color: var(--color-bg-hover);
        padding: 0.0625rem 0.375rem;
        border-radius: 3px;
    }

    .link-desc {
        font-size: 0.75rem;
        color: var(--color-text-muted);
    }

    @media (max-width: 768px) {
        .page-title {
            font-size: 1.25rem;
        }

        .links-grid {
            grid-template-columns: 1fr;
        }
    }
</style>
