<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import { deLocalizeUrl, localizeUrl } from '$lib/paraglide/runtime.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    import '../app.css';
    import { onMount, untrack, tick, setContext } from 'svelte';
    import { page } from '$app/stores';
    import { goto } from '$app/navigation';
    import { auth, isAuthenticated, currentUser, userHasAnyCapability } from '$lib/stores/auth.js';
    import { notifications } from '$lib/stores/notifications.js';
    import { events } from '$lib/stores/events.js';
    import { player } from '$lib/stores/player.js';
    import { viewingPreferences } from '$lib/stores/viewing-preferences.js';
    import { navigationGuard } from '$lib/navigation/guard.js';
    import { isAllowedDesktopRoute } from '$lib/navigation/routes.js';
    import { authReturnDestination, playbackSignInPath } from '$lib/navigation/auth-return.js';
    import { listProfiles, switchProfile, unlockParentProfile } from '$lib/api/profiles.js';
    import { invalidateProfileScopedRequests, getServerOrigin } from '$lib/api/core.js';
    import { createProfileScopeSync } from '$lib/profiles/scope.js';
    import { startDesktopBridge, stopDesktopBridge } from '$lib/desktop/tauri.js';
    import { createShellStartup } from '$lib/desktop/startup.js';
    import DesktopServerSelector from '$lib/components/DesktopServerSelector.svelte';
    import NotificationToast from '$lib/components/NotificationToast.svelte';
    import NotificationBell from '$lib/components/NotificationBell.svelte';
    import SearchBar from '$lib/components/SearchBar.svelte';
    import ProfileDisclosure from '$lib/components/ProfileDisclosure.svelte';
    import ProfilePicker from '$lib/components/ProfilePicker.svelte';
    import PlaybackReleaseNotice from '$lib/components/PlaybackReleaseNotice.svelte';

    let { children } = $props();

    let authChecked = $state(false);
    let setupRequired = $state(false);
    let desktopSession = $state(null);
    let desktopStartupError = $state('');
    let desktopConnecting = $state(false);
    let mounted = false;
    const startup = createShellStartup({ auth, onChange: (state) => {
        authChecked = state.ready;
        desktopSession = state.desktop;
        setupRequired = state.setupRequired;
        desktopStartupError = state.error ? m.tonight_desktop_server_load_failed() : '';
    } });
    let userMenuOpen = $state(false);
    let parentUnlockDialog = $state();
    let profileDisclosure = $state();
    let parentUnlockReturnFocus = null;
    let profileStatus = $state('');
    let profiles = $state([]);
    let switchingProfileId = $state(null);
    let profileTransitionPending = $state(false);
    let rememberedProfileId = $state(null);
    let deviceCanRememberProfile = $state(false);
    let rememberProfileOnDevice = $state(false);
    let profileLoadUserId = $state(null);
    let profileSelectionRequired = $state(false);
    let profileScopeReady = $state(false);
    let profileScopeNavigating = $state(false);
    let scopeNavigationRevision = 0;
    let profilesLoading = $state(false);
    let profilesRevision = 0;
    let profileScopeRevision = $state(0);
    let parentUnlockRequired = $state(false);
    let parentUnlockTarget = $state(null);
    let parentPin = $state('');
    let parentUnlockError = $state('');
    let parentUnlockErrorLocale = $state('en');
    let unlockingParent = $state(false);
    let activeProfile = $derived(profiles.find((profile) => profile.id === $currentUser?.active_profile_id));
    const { start: startProfileScopeSync, publish: publishProfileScopeChange } = createProfileScopeSync({ readContext: () => ({
        serverOrigin: getServerOrigin() || $page.url.origin,
        userId: authChecked && $isAuthenticated ? $currentUser?.id : null,
    }) });
    setContext('active-profile', () => activeProfile);
    setContext('refresh-profiles', () => loadProfiles());
    setContext('desktop-server-selection', { beforeSelect: beforeDesktopServerSelect, selected: desktopServerSelected, settled: desktopServerSettled });
    let canAccessAdmin = $derived(
        userHasAnyCapability($currentUser, ['can_manage_server', 'can_manage_users', 'can_manage_libraries']),
    );

    const AUTH_ROUTES = ['/auth/login', '/auth/setup', '/auth/link', '/settings/server'];
    const REDIRECT_WHEN_AUTHENTICATED = ['/auth/login', '/auth/setup'];
    let routePathname = $derived(deLocalizeUrl($page.url).pathname);
    let isPlaybackRoute = $derived(routePathname.startsWith('/play/'));

    let navLinks = $derived([
        { href: '/dashboard', label: m.routes_layout_home(), active: routePathname === '/dashboard' },
        { href: localizedDestination('/media?type=movie'), label: m.tonight_movies(), locale: messageLocale('tonight_movies'), active: routePathname === '/media' && $page.url.searchParams.get('type') === 'movie' },
        { href: localizedDestination('/media?type=series'), label: m.tonight_tv(), locale: messageLocale('tonight_tv'), active: routePathname === '/media' && $page.url.searchParams.get('type') === 'series' },
        { href: localizedDestination('/collections'), label: m.tonight_collections(), locale: messageLocale('tonight_collections'), active: routePathname.startsWith('/collections') },
    ]);

    $effect(() => {
        const ready = authChecked && $isAuthenticated;
        const userId = ready ? $currentUser?.id : null;
        const origin = desktopSession?.server?.origin || getServerOrigin() || $page.url.origin;
        untrack(() => player.setContext({ serverOrigin: origin, userId }));
    });

    $effect(() => {
        const dialog = parentUnlockDialog;
        if (parentUnlockTarget && dialog) {
            if (!dialog.open) dialog.showModal();
            return () => { if (dialog.open) dialog.close(); };
        }
    });

    onMount(() => {
        mounted = true;
        initializeShell();
        const stopProfileScopeSync = startProfileScopeSync(handleProfileScopeChange);

        return () => {
            mounted = false;
            startup.dispose();
            stopProfileScopeSync();
            stopDesktopBridge();
        };
    });

    async function initializeShell() {
        const state = await startup.start();
        if (mounted && state?.ready) startDesktopBridge(goto);
    }

    async function beforeDesktopServerSelect() {
        if (!await navigationGuard.requestNavigationTransition()) return false;
        desktopConnecting = true;
        profilesRevision += 1;
        await player.stop();
        invalidateProfileScopedRequests();
        viewingPreferences.invalidate();
        events.disconnect();
        return true;
    }

    async function desktopServerSelected(session) {
        desktopSession = session;
        authChecked = false;
        auth.resetForServerSelection();
        resetProfileScope();
        clearProfileState();
        await initializeShell();
        desktopConnecting = false;
        if (authChecked) await goto($isAuthenticated ? '/dashboard' : setupRequired ? '/auth/setup' : '/auth/login');
    }

    function desktopServerSettled(result) {
        if (result) return;
        desktopConnecting = false;
        const user = $currentUser;
        if (user?.id) loadProfiles();
        if (user?.id && user.active_profile_id) viewingPreferences.load(user.active_profile_id, { serverOrigin: getServerOrigin(), userId: user.id }).catch(() => {});
    }

    $effect(() => {
        if (!authChecked || desktopConnecting) return;
        const path = routePathname;
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
            goto(path.startsWith('/play/') ? playbackSignInPath(new URL(globalThis.location?.href || $page.url.href)) : '/auth/login');
        } else if ($isAuthenticated && redirectsWhenAuthenticated) {
            goto(path.startsWith('/auth/login') ? authReturnDestination($page.url.searchParams.get('return_to')) : '/dashboard');
        }
    });

    $effect(() => {
        if (!authChecked || desktopConnecting) return;
        const userId = $currentUser?.id;
        if ($isAuthenticated && userId && profileLoadUserId !== userId) {
            profileLoadUserId = userId;
            profileScopeReady = false;
            loadProfiles();
        } else if (!$isAuthenticated) {
            untrack(() => {
                resetProfileScope();
                clearProfileState();
            });
        }
    });

    $effect(() => {
        if (!authChecked || desktopConnecting) return;
        if ($isAuthenticated && profileScopeReady) {
            events.connect();
            return () => events.disconnect();
        } else {
            events.disconnect();
        }
    });

    $effect(() => {
        if (!authChecked || desktopConnecting) return;
        const profileId = $currentUser?.active_profile_id;
        const userId = $currentUser?.id;
        if ($isAuthenticated && profileScopeReady && profileId && userId) {
            untrack(() => viewingPreferences.load(profileId, { serverOrigin: getServerOrigin(), userId }).catch(() => {}));
        }
    });

    async function loadProfiles() {
        const revision = ++profilesRevision;
        const userId = $currentUser?.id;
        const origin = getServerOrigin();
        const current = () => mounted && revision === profilesRevision && userId === $currentUser?.id && origin === getServerOrigin() && !desktopConnecting;
        profilesLoading = true;
        try {
            const response = await listProfiles();
            if (!current()) return;
            profiles = response?.items || [];
            const activeProfileId = response?.active_profile_id || $currentUser?.active_profile_id;
            profileSelectionRequired = !!response?.profile_selection_required;
            profileScopeReady = !profileSelectionRequired;
            rememberedProfileId = response?.remembered_profile_id || null;
            deviceCanRememberProfile = !!response?.device_can_remember_profile;
            parentUnlockRequired = !!response?.parent_unlock_required;
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
            if (!current()) return;
            profiles = [];
            profileSelectionRequired = false;
            profileScopeReady = false;
            rememberedProfileId = null;
            deviceCanRememberProfile = false;
            rememberProfileOnDevice = false;
            parentUnlockRequired = false;
        } finally {
            if (current()) profilesLoading = false;
        }
    }

    async function selectProfile(profile) {
        if ((profile.id === $currentUser?.active_profile_id && !profileSelectionRequired) || switchingProfileId || profileTransitionPending) return;
        profileTransitionPending = true;
        try {
            if (!await navigationGuard.requestProfileTransition()) return;
            return await commitProfileSelection(profile);
        } finally {
            profileTransitionPending = false;
        }
    }

    async function commitProfileSelection(profile) {
        const canonical = deLocalizeUrl($page.url);
        const titleDestination = `${canonical.pathname}${canonical.search}`;
        const destination = canonical.pathname.startsWith('/media/')
            && isAllowedDesktopRoute(titleDestination) ? titleDestination : '/dashboard';
        const profileDestination = localizedDestination(destination);
        if (parentUnlockRequired && profile.profile_type === 'standard') {
            requestParentUnlock(profile);
            return false;
        }
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
            profileStatus = m.tonight_profile_changed();
            publishProfileScopeChange({ userId: $currentUser?.id, profileId: activeProfile.id });
            events.connect();
            await goto(profileDestination);
            return true;
        } catch (err) {
            if (err.title === 'PROFILE_012' && profile.profile_type === 'standard') {
                parentUnlockRequired = true;
                requestParentUnlock(profile, m.tonight_parent_unlock_expired(), messageLocale('tonight_parent_unlock_expired'));
            } else notifications.error(err.detail || err.message || 'Could not switch profiles');
            return false;
        } finally {
            switchingProfileId = null;
        }
    }

    function requestParentUnlock(profile, error = '', locale = 'en') {
        if (!parentUnlockTarget) parentUnlockReturnFocus = document.activeElement;
        parentUnlockTarget = profile;
        parentPin = '';
        parentUnlockError = error;
        parentUnlockErrorLocale = locale;
    }

    async function unlockParentAccess() {
        if (!parentUnlockTarget || unlockingParent) return;
        unlockingParent = true;
        parentUnlockError = '';
        try {
            await unlockParentProfile({ pin: parentPin });
            parentUnlockRequired = false;
            const target = parentUnlockTarget;
            parentPin = '';
            if (await selectProfile(target)) {
                parentUnlockTarget = null;
                await tick();
                document.getElementById('main-content')?.focus();
            }
        } catch (err) {
            parentUnlockError = err.detail || err.message || 'Could not unlock parent access';
            parentUnlockErrorLocale = 'en';
        } finally {
            unlockingParent = false;
            await tick();
            if (parentUnlockTarget) parentUnlockDialog?.querySelector('input')?.focus();
        }
    }

    function cancelParentUnlock() {
        if (unlockingParent) return;
        parentUnlockTarget = null;
        parentPin = '';
        parentUnlockError = '';
        tick().then(() => {
            if (parentUnlockReturnFocus?.isConnected) parentUnlockReturnFocus.focus();
            else profileDisclosure?.focusTrigger();
            parentUnlockReturnFocus = null;
        });
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
        parentUnlockRequired = !!response?.parent_unlock_required;
        auth.setUser({
            ...$currentUser,
            active_profile_id: activeProfile.id,
            profile_selection_required: profileSelectionRequired,
        });
    }

    function resetProfileScope({ preserveRelease = false } = {}) {
        invalidateProfileScopedRequests();
        viewingPreferences.invalidate();
        player.reset({ preserveRelease });
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
        parentUnlockRequired = false;
        parentUnlockTarget = null;
        parentPin = '';
        parentUnlockError = '';
    }

    async function handleProfileScopeChange(event) {
        if (!$currentUser || event.user_id !== $currentUser.id) return;
        const revision = ++scopeNavigationRevision;
        profileScopeNavigating = true;
        const titleDestination = `${$page.url.pathname}${$page.url.search}`;
        const destination = $page.url.pathname.startsWith('/media/') && isAllowedDesktopRoute(titleDestination)
            ? titleDestination : '/dashboard';
        try {
            try { await player.stop(); } catch {}
            if (!mounted || revision !== scopeNavigationRevision) return;
            resetProfileScope({ preserveRelease: true });
            profileScopeReady = false;
            await loadProfiles();
            if (!mounted || revision !== scopeNavigationRevision || event.user_id !== $currentUser?.id) return;
            await goto(destination);
        } finally {
            if (mounted && revision === scopeNavigationRevision) profileScopeNavigating = false;
        }
    }

    async function handleLogout() {
        if (profileTransitionPending || switchingProfileId) return;
        const userId = $currentUser?.id;
        const origin = getServerOrigin();
        profileTransitionPending = true;
        try {
            if (!await navigationGuard.requestProfileTransition()) return;
            try { await player.stop(); } catch {}
            if (userId !== $currentUser?.id || origin !== getServerOrigin()) return;
            closeUserMenu();
            await auth.logout();
            if (!$isAuthenticated) goto('/auth/login');
        } finally {
            profileTransitionPending = false;
        }
    }

    function handleSearch(query) {
        goto(localizedDestination(`/search?q=${encodeURIComponent(query)}`));
    }

    function localizedDestination(destination) {
        const localized = localizeUrl(new URL(destination, $page.url));
        return `${localized.pathname}${localized.search}${localized.hash}`;
    }
