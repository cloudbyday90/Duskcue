<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import { fly, fade } from 'svelte/transition';
    import { flip } from 'svelte/animate';
    import { notifications } from '../stores/notifications.js';
    import { NOTIFICATION_ICONS } from '../utils/constants.js';

    const typeColors = {
        success: { accent: 'var(--color-success)', bg: 'var(--color-success-bg)' },
        error: { accent: 'var(--color-error)', bg: 'var(--color-error-bg)' },
        warning: { accent: 'var(--color-warning)', bg: 'var(--color-warning-bg)' },
        info: { accent: 'var(--color-text-secondary)', bg: 'var(--color-info-bg)' },
    };
</script>

<div class="toast-container" role="region" aria-label={m.lib_components_notificationtoast_notifications()} aria-live="polite">
    {#each $notifications as notification (notification.id)}
        <div
            class="toast toast-{notification.type}"
            style="--toast-accent: {typeColors[notification.type]?.accent || 'var(--color-text-secondary)'}; --toast-bg: {typeColors[notification.type]?.bg || 'var(--color-info-bg)'}"
            animate:flip={{ duration: 200 }}
            in:fly={{ y: -20, duration: 250 }}
            out:fade={{ duration: 150 }}
        >
            <div class="toast-icon">
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <path d={NOTIFICATION_ICONS[notification.type] || NOTIFICATION_ICONS.info} />
                </svg>
            </div>
            <div class="toast-content">
                {#if notification.title}
                    <p class="toast-title">{notification.title}</p>
                {/if}
                <p class="toast-message">{notification.message}</p>
            </div>
            {#if notification.dismissible}
                <button
                    class="toast-dismiss"
                    onclick={() => notifications.dismiss(notification.id)}
                    aria-label={m.lib_components_notificationtoast_dismiss_notification()}
                >
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <path d="M18 6L6 18M6 6l12 12" />
                    </svg>
                </button>
            {/if}
        </div>
    {/each}
</div>

<style>
    .toast-container {
        position: fixed;
        top: 1rem;
        inset-inline-end: 1rem;
        z-index: 9999;
        display: flex;
        flex-direction: column;
        gap: 0.5rem;
        pointer-events: none;
        max-width: 400px;
    }

    .toast {
        display: flex;
        align-items: flex-start;
        gap: 0.75rem;
        padding: 0.875rem 1rem;
        background-color: var(--color-bg-elevated);
        border: 1px solid var(--color-border);
        border-inline-start: 3px solid var(--toast-accent);
        border-radius: var(--radius-md);
        box-shadow: var(--shadow-elevated);
        pointer-events: auto;
        min-width: 280px;
    }

    .toast-icon {
        flex-shrink: 0;
        color: var(--toast-accent);
        padding-top: 1px;
    }

    .toast-content {
        flex: 1;
        min-width: 0;
    }

    .toast-title {
        font-size: 0.875rem;
        font-weight: 600;
        color: var(--color-text-primary);
        margin-bottom: 0.125rem;
    }

    .toast-message {
        font-size: 0.8125rem;
        color: var(--color-text-secondary);
        word-wrap: break-word;
    }

    .toast-dismiss {
        flex-shrink: 0;
        color: var(--color-text-muted);
        padding: 2px;
        border-radius: var(--radius-sm);
        transition: color var(--transition-fast), background-color var(--transition-fast);
    }

    .toast-dismiss:hover {
        color: var(--color-text-primary);
        background-color: var(--color-bg-hover);
    }

    @media (max-width: 480px) {
        .toast-container {
            top: 0.5rem;
            inset-inline: 0.5rem;
            max-width: none;
        }

        .toast {
            min-width: 0;
        }
    }
</style>
