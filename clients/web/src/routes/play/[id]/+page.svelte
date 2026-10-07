<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    import { localizeUrl } from '$lib/paraglide/runtime.js';
    import { onMount, onDestroy } from 'svelte';
    import { page } from '$app/stores';
    import { beforeNavigate, goto, replaceState } from '$app/navigation';
    import { player } from '$lib/stores/player.js';
    import { currentUser } from '$lib/stores/auth.js';
    import { getServerOrigin } from '$lib/api/core.js';
    import { isAllowedDesktopRoute } from '$lib/navigation/routes.js';
    import { loadPlaybackEntry } from '$lib/playback/entry.js';
    import Player from '$lib/components/Player.svelte';

    let loading = $state(true);
    let entry = $state(null);
    let error = $state('');
    let errorLocale = $state('en');
    let controller;
    let closing = false;
    let activeDestination;
    let activePlaybackHref;
    let playerView = $state();
    let pendingNavigation = null;
    let approvedNavigation = false;
    let alive = true;
    let clearRestoration = null;
    let closeError = $state(false);
    const suppliedDestination = $page.url.searchParams.get('return_to') || '';
    const fallbackDestination = isAllowedDesktopRoute(suppliedDestination) && suppliedDestination.startsWith('/media/')
        ? suppliedDestination : `/media/${encodeURIComponent($page.params.id)}`;

    async function load() {
        controller?.abort();
        controller = new AbortController();
        const request = controller;
        loading = true;
        error = '';
        try {
            const result = await loadPlaybackEntry($page.params.id, {
                fileId: $page.url.searchParams.get('file') || '',
                returnTo: suppliedDestination,
                signal: request.signal,
            });
            if (!request.signal.aborted) entry = result;
        } catch (failure) {
            if (!request.signal.aborted) {
                error = failure.code === 'FILES_UNAVAILABLE' ? m.routes_play_id_page_no_playable_files_found() : failure.detail || m.routes_play_id_page_failed_to_load_media();
                errorLocale = failure.code === 'FILES_UNAVAILABLE' ? messageLocale('routes_play_id_page_no_playable_files_found') : failure.detail ? 'en' : messageLocale('routes_play_id_page_failed_to_load_media');
            }
        } finally {
            if (!request.signal.aborted) loading = false;
        }
    }

    onMount(() => {
        activePlaybackHref = window.location.href;
        load();
    });
    beforeNavigate((navigation) => {
        if (approvedNavigation || !playerView || navigation.to?.url.pathname.startsWith('/auth/')) return;
        if (navigation.willUnload) {
            controller?.abort();
            playerView.requestClose().catch(() => {});
            return;
        }
        if (!$player.sessionId && !$player.loading && $player.release?.phase === 'idle') return;
        const destination = navigation.to?.url;
        if (!destination || destination.origin !== $page.url.origin) return;
        navigation.cancel();
        if (pendingNavigation) return;
        const restore = navigation.type === 'popstate' ? waitForRestoredHistory(activePlaybackHref || $page.url.href) : Promise.resolve();
        pendingNavigation = { destination: `${destination.pathname}${destination.search}${destination.hash}`, delta: navigation.type === 'popstate' ? navigation.delta : null, restore };
        playerView.requestClose().then((released) => { if (released && alive) handleStop(); }).catch(() => {});
    });
    onDestroy(() => {
        alive = false;
        clearRestoration?.();
        controller?.abort();
        player.stop().catch(() => {});
    });

    function waitForRestoredHistory(originalUrl) {
        return new Promise((resolve) => {
            const restored = () => {
                if (window.location.href !== originalUrl) return;
                window.removeEventListener('popstate', restored);
                clearRestoration = null;
                resolve(undefined);
            };
            clearRestoration = () => { window.removeEventListener('popstate', restored); resolve(undefined); };
            window.addEventListener('popstate', restored);
            restored();
        });
    }

    function titleExitIntent() {
        clearRestoration?.();
        clearRestoration = null;
        pendingNavigation = null;
    }

    function handleActiveChange(state) {
        activeDestination = state.destination;
        const query = new URLSearchParams({ file: state.file.id, return_to: state.destination });
        const playbackUrl = localizeUrl(new URL(`/play/${encodeURIComponent(state.item.id)}?${query}`, $page.url));
        replaceState(`${playbackUrl.pathname}${playbackUrl.search}${playbackUrl.hash}`, $page.state);
        activePlaybackHref = window.location.href;
    }

    async function handleStop(destination = null) {
        if (closing) return;
        closing = true;
        closeError = false;
        try {
            await player.stop();
            const requested = pendingNavigation;
            if (requested) await requested.restore;
            if (!alive) return;
            approvedNavigation = true;
            if (requested?.delta) {
                history.go(requested.delta);
            } else {
                const safeDestination = typeof destination === 'string' && destination.startsWith('/media/') && isAllowedDesktopRoute(destination) ? destination : null;
                await goto(requested?.destination || safeDestination || activeDestination || entry?.destination || fallbackDestination);
            }
        } catch {
            approvedNavigation = false;
            closeError = true;
        } finally { closing = false; }
    }

    async function retryRouteClose() {
        try { await player.stop({ retry: true }); await handleStop(); }
        catch { closeError = true; }
    }
</script>

<div class="player-route">
    {#if loading}
        <div class="loading-state" role="status">
            <div class="loading-spinner" aria-hidden="true"></div>
            <p>{m.routes_play_id_page_loading_player()}</p>
        </div>
    {:else if error}
        <div class="loading-state">
            <p lang={errorLocale} role="alert">{error}</p>
            <button type="button" class="tonight-button" onclick={load}>{m.lib_components_player_retry()}</button>
            <button type="button" class="tonight-button secondary" onclick={handleStop}>{m.lib_components_player_close_player()}</button>
            {#if closeError}
                <p lang={messageLocale('tonight_player_close_failed')} role="status">{m.tonight_player_close_failed()}</p>
                <button lang={messageLocale('tonight_player_retry_close')} type="button" class="tonight-button" onclick={retryRouteClose}>{m.tonight_player_retry_close()}</button>
            {/if}
        </div>
    {:else if entry}
        <Player bind:this={playerView} {entry} profileId={$currentUser?.active_profile_id} preferenceContext={{ serverOrigin: getServerOrigin() || $page.url.origin, userId: $currentUser?.id }} mediaItem={entry.item} mediaFileId={entry.file.id} startPositionMs={entry.startPositionMs} onactivechange={handleActiveChange} onexitintent={titleExitIntent} onstop={handleStop} />
    {/if}
</div>

<style>
    .player-route {
        position: fixed;
        inset: 0;
        background-color: #000;
        z-index: 1000;
    }

    .loading-state {
        position: absolute;
        inset: 0;
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: 1rem;
        color: var(--color-text-muted);
    }

    .loading-spinner {
        width: 32px;
        height: 32px;
        border: 3px solid var(--color-border);
        border-top-color: var(--color-accent);
        border-radius: 50%;
        animation: spin 0.8s linear infinite;
    }

    @media (prefers-reduced-motion: reduce) {
        .loading-spinner { animation: none; }
    }

    @keyframes spin {
        to {
            transform: rotate(360deg);
        }
    }
</style>
