<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import { getHealth } from '$lib/api/settings.js';
    import { currentUser, userHasAnyCapability, userHasCapability } from '$lib/stores/auth.js';

    const ADMIN_CAPABILITIES = ['can_manage_server', 'can_manage_users', 'can_manage_libraries'];

    let healthLoading = $state(true);
    let health = $state(null);
    let healthRequested = $state(false);

    const sections = [
        {
            id: 'admin-server-heading',
            title: m.routes_admin_page_server(),
            items: [
                {
                    href: '/settings/system',
                    label: m.routes_settings_page_system(),
                    desc: m.routes_settings_page_server_configuration_and_operations(),
                    capability: 'can_manage_server',
                    icon: 'M4 7h16M4 12h16M4 17h16',
                },
                {
                    href: '/settings/backups',
                    label: m.routes_settings_page_backups(),
                    desc: m.routes_settings_page_backup_and_recovery(),
                    capability: 'can_manage_server',
                    icon: 'M21 8v13H3V8M1 3h22v5H1z',
                },
            ],
        },
        {
            id: 'admin-libraries-heading',
            title: m.routes_admin_page_library_management(),
            items: [
                {
                    href: '/settings/libraries',
                    label: m.routes_settings_page_libraries(),
                    desc: m.routes_settings_page_configure_media_libraries_and_scan_paths(),
                    capability: 'can_manage_libraries',
                    icon: 'M2 3h20v18H2zM2 8h20',
                },
                {
                    href: '/admin/collections',
                    label: m.routes_settings_page_collections(),
                    desc: m.routes_settings_page_collection_management(),
                    capability: 'can_manage_libraries',
                    icon: 'M3 3h18v18H3z',
                },
                {
                    href: '/admin/overlays',
                    label: m.routes_settings_page_overlays(),
                    desc: m.routes_settings_page_artwork_overlays_and_posters(),
                    capability: 'can_manage_libraries',
                    icon: 'M3 3h18v18H3z',
                },
                {
                    href: '/settings/subtitles',
                    label: m.routes_settings_page_subtitles(),
                    desc: m.routes_settings_page_subtitle_preferences_and_providers(),
                    capability: 'can_manage_server',
                    icon: 'M4 4h16v16H4z',
                },
            ],
        },
        {
            id: 'admin-access-heading',
            title: m.routes_admin_page_access_and_delivery(),
            items: [
                {
                    href: '/settings/users',
                    label: m.routes_settings_page_users(),
                    desc: m.routes_settings_page_manage_user_accounts_and_invitations(),
                    capability: 'can_manage_users',
                    icon: 'M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2M9 7a4 4 0 1 0 0 .01',
                },
                {
                    href: '/settings/downloads',
                    label: m.routes_admin_page_downloads(),
                    desc: m.routes_admin_page_offline_download_policy_and_inventory(),
                    capability: 'can_manage_server',
                    icon: 'M12 3v12m0 0l-5-5m5 5l5-5M4 19h16',
                },
                {
                    href: '/admin/notifications',
                    label: m.routes_settings_page_notifications(),
                    desc: m.routes_settings_notifications_page_send_a_test_notification_to_yourself_to_verify_t(),
                    capability: 'can_manage_server',
                    icon: 'M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9M13.73 21a2 2 0 0 1-3.46 0',
                },
            ],
        },
        {
            id: 'admin-advanced-heading',
            title: m.routes_admin_page_advanced(),
            items: [
                {
                    href: '/admin/migration',
                    label: m.routes_settings_page_migration(),
                    desc: m.routes_settings_page_import_from_other_platforms(),
                    capability: 'can_manage_users',
                    icon: 'M3 12h18M3 6h18M3 18h18',
                },
            ],
        },
    ];

    let canAccessAdmin = $derived(userHasAnyCapability($currentUser, ADMIN_CAPABILITIES));
    let canManageServer = $derived(userHasCapability($currentUser, 'can_manage_server'));
    let visibleSections = $derived(
        sections
            .map((section) => ({
                ...section,
                items: section.items.filter((item) => userHasCapability($currentUser, item.capability)),
            }))
            .filter((section) => section.items.length > 0),
    );

    $effect(() => {
        if (!canManageServer || healthRequested) return;
        healthRequested = true;
        loadHealth();
    });

    async function loadHealth() {
        try {
            health = await getHealth();
        } catch {
            health = null;
        } finally {
            healthLoading = false;
        }
    }
</script>