</script>

{#if desktopStartupError || desktopSession?.requiresServerSelection}
    <div class="desktop-server-startup">
        {#if desktopStartupError}
            <p lang={messageLocale('tonight_desktop_server_load_failed')} role="alert">{desktopStartupError}</p>
            <button lang={messageLocale('tonight_desktop_server_retry')} type="button" class="tonight-button" onclick={initializeShell}>{m.tonight_desktop_server_retry()}</button>
        {/if}
        <DesktopServerSelector server={desktopSession?.server} onbeforeselect={beforeDesktopServerSelect} onselected={desktopServerSelected} onsettled={desktopServerSettled} />
    </div>
{:else if authChecked && ($isAuthenticated || AUTH_ROUTES.some((r) => routePathname.startsWith(r)))}
    <div class="app-shell">
        <a lang={messageLocale('tonight_skip_content')} class="skip-link" href="#main-content" hidden={isPlaybackRoute} inert={isPlaybackRoute}>{m.tonight_skip_content()}</a>
        <header class="nav-bar" hidden={isPlaybackRoute} inert={isPlaybackRoute}>
            <nav class="nav-content" aria-label={m.routes_layout_mobile_navigation()}>
                <a href="/dashboard" class="nav-logo">{m.routes_layout_duskcue()}</a>
                {#if $isAuthenticated && profileScopeReady}
                    <ul class="nav-links">
                        {#each navLinks as link}
                            <li><a lang={link.locale} href={link.href} class="nav-link" class:active={link.active} aria-current={link.active ? 'page' : undefined}>{link.label}</a></li>
                        {/each}
                    </ul>
                    <div class="nav-search"><SearchBar compact onsearch={handleSearch} navigate={false} /></div>
                    <div class="nav-account">
                        <NotificationBell />
                        <ProfileDisclosure bind:this={profileDisclosure} bind:open={userMenuOpen} {profiles} {activeProfile} {switchingProfileId}
                            {deviceCanRememberProfile} {rememberedProfileId} bind:rememberProfileOnDevice
                            {canAccessAdmin} onselect={selectProfile} onremember={updateRememberedProfile}
                            onforget={forgetProfileOnDevice} onlogout={handleLogout} />
                    </div>
                {/if}
            </nav>
        </header>
        <div lang={messageLocale('tonight_profile_changed')} class="visually-hidden" role="status">{profileStatus}</div>
        {#if !isPlaybackRoute || !$player.sessionId}<PlaybackReleaseNotice />{/if}
        <main id="main-content" class="main-content" tabindex="-1">
            {#if profileScopeNavigating}
                <p lang={messageLocale('tonight_loading')} role="status">{m.tonight_loading()}</p>
            {:else if !$isAuthenticated || profileScopeReady}
                {#key profileScopeRevision}{@render children()}{/key}
            {:else}
                <ProfilePicker {profiles} loading={profilesLoading} {switchingProfileId} {deviceCanRememberProfile}
                    bind:rememberProfileOnDevice onselect={selectProfile} onreload={loadProfiles} onlogout={handleLogout} />
            {/if}
        </main>
        {#if parentUnlockTarget}
            <dialog lang={messageLocale('tonight_enter_parent_pin')} bind:this={parentUnlockDialog} class="parent-unlock-dialog" aria-labelledby="parent-unlock-title"
                oncancel={(event) => { event.preventDefault(); cancelParentUnlock(); }}>
                <span lang={messageLocale('tonight_parent_access')} class="parent-unlock-eyebrow">{m.tonight_parent_access()}</span>
                <h2 lang={messageLocale('tonight_enter_parent_pin')} id="parent-unlock-title">{m.tonight_enter_parent_pin()}</h2>
                <p lang={messageLocale('tonight_parent_unlock_description')}>{m.tonight_parent_unlock_description()}</p>
                <form onsubmit={(event) => { event.preventDefault(); unlockParentAccess(); }}>
                    <label lang={messageLocale('tonight_parent_pin')} for="parent-pin">{m.tonight_parent_pin()}</label>
                    <input lang={messageLocale('tonight_parent_pin')} id="parent-pin" type="password" inputmode="numeric" pattern="[0-9]*" minlength="4" maxlength="12"
                        required bind:value={parentPin} autocomplete="off" disabled={unlockingParent} aria-describedby={parentUnlockError ? 'parent-pin-error' : undefined} />
                    <p lang={parentUnlockErrorLocale} id="parent-pin-error" class="parent-unlock-error" role="status">{parentUnlockError}</p>
                    <div class="parent-unlock-actions">
                        <button lang={messageLocale('tonight_cancel')} type="button" class="secondary-action" onclick={cancelParentUnlock} disabled={unlockingParent}>{m.tonight_cancel()}</button>
                        <button lang={unlockingParent ? messageLocale('tonight_unlocking') : messageLocale('tonight_unlock')} type="submit" class="primary-action" disabled={unlockingParent || parentPin.length < 4}>{unlockingParent ? m.tonight_unlocking() : m.tonight_unlock()}</button>
                    </div>
                </form>
            </dialog>
        {/if}
    </div>
    <NotificationToast />
{:else}
    <div class="app-loading" role="status"><p lang={messageLocale('tonight_loading')}>{m.tonight_loading()}</p></div>
{/if}

<style>
    .desktop-server-startup { max-width: 42rem; margin-inline: auto; padding: 3rem var(--space-page); display: grid; gap: 1.25rem; }
    .app-shell { min-height: 100vh; display: flex; flex-direction: column; }
    .skip-link { position: fixed; top: 0.5rem; inset-inline-start: 1rem; transform: translateY(-150%); padding: 0.75rem 1rem; background: var(--color-accent); color: var(--color-on-accent); z-index: 500; }
    .skip-link:focus { transform: none; }
    .nav-bar { position: sticky; top: 0; z-index: 100; background: var(--color-bg-deep); border-bottom: 1px solid var(--color-border-subtle); }
    .nav-content { max-width: 1600px; margin: auto; display: flex; align-items: center; gap: 1.25rem; padding: 0.7rem var(--space-page); min-height: 76px; }
    .nav-logo { font-family: var(--font-display); font-size: 1.65rem; letter-spacing: -0.04em; }
    .nav-links { display: flex; list-style: none; gap: 0.25rem; }
    .nav-link { display: flex; align-items: center; min-height: 44px; padding: 0.5rem 0.75rem; border-radius: var(--radius-sm); color: var(--color-text-secondary); font-size: 0.9rem; }
    .nav-link:hover { background: var(--color-bg-hover); color: var(--color-text-primary); }
    .nav-link.active { color: var(--color-accent); background: var(--color-accent-muted); }
    .nav-search { flex: 1; max-width: 26rem; min-width: 9rem; margin-inline-start: auto; }
    .nav-account { display: flex; align-items: center; gap: 0.5rem; }
    .main-content { width: 100%; max-width: 1600px; margin: auto; flex: 1; padding: 2rem var(--space-page) 4rem; }
    .parent-unlock-dialog { width: min(92vw, 430px); max-height: 90dvh; overflow: auto; margin: auto; padding: 1.5rem; border: 1px solid var(--color-border); border-radius: var(--radius-lg); background: var(--color-bg-surface); color: var(--color-text-primary); }
    .parent-unlock-dialog[open] { display: grid; gap: 1rem; }
    .parent-unlock-dialog::backdrop { background: rgb(0 0 0 / 70%); }
    .parent-unlock-dialog h2 { font-family: var(--font-display); font-weight: 400; font-size: 1.7rem; }
    .parent-unlock-dialog p { color: var(--color-text-secondary); }
    .parent-unlock-eyebrow { color: var(--color-accent); font-size: 0.8rem; }
    .parent-unlock-dialog form { display: grid; gap: 0.65rem; }
    .parent-unlock-dialog input { min-height: 44px; border: 1px solid var(--color-border); border-radius: var(--radius-sm); padding: 0.65rem; background: var(--color-bg-deep); }
    .parent-unlock-error { color: var(--color-error) !important; }
    .parent-unlock-actions { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 0.75rem; }
    .app-loading { display: grid; place-items: center; min-height: 100vh; }
    @media (max-width: 1100px) {
        .nav-content { flex-wrap: wrap; gap: 0.5rem 1rem; }
        .nav-account { margin-inline-start: auto; }
        .nav-links { order: 3; flex: 1; }
        .nav-search { order: 4; max-width: none; flex: 1; }
    }
    @media (max-width: 600px) {
        .nav-links { flex-basis: 100%; justify-content: space-between; gap: 0; }
        .nav-link { padding-inline: 0.55rem; font-size: 0.85rem; }
        .nav-search { flex-basis: 100%; min-width: 0; }
        .main-content { padding-top: 1.5rem; }
    }
    @media (max-height: 420px) { .nav-bar { position: static; } }
</style>
