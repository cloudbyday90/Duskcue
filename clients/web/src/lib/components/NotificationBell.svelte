<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import { onMount, onDestroy } from 'svelte';
    import { fly, fade } from 'svelte/transition';
    import { flip } from 'svelte/animate';
    import { goto } from '$app/navigation';
    import {
        notificationCenter,
        unreadCount,
    } from '$lib/stores/notificationCenter.js';
    import { notifications as toastStore } from '$lib/stores/notifications.js';

    const CATEGORY_META = {
        security: { label: m.lib_components_notificationbell_security(), color: 'var(--color-error)', icon: 'M12 2l8 4v6c0 5-3.5 9-8 10-4.5-1-8-5-8-10V6z' },
        system: { label: m.lib_components_notificationbell_system(), color: 'var(--color-accent)', icon: 'M4 7h16M4 12h16M4 17h16' },
        media: { label: m.lib_components_notificationbell_media(), color: 'var(--color-success)', icon: 'M4 4h16v16H4zM2 8h20' },
        task: { label: m.lib_components_notificationbell_task(), color: 'var(--color-text-secondary)', icon: 'M9 11l3 3L22 4M21 12v7a2 2 0 01-2 2H5a2 2 0 01-2-2V5a2 2 0 012-2h11' },
        user: { label: m.lib_components_notificationbell_user(), color: 'var(--color-text-secondary)', icon: 'M17 21v-2a4 4 0 00-4-4H5a4 4 0 00-4 4v2M9 7a4 4 0 100 8 4 4 0 000-8' },
    };

    const PRIORITY_META = {
        high: { label: m.lib_components_notificationbell_high(), color: 'var(--color-error)' },
        medium: { label: m.lib_components_notificationbell_medium(), color: 'var(--color-warning)' },
        low: { label: m.lib_components_notificationbell_low(), color: 'var(--color-text-muted)' },
    };

    let open = $state(false);
    let containerEl;

    onMount(() => {
        notificationCenter.init();
    });

    onDestroy(() => {
        notificationCenter.shutdown();
    });

    function toggle() {
        open = !open;
    }

    function close() {
        open = false;
    }

    function handleKeydown(event) {
        if (event.key === 'Escape') close();
    }

    function formatRelative(iso) {
        if (!iso) return '';
        const then = new Date(iso).getTime();
        if (Number.isNaN(then)) return '';
        const diff = Date.now() - then;
        const sec = Math.floor(diff / 1000);
        if (sec < 60) return 'just now';
        const min = Math.floor(sec / 60);
        if (min < 60) return `${min}m ago`;
        const hr = Math.floor(min / 60);
        if (hr < 24) return `${hr}h ago`;
        const day = Math.floor(hr / 24);
        if (day < 7) return `${day}d ago`;
        return new Date(then).toLocaleDateString();
    }

    async function handleClickNotification(n) {
        if (!n.is_read) {
            try {
                await notificationCenter.markRead(n.id);
            } catch {
                // optimistic UI already updated; ignore network error
            }
        }
        if (n.link) {
            close();
            goto(n.link);
        }
    }

    async function handleDelete(event, n) {
        event.stopPropagation();
        try {
            await notificationCenter.remove(n.id);
        } catch (err) {
            toastStore.error(err.detail || err.message || 'Failed to delete notification');
        }
    }

    async function handleMarkAllRead() {
        try {
            const count = await notificationCenter.markAllRead();
            if (count > 0) {
                toastStore.success(`Marked ${count} notification${count === 1 ? '' : 's'} as read`);
            }
        } catch (err) {
            toastStore.error(err.detail || err.message || 'Failed to mark all as read');
        }
    }

    function viewAll() {
        close();
        goto('/settings/notifications');
    }

    let recent = $derived($notificationCenter.items.slice(0, 6));
    let hasUnread = $derived($unreadCount > 0);
    let loading = $derived($notificationCenter.loading && !$notificationCenter.initialized);
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="bell-container" bind:this={containerEl}>
    <button
        class="bell-button"
        class:active={open}
        onclick={toggle}
        aria-label={m.lib_components_notificationbell_notifications()}
        aria-expanded={open}
        title={m.lib_components_notificationbell_notifications()}
    >
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
            <path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9" />
            <path d="M13.73 21a2 2 0 0 1-3.46 0" />
        </svg>
        {#if hasUnread}
            <span class="bell-badge">{$unreadCount > 99 ? '99+' : $unreadCount}</span>
        {/if}
    </button>

    {#if open}
        <div
            class="bell-backdrop"
            role="button"
            tabindex="0"
            onclick={close}
            onkeydown={(e) => e.key === 'Escape' && close()}
            aria-label={m.lib_components_notificationbell_close_notifications()}
        ></div>
        <div class="bell-dropdown" transition:fly={{ y: -8, duration: 180 }}>
            <div class="dropdown-header">
                <span class="dropdown-title">{m.lib_components_notificationbell_notifications()}</span>
                <div class="header-actions">
                    {#if hasUnread}
                        <button class="header-action" onclick={handleMarkAllRead}>
                            Mark all read
                        </button>
                    {/if}
                </div>
            </div>

            <div class="dropdown-body">
                {#if loading}
                    <div class="dropdown-state">
                        <div class="mini-spinner"></div>
                        <span>{m.lib_components_notificationbell_loading()}</span>
                    </div>
                {:else if recent.length === 0}
                    <div class="dropdown-state empty">
                        <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round">
                            <path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9" />
                            <path d="M13.73 21a2 2 0 0 1-3.46 0" />
                        </svg>
                        <span>{m.lib_components_notificationbell_you_re_all_caught_up()}</span>
                    </div>
                {:else}
                    <div class="notif-list">
                        {#each recent as n (n.id)}
                            <div
                                class="notif-item"
                                class:unread={!n.is_read}
                                onclick={() => handleClickNotification(n)}
                                role="button"
                                tabindex="0"
                                onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && handleClickNotification(n)}
                                animate:flip={{ duration: 180 }}
                                in:fade={{ duration: 150 }}
                            >
                                <span class="notif-dot" style="--cat-color: {(CATEGORY_META[n.category] || CATEGORY_META.system).color}"></span>
                                <div class="notif-icon" style="--cat-color: {(CATEGORY_META[n.category] || CATEGORY_META.system).color}">
                                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                        <path d={(CATEGORY_META[n.category] || CATEGORY_META.system).icon} />
                                    </svg>
                                </div>
                                <div class="notif-body">
                                    {#if n.title}
                                        <p class="notif-title">{n.title}</p>
                                    {/if}
                                    <p class="notif-text">{n.body}</p>
                                    <div class="notif-meta">
                                        <span class="notif-category">{(CATEGORY_META[n.category] || CATEGORY_META.user).label}</span>
                                        <span class="meta-sep">·</span>
                                        <span class="notif-time">{formatRelative(n.created_at)}</span>
                                        {#if n.priority === 'high'}
                                            <span class="meta-sep">·</span>
                                            <span class="notif-priority" style="--prio-color: {(PRIORITY_META[n.priority] || PRIORITY_META.low).color}">
                                                {(PRIORITY_META[n.priority] || PRIORITY_META.low).label}
                                            </span>
                                        {/if}
                                    </div>
                                </div>
                                <button
                                    class="notif-delete"
                                    onclick={(e) => handleDelete(e, n)}
                                    aria-label={m.lib_components_notificationbell_delete_notification()}
                                    title={m.lib_components_notificationbell_delete()}
                                >
                                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                        <path d="M18 6L6 18M6 6l12 12" />
                                    </svg>
                                </button>
                            </div>
                        {/each}
                    </div>
                {/if}
            </div>

            <div class="dropdown-footer">
                <button class="footer-link" onclick={viewAll}>
                    View all
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <path d="M5 12h14M13 6l6 6-6 6" />
                    </svg>
                </button>
            </div>
        </div>
    {/if}
</div>

<style>
    .bell-container {
        position: relative;
        flex-shrink: 0;
    }

    .bell-button {
        position: relative;
        display: flex;
        align-items: center;
        justify-content: center;
        width: 36px;
        height: 36px;
        color: var(--color-text-secondary);
        border-radius: var(--radius-md);
        transition: color var(--transition-fast), background-color var(--transition-fast);
    }

    .bell-button:hover,
    .bell-button.active {
        background-color: var(--color-bg-hover);
        color: var(--color-text-primary);
    }

    .bell-badge {
        position: absolute;
        top: 2px;
        right: 2px;
        min-width: 16px;
        height: 16px;
        padding: 0 4px;
        background-color: var(--color-error);
        color: #fff;
        font-size: 0.5625rem;
        font-weight: 700;
        line-height: 16px;
        text-align: center;
        border-radius: 8px;
        border: 2px solid var(--color-bg-surface);
        box-sizing: content-box;
    }

    .bell-backdrop {
        position: fixed;
        inset: 0;
        z-index: 99;
        cursor: default;
    }

    .bell-dropdown {
        position: absolute;
        top: calc(100% + 6px);
        right: 0;
        width: 380px;
        max-width: calc(100vw - 2rem);
        background-color: var(--color-bg-elevated);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        box-shadow: var(--shadow-elevated);
        z-index: 100;
        overflow: hidden;
        display: flex;
        flex-direction: column;
    }

    .dropdown-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 0.75rem 1rem;
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .dropdown-title {
        font-size: 0.8125rem;
        font-weight: 700;
        color: var(--color-text-primary);
        text-transform: uppercase;
        letter-spacing: 0.04em;
    }

    .header-actions {
        display: flex;
        gap: 0.5rem;
    }

    .header-action {
        font-size: 0.75rem;
        color: var(--color-accent);
        padding: 0.25rem 0.5rem;
        border-radius: var(--radius-sm);
        transition: background-color var(--transition-fast);
    }

    .header-action:hover {
        background-color: var(--color-accent-muted);
    }

    .dropdown-body {
        max-height: 420px;
        overflow-y: auto;
    }

    .dropdown-state {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: 0.75rem;
        padding: 2.5rem 1rem;
        color: var(--color-text-muted);
        font-size: 0.8125rem;
    }

    .dropdown-state.empty svg {
        color: var(--color-text-muted);
        opacity: 0.6;
    }

    .mini-spinner {
        width: 22px;
        height: 22px;
        border: 2px solid var(--color-border);
        border-top-color: var(--color-accent);
        border-radius: 50%;
        animation: bell-spin 0.8s linear infinite;
    }

    @keyframes bell-spin {
        to { transform: rotate(360deg); }
    }

    .notif-list {
        list-style: none;
        display: flex;
        flex-direction: column;
    }

    .notif-item {
        position: relative;
        display: flex;
        align-items: flex-start;
        gap: 0.625rem;
        padding: 0.75rem 1rem 0.75rem 0.875rem;
        border-bottom: 1px solid var(--color-border-subtle);
        cursor: pointer;
        transition: background-color var(--transition-fast);
    }

    .notif-item:last-child {
        border-bottom: none;
    }

    .notif-item:hover {
        background-color: var(--color-bg-hover);
    }

    .notif-item:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
    }

    .notif-item.unread {
        background-color: var(--color-accent-muted);
    }

    .notif-item.unread:hover {
        background-color: rgba(200, 150, 90, 0.22);
    }

    .notif-dot {
        position: absolute;
        left: 0.25rem;
        top: 1.125rem;
        width: 6px;
        height: 6px;
        border-radius: 50%;
        background-color: var(--cat-color, var(--color-accent));
        opacity: 0;
        transition: opacity var(--transition-fast);
    }

    .notif-item.unread .notif-dot {
        opacity: 1;
    }

    .notif-icon {
        flex-shrink: 0;
        width: 28px;
        height: 28px;
        border-radius: var(--radius-sm);
        background-color: color-mix(in srgb, var(--cat-color, var(--color-accent)) 16%, transparent);
        color: var(--cat-color, var(--color-accent));
        display: flex;
        align-items: center;
        justify-content: center;
    }

    .notif-body {
        flex: 1;
        min-width: 0;
    }

    .notif-title {
        font-size: 0.8125rem;
        font-weight: 600;
        color: var(--color-text-primary);
        margin-bottom: 0.125rem;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .notif-text {
        font-size: 0.75rem;
        color: var(--color-text-secondary);
        line-height: 1.4;
        display: -webkit-box;
        -webkit-line-clamp: 2;
        line-clamp: 2;
        -webkit-box-orient: vertical;
        overflow: hidden;
    }

    .notif-meta {
        display: flex;
        align-items: center;
        gap: 0.375rem;
        margin-top: 0.25rem;
        font-size: 0.6875rem;
        color: var(--color-text-muted);
    }

    .meta-sep {
        opacity: 0.6;
    }

    .notif-priority {
        font-weight: 600;
        color: var(--prio-color, var(--color-text-muted));
    }

    .notif-delete {
        flex-shrink: 0;
        color: var(--color-text-muted);
        padding: 4px;
        border-radius: var(--radius-sm);
        opacity: 0;
        transition: opacity var(--transition-fast), color var(--transition-fast), background-color var(--transition-fast);
    }

    .notif-item:hover .notif-delete {
        opacity: 1;
    }

    .notif-delete:hover {
        color: var(--color-error);
        background-color: var(--color-error-bg);
    }

    .dropdown-footer {
        padding: 0.5rem;
        border-top: 1px solid var(--color-border-subtle);
        text-align: center;
    }

    .footer-link {
        display: inline-flex;
        align-items: center;
        gap: 0.375rem;
        padding: 0.375rem 0.75rem;
        font-size: 0.75rem;
        font-weight: 600;
        color: var(--color-accent);
        border-radius: var(--radius-sm);
        transition: background-color var(--transition-fast);
    }

    .footer-link:hover {
        background-color: var(--color-accent-muted);
    }

    @media (max-width: 768px) {
        .bell-dropdown {
            width: calc(100vw - 1.5rem);
            right: -0.5rem;
        }
    }
</style>