<div class="admin-page">
    <header class="page-header">
        <div>
            <a href="/settings" class="back-link">{m.routes_settings_page_settings()}</a>
            <h1 class="page-title">{m.routes_admin_page_admin()}</h1>
            <p class="page-description">{m.routes_admin_page_manage_your_server()}</p>
        </div>
    </header>

    {#if !canAccessAdmin}
        <div class="empty-state">{m.routes_admin_page_no_admin_access()}</div>
    {:else}
        {#if canManageServer}
            <section class="health-section" aria-labelledby="server-health-heading">
                <div class="section-heading">
                    <h2 id="server-health-heading">{m.routes_settings_page_server_status()}</h2>
                    <a href="/settings/system">{m.routes_settings_page_system()}</a>
                </div>
                {#if healthLoading}
                    <div class="state-copy">{m.routes_settings_page_checking_server_health()}</div>
                {:else if health}
                    <div class="metrics-grid">
                        <div class="metric-card">
                            <span>{m.routes_settings_page_status()}</span>
                            <strong class="status-{$health.status || 'unknown'}">{$health.status || 'Unknown'}</strong>
                        </div>
                        <div class="metric-card">
                            <span>{m.routes_settings_page_version()}</span>
                            <strong>{$health.version || '—'}</strong>
                        </div>
                        <div class="metric-card">
                            <span>{m.routes_settings_page_database()}</span>
                            <strong>{$health.database || '—'}</strong>
                        </div>
                        <div class="metric-card">
                            <span>{m.routes_settings_page_uptime()}</span>
                            <strong>
                                {$health.uptime_seconds
                                    ? Math.floor($health.uptime_seconds / 3600) + 'h ' +
                                      Math.floor(($health.uptime_seconds % 3600) / 60) + 'm'
                                    : '—'}
                            </strong>
                        </div>
                    </div>
                {:else}
                    <div class="state-copy error">{m.routes_settings_page_unable_to_fetch_server_status()}</div>
                {/if}
            </section>
        {/if}

        {#each visibleSections as section}
            <section class="admin-section" aria-labelledby={section.id}>
                <h2 id={section.id}>{section.title}</h2>
                <div class="admin-grid">
                    {#each section.items as item}
                        <a href={item.href} class="admin-card">
                            <div class="card-icon" aria-hidden="true">
                                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                                    <path d={item.icon} />
                                </svg>
                            </div>
                            <div>
                                <span class="card-title">{item.label}</span>
                                <span class="card-description">{item.desc}</span>
                            </div>
                        </a>
                    {/each}
                </div>
            </section>
        {/each}
    {/if}
</div>

<style>
    .admin-page {
        display: flex;
        flex-direction: column;
        gap: 2rem;
        max-width: 1120px;
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

    .back-link:hover,
    .section-heading a:hover {
        color: var(--color-accent);
    }

    .page-title {
        margin-top: 0.25rem;
        font-size: 1.5rem;
        font-weight: 700;
        color: var(--color-text-primary);
    }

    .page-description {
        margin-top: 0.25rem;
        color: var(--color-text-secondary);
    }

    .health-section,
    .admin-section {
        display: flex;
        flex-direction: column;
        gap: 0.875rem;
    }

    .section-heading {
        display: flex;
        align-items: baseline;
        justify-content: space-between;
        gap: 1rem;
    }

    .section-heading h2,
    .admin-section h2 {
        font-size: 0.75rem;
        font-weight: 600;
        letter-spacing: 0.05em;
        text-transform: uppercase;
        color: var(--color-text-secondary);
    }

    .section-heading a {
        font-size: 0.8125rem;
        color: var(--color-text-muted);
    }

    .metrics-grid,
    .admin-grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
        gap: 0.75rem;
    }

    .metric-card,
    .admin-card {
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
        background-color: var(--color-bg-surface);
    }

    .metric-card {
        display: flex;
        flex-direction: column;
        gap: 0.25rem;
        padding: 0.875rem 1rem;
    }

    .metric-card span {
        font-size: 0.6875rem;
        font-weight: 500;
        letter-spacing: 0.05em;
        text-transform: uppercase;
        color: var(--color-text-muted);
    }

    .metric-card strong {
        font-size: 0.9375rem;
        color: var(--color-text-primary);
    }

    .status-healthy {
        color: var(--color-success) !important;
    }

    .status-degraded {
        color: var(--color-warning) !important;
    }

    .admin-card {
        display: flex;
        align-items: center;
        gap: 0.75rem;
        padding: 0.875rem 1rem;
        transition: border-color var(--transition-fast), background-color var(--transition-fast);
    }

    .admin-card:hover {
        border-color: var(--color-accent);
        background-color: var(--color-bg-elevated);
    }

    .card-icon {
        display: grid;
        flex: 0 0 auto;
        width: 2.25rem;
        height: 2.25rem;
        place-items: center;
        border-radius: var(--radius-sm);
        background: var(--color-bg-elevated);
        color: var(--color-text-secondary);
    }

    .admin-card:hover .card-icon {
        color: var(--color-accent);
    }

    .card-title,
    .card-description {
        display: block;
    }

    .card-title {
        font-size: 0.875rem;
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .card-description {
        margin-top: 0.125rem;
        font-size: 0.75rem;
        color: var(--color-text-muted);
    }

    .state-copy,
    .empty-state {
        padding: 1rem;
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
        color: var(--color-text-secondary);
        background: var(--color-bg-surface);
    }

    .state-copy.error {
        color: var(--color-error);
    }

    @media (max-width: 640px) {
        .admin-page {
            gap: 1.5rem;
        }

        .metrics-grid,
        .admin-grid {
            grid-template-columns: 1fr;
        }
    }
</style>
