<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import { sendTestNotification } from '$lib/api/notifications.js';
    import { hasCapability } from '$lib/stores/auth.js';
    import { notifications } from '$lib/stores/notifications.js';

    let canManage = $state(false);
    let sending = $state(false);

    $effect(() => {
        const unsubscribe = hasCapability('can_manage_server').subscribe((value) => (canManage = value));
        return unsubscribe;
    });

    async function sendTest() {
        sending = true;
        try {
            const response = await sendTestNotification({});
            const channels = Object.entries(response?.delivery_status || {})
                .map(([channel, status]) => `${channel}: ${status}`)
                .join(', ');
            notifications.success(channels || m.routes_settings_notifications_page_test_notification());
        } catch (err) {
            notifications.error(err.detail || err.message || m.routes_settings_notifications_page_test_notification());
        } finally {
            sending = false;
        }
    }
</script>

<div class="admin-notifications">
    <header class="page-header">
        <div>
            <a href="/admin" class="back-link">{m.routes_admin_page_admin()}</a>
            <h1 class="page-title">{m.routes_settings_notifications_page_test_notification()}</h1>
            <p class="page-description">{m.routes_settings_notifications_page_send_a_test_notification_to_yourself_to_verify_t()}</p>
        </div>
    </header>

    {#if !canManage}
        <div class="empty-state">{m.routes_admin_page_no_admin_access()}</div>
    {:else}
        <section class="test-card" aria-labelledby="test-title">
            <h2 id="test-title">{m.routes_settings_notifications_page_test_notification()}</h2>
            <p>{m.routes_settings_notifications_page_send_a_test_notification_to_yourself_to_verify_t()}</p>
            <button class="btn-primary" onclick={sendTest} disabled={sending}>
                {m.routes_settings_notifications_page_test_notification()}
            </button>
        </section>
    {/if}
</div>

<style>
    .admin-notifications {
        display: flex;
        flex-direction: column;
        gap: 1.5rem;
        max-width: 720px;
    }

    .back-link {
        font-size: 0.8125rem;
        color: var(--color-text-muted);
    }

    .back-link:hover {
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

    .test-card,
    .empty-state {
        padding: 1.25rem;
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-md);
        background: var(--color-bg-surface);
    }

    .test-card h2 {
        font-size: 1rem;
        color: var(--color-text-primary);
    }

    .test-card p {
        margin: 0.5rem 0 1rem;
        font-size: 0.8125rem;
        line-height: 1.5;
        color: var(--color-text-secondary);
    }

    .btn-primary {
        padding: 0.5rem 1.25rem;
        border-radius: var(--radius-sm);
        font-size: 0.8125rem;
        font-weight: 600;
        color: var(--color-bg-deep);
        background: var(--color-accent);
    }

    .btn-primary:disabled {
        opacity: 0.5;
    }
</style>
