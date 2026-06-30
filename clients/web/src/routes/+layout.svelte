<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import '../app.css';
    import { onMount } from 'svelte';
    import { page } from '$app/stores';
    import { goto } from '$app/navigation';
    import { auth, isAuthenticated, currentUser } from '$lib/stores/auth.js';
    import { notifications } from '$lib/stores/notifications.js';
    import { events } from '$lib/stores/events.js';
    import NotificationToast from '$lib/components/NotificationToast.svelte';
    import NotificationBell from '$lib/components/NotificationBell.svelte';
    import SearchBar from '$lib/components/SearchBar.svelte';

    let { children } = $props();

    let authChecked = $state(false);
    let userMenuOpen = $state(false);
    let mobileMenuOpen = $state(false);

    const AUTH_ROUTES = ['/auth/login', '/auth/setup'];

    const navLinks = [
        { href: '/dashboard', label: m.routes_layout_home() },
        { href: '/libraries', label: m.routes_layout_libraries() },
    ];

    onMount(() => {
        auth.init();
        authChecked = true;
    });

    $effect(() => {
        if (!authChecked) return;
        const path = $page.url.pathname;
        const isAuthRoute = AUTH_ROUTES.some((r) => path.startsWith(r));
        if (!$isAuthenticated && !isAuthRoute) {
            goto('/auth/login');
        } else if ($isAuthenticated && isAuthRoute) {
            goto('/dashboard');
        }
    });

    $effect(() => {
        if (!authChecked) return;
        if ($isAuthenticated) {
            events.connect();
            return () => events.disconnect();
        } else {
            events.disconnect();
        }
    });

    function toggleUserMenu() {
        userMenuOpen = !userMenuOpen;
    }

    function closeUserMenu() {
        userMenuOpen = false;
    }

    function toggleMobileMenu() {
        mobileMenuOpen = !mobileMenuOpen;
    }

    function closeMobileMenu() {
        mobileMenuOpen = false;
    }

    async function handleLogout() {
        closeUserMenu();
        closeMobileMenu();
        await auth.logout();
        goto('/auth/login');
    }

    function handleSearch(query) {
        closeMobileMenu();
        goto(`/search?q=${encodeURIComponent(query)}`);
    }
</script>

