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
    import { auth, isAuthenticated, currentUser, userHasAnyCapability } from '$lib/stores/auth.js';
    import { notifications } from '$lib/stores/notifications.js';
    import { events } from '$lib/stores/events.js';
    import { player } from '$lib/stores/player.js';
    import { getSetupStatus } from '$lib/api/auth.js';
    import { listProfiles, switchProfile } from '$lib/api/profiles.js';
    import { invalidateProfileScopedRequests } from '$lib/api/core.js';
    import { publishProfileScopeChange, startProfileScopeSync } from '$lib/profiles/scope.js';
    import { startDesktopBridge, stopDesktopBridge } from '$lib/desktop/tauri.js';
    import NotificationToast from '$lib/components/NotificationToast.svelte';
    import NotificationBell from '$lib/components/NotificationBell.svelte';
    import SearchBar from '$lib/components/SearchBar.svelte';

    let { children } = $props();

    let authChecked = $state(false);
    let setupRequired = $state(false);
    let userMenuOpen = $state(false);
    let mobileMenuOpen = $state(false);
    let profiles = $state([]);
    let switchingProfileId = $state(null);
    let rememberedProfileId = $state(null);
    let deviceCanRememberProfile = $state(false);
    let rememberProfileOnDevice = $state(false);
    let profileLoadUserId = $state(null);
    let profileSelectionRequired = $state(false);
    let profileScopeReady = $state(false);
    let profilesLoading = $state(false);
    let profileScopeRevision = $state(0);
    let activeProfile = $derived(profiles.find((profile) => profile.id === $currentUser?.active_profile_id));
    let canAccessAdmin = $derived(
        userHasAnyCapability($currentUser, ['can_manage_server', 'can_manage_users', 'can_manage_libraries']),
    );

    const AUTH_ROUTES = ['/auth/login', '/auth/setup', '/auth/link'];
    const REDIRECT_WHEN_AUTHENTICATED = ['/auth/login', '/auth/setup'];

    const navLinks = [
        { href: '/dashboard', label: m.routes_layout_home() },
        { href: '/libraries', label: m.routes_layout_libraries() },
    ];

    onMount(() => {
        auth.init();
        startDesktopBridge(goto);
        const stopProfileScopeSync = startProfileScopeSync(handleProfileScopeChange);
        getSetupStatus()
            .then((status) => {
                setupRequired = !!status?.setup_required;
            })
            .catch(() => {
                setupRequired = false;
            })
            .finally(() => {
                authChecked = true;
            });

        return () => {
            stopProfileScopeSync();
            stopDesktopBridge();
        };
    });

    $effect(() => {
        if (!authChecked) return;
        const path = $page.url.pathname;
        const isAuthRoute = AUTH_ROUTES.some((r) => path.startsWith(r));
        const redirectsWhenAuthenticated = REDIRECT_WHEN_AUTHENTICATED.some((r) => path.startsWith(r));
        const isDeviceLinkRoute = path.startsWith('/auth/link');
        const isSetupRoute = path.startsWith('/auth/setup');
        if (setupRequired && !$isAuthenticated && !isSetupRoute) {
            goto('/auth/setup');
        } else if (!setupRequired && !$isAuthenticated && isSetupRoute) {
            goto('/auth/login');
        } else if (!$isAuthenticated && isDeviceLinkRoute) {
            const returnTo = `${$page.url.pathname}${$page.url.search}`;
            goto(`/auth/login?return_to=${encodeURIComponent(returnTo)}`);
        } else if (!$isAuthenticated && !isAuthRoute) {
            goto('/auth/login');
        } else if ($isAuthenticated && redirectsWhenAuthenticated) {
            goto('/dashboard');
        }
    });

    $effect(() => {
        const userId = $currentUser?.id;
        if ($isAuthenticated && userId && profileLoadUserId !== userId) {
            profileLoadUserId = userId;
            profileScopeReady = false;
            loadProfiles();
        } else if (!$isAuthenticated) {
            resetProfileScope();
            clearProfileState();
        }
    });

    $effect(() => {
        if (!authChecked) return;
        if ($isAuthenticated && profileScopeReady) {
            events.connect();
            return () => events.disconnect();
        } else {
            events.disconnect();
        }
    });

    function toggleUserMenu() {
        userMenuOpen = !userMenuOpen;
    }

    async function loadProfiles() {
        profilesLoading = true;
        try {
            const response = await listProfiles();
            profiles = response?.items || [];
            const activeProfileId = response?.active_profile_id || $currentUser?.active_profile_id;
            profileSelectionRequired = !!response?.profile_selection_required;
            profileScopeReady = !profileSelectionRequired;
            rememberedProfileId = response?.remembered_profile_id || null;
            deviceCanRememberProfile = !!response?.device_can_remember_profile;
            rememberProfileOnDevice = rememberedProfileId === activeProfileId;
            if ($currentUser && (
                $currentUser.active_profile_id !== activeProfileId
                || !!$currentUser.profile_selection_required !== profileSelectionRequired
            )) {
                auth.setUser({
                    ...$currentUser,
                    active_profile_id: activeProfileId,
                    profile_selection_required: profileSelectionRequired,
                });
            }
        } catch {
            profiles = [];
            profileSelectionRequired = false;
            profileScopeReady = false;
            rememberedProfileId = null;
            deviceCanRememberProfile = false;
            rememberProfileOnDevice = false;
        } finally {
            profilesLoading = false;
        }
    }

    async function selectProfile(profile) {
        if ((profile.id === $currentUser?.active_profile_id && !profileSelectionRequired) || switchingProfileId) return;
        switchingProfileId = profile.id;
        try {
            if (!profileSelectionRequired) {
                await player.stop();
            }
            const response = await switchProfile(
                profile.id,
                profileSelectionRequired && deviceCanRememberProfile
                    ? { remember_on_device: rememberProfileOnDevice }
                    : {},
            );
            const activeProfile = response?.active_profile || profile;
            applyProfileResponse(response, activeProfile);
            resetProfileScope();
            profileScopeReady = true;
            closeUserMenu();
            publishProfileScopeChange({ userId: $currentUser?.id, profileId: activeProfile.id });
            events.connect();
            goto('/dashboard');
        } catch (err) {
            notifications.error(err.detail || err.message || 'Could not switch profiles');
        } finally {
            switchingProfileId = null;
        }
    }

    async function updateRememberedProfile() {
        const profile = activeProfile;
        if (!profile || switchingProfileId) return;

        switchingProfileId = profile.id;
        try {
            const response = await switchProfile(profile.id, {
                remember_on_device: rememberProfileOnDevice,
            });
            applyProfileResponse(response, profile);
        } catch (err) {
            rememberProfileOnDevice = rememberedProfileId === profile.id;
            notifications.error(err.detail || err.message || 'Could not update this device preference');
        } finally {
            switchingProfileId = null;
        }
    }

    function closeUserMenu() {
        userMenuOpen = false;
    }

    async function forgetProfileOnDevice() {
        if (!activeProfile || switchingProfileId) return;
        rememberProfileOnDevice = false;
        await updateRememberedProfile();
    }

    function applyProfileResponse(response, fallbackProfile) {
        const activeProfile = response?.active_profile || fallbackProfile;
        profileSelectionRequired = !!response?.profile_selection_required;
        profiles = profiles.map((item) => item.id === activeProfile.id ? activeProfile : item);
        rememberedProfileId = response?.remembered_profile_id || null;
        deviceCanRememberProfile = !!response?.device_can_remember_profile;
        rememberProfileOnDevice = rememberedProfileId === activeProfile.id;
        auth.setUser({
            ...$currentUser,
            active_profile_id: activeProfile.id,
            profile_selection_required: profileSelectionRequired,
        });
    }

    function resetProfileScope() {
        invalidateProfileScopedRequests();
        player.reset();
        events.disconnect();
        profileScopeRevision += 1;
    }

    function clearProfileState() {
        profiles = [];
        profileSelectionRequired = false;
        profileScopeReady = false;
        profilesLoading = false;
        rememberedProfileId = null;
        deviceCanRememberProfile = false;
        rememberProfileOnDevice = false;
        profileLoadUserId = null;
    }

    async function handleProfileScopeChange(event) {
        if (!$currentUser || event.user_id !== $currentUser.id) return;
        resetProfileScope();
        profileScopeReady = false;
        await loadProfiles();
        if (profileScopeReady) {
            events.connect();
        }
        goto('/dashboard');
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
                    {#if profileScopeReady}
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
                    {/if}

                    <div class="nav-user">
                        <button
                            class="user-button"
                            onclick={toggleUserMenu}
                            aria-label={m.routes_layout_user_menu()}
                            aria-expanded={userMenuOpen}
                        >
                            <span class="user-avatar">
                                {activeProfile?.name?.[0]?.toUpperCase() || $currentUser?.display_name?.[0]?.toUpperCase() || 'U'}
                            </span>
                            <span class="user-name">{activeProfile?.name || $currentUser?.display_name || 'User'}</span>
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
                                <div class="profile-picker" aria-label="Choose profile">
                                    <span class="profile-picker-title">Who’s watching?</span>
                                    <div class="profile-list">
                                        {#each profiles as profile}
                                            <button
                                                class="profile-option"
                                                class:active-profile={profile.id === $currentUser?.active_profile_id}
                                                onclick={() => selectProfile(profile)}
                                                disabled={switchingProfileId === profile.id}
                                            >
                                                <span class="profile-option-avatar">{profile.name?.[0]?.toUpperCase() || 'P'}</span>
                                                <span>{profile.name}</span>
                                                {#if profile.profile_type === 'kids'}<small>Kids</small>{/if}
                                            </button>
                                        {/each}
                                    </div>
                                    {#if deviceCanRememberProfile && activeProfile}
                                        {#if rememberedProfileId === activeProfile.id}
                                            <button class="forget-profile" onclick={forgetProfileOnDevice} disabled={!!switchingProfileId}>
                                                Forget this device
                                            </button>
                                        {:else}
                                            <label class="remember-profile">
                                                <input
                                                    type="checkbox"
                                                    bind:checked={rememberProfileOnDevice}
                                                    onchange={updateRememberedProfile}
                                                    disabled={!!switchingProfileId}
                                                />
                                                <span>Remember this profile on this device</span>
                                            </label>
                                        {/if}
                                    {/if}
                                    <a href="/settings/profiles" class="manage-profiles" onclick={closeUserMenu}>Manage profiles</a>
                                </div>
                                <a href="/settings" class="dropdown-item" onclick={closeUserMenu}>
                                    Settings
                                </a>
                                {#if canAccessAdmin}
                                    <a href="/admin" class="dropdown-item" onclick={closeUserMenu}>
                                        {m.routes_admin_page_admin()}
                                    </a>
                                {/if}
                                <button class="dropdown-item dropdown-danger" onclick={handleLogout}>
                                    Sign Out
                                </button>
                            </div>
                        {/if}
                    </div>

                    {#if profileScopeReady}
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
                <a href="/settings/profiles" class="drawer-link" onclick={closeMobileMenu}>
                    Profiles
                </a>
                {#if canAccessAdmin}
                    <a href="/admin" class="drawer-link" onclick={closeMobileMenu}>
                        {m.routes_admin_page_admin()}
                    </a>
                {/if}
                <button class="drawer-link drawer-danger" onclick={handleLogout}>
                    Sign Out
                </button>
            </div>
        {/if}

        <main class="main-content">
            {#if !$isAuthenticated || profileScopeReady}
                {#key profileScopeRevision}
                    {@render children()}
                {/key}
            {:else}
                <section class="profile-gate" aria-labelledby="profile-gate-title">
                    {#if profilesLoading}
                        <div class="loading-spinner"></div>
                        <p>Loading profiles…</p>
                    {:else if profiles.length > 0}
                        <span class="profile-gate-eyebrow">Duskcue</span>
                        <h1 id="profile-gate-title">Who’s watching?</h1>
                        <p>Choose a profile before viewing this device’s personalized rows and playback state.</p>
                        <div class="profile-gate-list">
                            {#each profiles as profile}
                                <button
                                    class="profile-gate-option"
                                    onclick={() => selectProfile(profile)}
                                    disabled={!!switchingProfileId}
                                >
                                    <span class="profile-gate-avatar">{profile.name?.[0]?.toUpperCase() || 'P'}</span>
                                    <span>{profile.name}</span>
                                    {#if profile.profile_type === 'kids'}<small>Kids</small>{/if}
                                </button>
                            {/each}
                        </div>
                        {#if deviceCanRememberProfile}
                            <label class="remember-profile profile-gate-remember">
                                <input type="checkbox" bind:checked={rememberProfileOnDevice} disabled={!!switchingProfileId} />
                                <span>Remember my choice on this device</span>
                            </label>
                        {/if}
                        <button class="profile-gate-signout" onclick={handleLogout}>Sign Out</button>
                    {:else}
                        <h1 id="profile-gate-title">Profiles are unavailable</h1>
                        <p>Reconnect to load profiles before continuing.</p>
                        <button class="profile-gate-retry" onclick={loadProfiles}>Try again</button>
                        <button class="profile-gate-signout" onclick={handleLogout}>Sign Out</button>
                    {/if}
                </section>
            {/if}
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
        margin-inline-start: auto;
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
        inset-inline-end: 0;
        min-width: 180px;
        background-color: var(--color-bg-elevated);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        box-shadow: var(--shadow-elevated);
        z-index: 100;
        overflow: hidden;
    }

    .profile-picker {
        padding: 0.75rem;
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .profile-picker-title {
        display: block;
        margin: 0 0 0.5rem;
        color: var(--color-text-muted);
        font-size: 0.7rem;
        font-weight: 700;
        letter-spacing: 0.06em;
        text-transform: uppercase;
    }

    .profile-list {
        display: grid;
        gap: 0.25rem;
    }

    .profile-option {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        width: 100%;
        border: 1px solid transparent;
        border-radius: var(--radius-sm);
        padding: 0.35rem;
        color: var(--color-text-secondary);
        text-align: start;
    }

    .profile-option:hover,
    .profile-option.active-profile {
        background-color: var(--color-bg-hover);
        color: var(--color-text-primary);
    }

    .profile-option.active-profile {
        border-color: var(--color-accent-muted);
    }

    .profile-option-avatar {
        display: grid;
        width: 26px;
        height: 26px;
        place-items: center;
        border-radius: 50%;
        background-color: var(--color-accent-muted);
        color: var(--color-accent);
        font-size: 0.75rem;
        font-weight: 700;
    }

    .profile-option small {
        margin-inline-start: auto;
        color: var(--color-text-muted);
        font-size: 0.7rem;
    }

    .manage-profiles {
        display: block;
        margin-top: 0.625rem;
        color: var(--color-accent);
        font-size: 0.8rem;
    }

    .remember-profile {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        margin-top: 0.75rem;
        color: var(--color-text-secondary);
        font-size: 0.75rem;
        cursor: pointer;
    }

    .remember-profile input {
        width: 1rem;
        height: 1rem;
        accent-color: var(--color-accent);
    }

    .forget-profile {
        width: 100%;
        margin-top: 0.75rem;
        color: var(--color-text-secondary);
        font-size: 0.75rem;
        text-align: start;
    }

    .forget-profile:hover {
        color: var(--color-error);
    }

    .dropdown-item {
        display: block;
        width: 100%;
        text-align: start;
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

    .profile-gate {
        width: min(100%, 520px);
        min-height: calc(100vh - 160px);
        display: grid;
        align-content: center;
        justify-items: stretch;
        gap: 1rem;
        margin: 0 auto;
        padding: 2rem;
        background: var(--color-bg-surface);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-lg);
        text-align: center;
    }

    .profile-gate h1 {
        margin: 0;
        color: var(--color-text-primary);
        font-size: 1.5rem;
    }

    .profile-gate p {
        margin: 0;
        color: var(--color-text-secondary);
        line-height: 1.5;
    }

    .profile-gate-eyebrow {
        color: var(--color-accent);
        font-size: 0.75rem;
        font-weight: 700;
        letter-spacing: 0.08em;
        text-transform: uppercase;
    }

    .profile-gate-list {
        display: grid;
        gap: 0.625rem;
    }

    .profile-gate-option {
        display: flex;
        align-items: center;
        gap: 0.75rem;
        width: 100%;
        padding: 0.75rem;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        background: var(--color-bg-elevated);
        color: var(--color-text-primary);
        font-weight: 600;
        text-align: start;
    }

    .profile-gate-option:hover:not(:disabled) {
        border-color: var(--color-accent);
        background: var(--color-bg-hover);
    }

    .profile-gate-option small {
        margin-inline-start: auto;
        color: var(--color-text-muted);
        font-size: 0.75rem;
    }

    .profile-gate-avatar {
        display: grid;
        width: 2.25rem;
        height: 2.25rem;
        place-items: center;
        border-radius: 50%;
        background: var(--color-accent-muted);
        color: var(--color-accent);
    }

    .profile-gate-remember {
        justify-content: center;
        margin-top: 0;
    }

    .profile-gate-signout,
    .profile-gate-retry {
        justify-self: center;
        color: var(--color-text-secondary);
        font-size: 0.8125rem;
    }

    .profile-gate-signout:hover {
        color: var(--color-error);
    }

    .profile-gate-retry {
        padding: 0.625rem 0.875rem;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-bg-elevated);
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
        inset-inline-end: 0;
        bottom: 0;
        width: 300px;
        max-width: 85vw;
        z-index: 150;
        background-color: var(--color-bg-surface);
        border-inline-start: 1px solid var(--color-border);
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
            transform: translateX(var(--drawer-closed-offset, 100%));
        }
        to {
            transform: translateX(0);
        }
    }

    :global([dir='rtl']) .mobile-drawer {
        --drawer-closed-offset: -100%;
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
        text-align: start;
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