{#if authChecked && ($isAuthenticated || AUTH_ROUTES.some((r) => $page.url.pathname.startsWith(r)))}
    <div class="app-shell">
        <header class="nav-bar">
            <nav class="nav-content">
                <a href="/dashboard" class="nav-logo">{m.routes_layout_duskcue()}</a>

                {#if $isAuthenticated}
                    <ul class="nav-links">
                        {#each navLinks as link}
                            <li>
                                <a
                                    href={link.href}
                                    class="nav-link"
                                    class:active={$page.url.pathname.startsWith(link.href)}
                                >
                                    {link.label}
                                </a>
                            </li>
                        {/each}
                    </ul>

                    <div class="nav-search">
                        <SearchBar compact onsearch={handleSearch} navigate={false} />
                    </div>

                    <NotificationBell />

                    <div class="nav-user">
                        <button
                            class="user-button"
                            onclick={toggleUserMenu}
                            aria-label={m.routes_layout_user_menu()}
                            aria-expanded={userMenuOpen}
                        >
                            <span class="user-avatar">
                                {$currentUser?.display_name?.[0]?.toUpperCase() || 'U'}
                            </span>
                            <span class="user-name">{$currentUser?.display_name || 'User'}</span>
                        </button>

                        {#if userMenuOpen}
                            <div
                                class="menu-backdrop"
                                role="button"
                                tabindex="0"
                                onclick={closeUserMenu}
                                onkeydown={(e) => e.key === 'Escape' && closeUserMenu()}
                                aria-label={m.routes_layout_close_menu()}
                            ></div>
                            <div class="user-dropdown">
                                <a href="/settings" class="dropdown-item" onclick={closeUserMenu}>
                                    Settings
                                </a>
                                <button class="dropdown-item dropdown-danger" onclick={handleLogout}>
                                    Sign Out
                                </button>
                            </div>
                        {/if}
                    </div>

                    <button
                        class="menu-toggle"
                        onclick={toggleMobileMenu}
                        aria-label={m.routes_layout_open_menu()}
                        aria-expanded={mobileMenuOpen}
                    >
                        <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                            {#if mobileMenuOpen}
                                <path d="M18 6L6 18M6 6l12 12" />
                            {:else}
                                <path d="M3 12h18M3 6h18M3 18h18" />
                            {/if}
                        </svg>
                    </button>
                {/if}
            </nav>
        </header>

        {#if mobileMenuOpen}
            <div
                class="mobile-backdrop"
                role="button"
                tabindex="0"
                onclick={closeMobileMenu}
                onkeydown={(e) => e.key === 'Escape' && closeMobileMenu()}
                aria-label={m.routes_layout_close_menu()}
            ></div>
            <div class="mobile-drawer" role="navigation" aria-label={m.routes_layout_mobile_navigation()}>
                <div class="drawer-search">
                    <SearchBar onsearch={handleSearch} navigate={false} />
                </div>

                <ul class="drawer-links">
                    {#each navLinks as link}
                        <li>
                            <a
                                href={link.href}
                                class="drawer-link"
                                class:active={$page.url.pathname.startsWith(link.href)}
                                onclick={closeMobileMenu}
                            >
                                {link.label}
                            </a>
                        </li>
                    {/each}
                </ul>

                <div class="drawer-divider"></div>

                <a href="/settings/notifications" class="drawer-link" onclick={closeMobileMenu}>
                    Notifications
                </a>
                <a href="/settings" class="drawer-link" onclick={closeMobileMenu}>
                    Settings
                </a>
                <button class="drawer-link drawer-danger" onclick={handleLogout}>
                    Sign Out
                </button>
            </div>
        {/if}

        <main class="main-content">
            {@render children()}
        </main>
    </div>

    <NotificationToast />
{:else}
    <div class="app-loading">
        <div class="loading-spinner"></div>
    </div>
{/if}

<style>
    .app-shell {
        min-height: 100vh;
        display: flex;
        flex-direction: column;
    }

    .nav-bar {
        position: sticky;
        top: 0;
        z-index: 100;
        background-color: var(--color-bg-surface);
        border-bottom: 1px solid var(--color-border-subtle);
        backdrop-filter: blur(12px);
    }

    .nav-content {
        max-width: 1600px;
        margin: 0 auto;
        display: flex;
        align-items: center;
        gap: 1.5rem;
        padding: 0 1.5rem;
        height: 56px;
    }

    .nav-logo {
        font-size: 1.25rem;
        font-weight: 700;
        letter-spacing: -0.02em;
        color: var(--color-accent);
        flex-shrink: 0;
    }

    .nav-links {
        display: flex;
        list-style: none;
        gap: 0.25rem;
    }

    .nav-link {
        display: block;
        padding: 0.375rem 0.75rem;
        font-size: 0.875rem;
        font-weight: 500;
        color: var(--color-text-secondary);
        border-radius: var(--radius-sm);
        transition: color var(--transition-fast), background-color var(--transition-fast);
    }

    .nav-link:hover {
        color: var(--color-text-primary);
        background-color: var(--color-bg-hover);
    }

    .nav-link.active {
        color: var(--color-accent);
    }

    .nav-search {
        margin-left: auto;
    }

    .nav-user {
        position: relative;
        flex-shrink: 0;
    }

    .user-button {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        padding: 0.25rem 0.5rem;
        border-radius: var(--radius-md);
        transition: background-color var(--transition-fast);
    }

    .user-button:hover {
        background-color: var(--color-bg-hover);
    }

    .user-avatar {
        width: 28px;
        height: 28px;
        border-radius: 50%;
        background-color: var(--color-accent);
        color: var(--color-bg-deep);
        font-size: 0.75rem;
        font-weight: 700;
        display: flex;
        align-items: center;
        justify-content: center;
    }

    .user-name {
        font-size: 0.8125rem;
        color: var(--color-text-secondary);
        max-width: 120px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .menu-backdrop {
        position: fixed;
        inset: 0;
        z-index: 99;
        cursor: default;
    }

    .user-dropdown {
        position: absolute;
        top: calc(100% + 4px);
        right: 0;
        min-width: 180px;
        background-color: var(--color-bg-elevated);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        box-shadow: var(--shadow-elevated);
        z-index: 100;
        overflow: hidden;
    }

    .dropdown-item {
        display: block;
        width: 100%;
        text-align: left;
        padding: 0.625rem 1rem;
        font-size: 0.875rem;
        color: var(--color-text-secondary);
        transition: color var(--transition-fast), background-color var(--transition-fast);
    }

    .dropdown-item:hover {
        color: var(--color-text-primary);
        background-color: var(--color-bg-hover);
    }

    .dropdown-danger:hover {
        color: var(--color-error);
    }

    .main-content {
        flex: 1;
        max-width: 1600px;
        width: 100%;
        margin: 0 auto;
        padding: 1.5rem;
    }

    .menu-toggle {
        display: none;
        align-items: center;
        justify-content: center;
        width: 40px;
        height: 40px;
        color: var(--color-text-secondary);
        border-radius: var(--radius-md);
        transition: background-color var(--transition-fast);
    }

    .menu-toggle:hover {
        background-color: var(--color-bg-hover);
        color: var(--color-text-primary);
    }

    .mobile-backdrop {
        position: fixed;
        inset: 0;
        z-index: 149;
        background-color: rgba(0, 0, 0, 0.5);
        backdrop-filter: blur(2px);
        cursor: default;
    }

    .mobile-drawer {
        position: fixed;
        top: 0;
        right: 0;
        bottom: 0;
        width: 300px;
        max-width: 85vw;
        z-index: 150;
        background-color: var(--color-bg-surface);
        border-left: 1px solid var(--color-border);
        box-shadow: var(--shadow-elevated);
        display: flex;
        flex-direction: column;
        gap: 0.5rem;
        padding: 1.25rem;
        overflow-y: auto;
        animation: drawer-slide-in 0.2s ease-out;
    }

    @keyframes drawer-slide-in {
        from {
            transform: translateX(100%);
        }
        to {
            transform: translateX(0);
        }
    }

    .drawer-search {
        margin-bottom: 0.75rem;
    }

    .drawer-links {
        list-style: none;
        display: flex;
        flex-direction: column;
        gap: 0.125rem;
    }

    .drawer-link {
        display: block;
        padding: 0.75rem 1rem;
        font-size: 0.9375rem;
        font-weight: 500;
        color: var(--color-text-secondary);
        border-radius: var(--radius-sm);
        transition: color var(--transition-fast), background-color var(--transition-fast);
        text-align: left;
        width: 100%;
    }

    .drawer-link:hover {
        color: var(--color-text-primary);
        background-color: var(--color-bg-hover);
    }

    .drawer-link.active {
        color: var(--color-accent);
        background-color: var(--color-accent-muted);
    }

    .drawer-danger {
        color: var(--color-text-secondary);
    }

    .drawer-danger:hover {
        color: var(--color-error);
    }

    .drawer-divider {
        height: 1px;
        background-color: var(--color-border);
        margin: 0.5rem 0;
    }

    .app-loading {
        display: flex;
        align-items: center;
        justify-content: center;
        min-height: 100vh;
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
        .nav-content {
            padding: 0 1rem;
            gap: 0.75rem;
        }

        .nav-links {
            display: none;
        }

        .nav-search {
            display: none;
        }

        .user-name {
            display: none;
        }

        .user-button {
            padding: 0.25rem;
        }

        .menu-toggle {
            display: flex;
        }

        .main-content {
            padding: 1rem;
        }
    }

    @media (max-width: 480px) {
        .main-content {
            padding: 0.75rem;
        }
    }
</style>
