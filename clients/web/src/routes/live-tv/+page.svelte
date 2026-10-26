<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->
<script>
    import { getContext, onMount, onDestroy, tick } from 'svelte';
    import { get } from 'svelte/store';
    import { auth, currentUser } from '$lib/stores/auth.js';
    import { getServerOrigin } from '$lib/api/core.js';
    import { m } from '$lib/paraglide/messages.js';
    import { getLocale } from '$lib/paraglide/runtime.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    import { createCatalogController } from '$lib/live-tv/controller.js';
    import { formatInstant } from '$lib/live-tv/presentation.js';
    import ChannelBrowser from '$lib/components/live-tv/ChannelBrowser.svelte';
    import ProgrammeGuide from '$lib/components/live-tv/ProgrammeGuide.svelte';

    const activeProfile = getContext('active-profile');
    const profile = activeProfile?.();
    const profileId = profile?.id;
    const standard = profile?.profile_type === 'standard';
    let root;
    let heading;
    let guideHeading = $state();
    let clock = $state(Date.now());
    const sessionContext = { serverOrigin: getServerOrigin() || globalThis.location?.origin, userId: get(currentUser)?.id };
    const controller = createCatalogController({ profileId, revalidate: async () => {
        await auth.checkSession({ revalidate: true, expectedContext: sessionContext });
    } });
    let retryWait = $derived($controller.error ? Math.max(0, Math.ceil(($controller.error.notBefore - clock) / 1000)) : 0);
    let status = $derived($controller.error ? errorText($controller.error.kind) : $controller.channelsLoading ? m.live_tv_loading_channels()
        : $controller.guideLoading ? m.live_tv_loading_guide() : $controller.context ? m.live_tv_ready({ channels: $controller.channels.length, programmes: $controller.programmes.length }) : '');

    function errorText(kind) {
        if (kind === 'changed') return m.live_tv_changed();
        if (kind === 'session') return m.live_tv_session();
        if (kind === 'unavailable') return m.live_tv_unavailable();
        if (kind === 'busy') return m.live_tv_busy();
        return m.live_tv_failed();
    }
    async function select(id, source) {
        await controller.select(id);
        await tick();
        if (controller.getState().selected?.id === id && document.activeElement === source) guideHeading?.focus();
    }
    async function reload(event) {
        if (retryWait > 0) return;
        const source = event.currentTarget;
        await controller.reload();
        await tick();
        if (!source.isConnected && document.activeElement === document.body) heading?.focus();
    }
    onMount(() => {
        const unsubscribe = controller.subscribe((state) => {
            if (!state.error) return;
            clock = Date.now();
            const focused = document.activeElement;
            const inside = root?.contains(focused);
            tick().then(() => { if (inside && focused && !focused.isConnected) heading?.focus(); });
        });
        const hide = () => controller.clear();
        const restore = (event) => { if (event.persisted && standard) controller.reload(); };
        window.addEventListener('pagehide', hide);
        window.addEventListener('pageshow', restore);
        const timer = setInterval(() => { if (controller.getState().error) clock = Date.now(); }, 1000);
        if (standard) controller.reload();
        return () => { unsubscribe(); clearInterval(timer); window.removeEventListener('pagehide', hide); window.removeEventListener('pageshow', restore); };
    });
    onDestroy(() => controller.dispose());
</script>

<svelte:head><title>{m.live_tv_title()} — Duskcue</title></svelte:head>

<div class="live-tv" lang={messageLocale('live_tv_title')} bind:this={root}>
    <header><h1 class="tonight-heading" tabindex="-1" bind:this={heading}>{m.live_tv_title()}</h1><p>{m.live_tv_description()}</p></header>
    {#if !standard}<p>{m.live_tv_standard_only()}</p>
    {:else}
        <p class="status" role="status" aria-live="polite" aria-atomic="true">{status}</p>
        {#if $controller.error}
            <div class="recovery">
                {#if retryWait > 0}<p>{m.live_tv_retry_wait({ seconds: retryWait })}</p>{/if}
                <button class="primary-action" type="button" aria-disabled={retryWait > 0} onclick={reload}>{m.live_tv_reload()}</button>
            </div>
        {:else}
            {#if $controller.context}
                <details class="snapshot"><summary>{m.live_tv_snapshot_details()}</summary>
                    <p>{m.live_tv_imported()}: <time datetime={$controller.context.imported_at}>{formatInstant($controller.context.imported_at, getLocale())}</time></p>
                    <p>{m.live_tv_observed()}: <time datetime={$controller.context.observed_at}>{formatInstant($controller.context.observed_at, getLocale())}</time></p>
                    <p>{m.live_tv_refresh_state()}: {#if $controller.context.latest_refresh.state === 'succeeded'}{m.live_tv_refresh_succeeded()}{:else if $controller.context.latest_refresh.state === 'running'}{m.live_tv_refresh_running()}{:else}{m.live_tv_refresh_failed()}{/if}</p>
                </details>
            {/if}
            <div class="catalog-columns">
                <ChannelBrowser state={$controller} onselect={select} onpage={controller.channels} />
                <ProgrammeGuide state={$controller} locale={getLocale()} bind:heading={guideHeading} onview={controller.setView} onwindow={controller.shiftWindow} onpage={controller.programmes} />
            </div>
        {/if}
    {/if}
</div>

<style>
    .live-tv { min-width: 0; display: flex; flex-direction: column; gap: 1.2rem; overflow-wrap: anywhere; }
    header { display: grid; gap: 0.8rem; }
    h1 { font-size: clamp(2.2rem, 5vw, 3.5rem); }
    header p, .snapshot { color: var(--color-text-secondary); }
    .status { min-height: 1.5em; }
    .catalog-columns { display: grid; grid-template-columns: minmax(0, 1fr); gap: 2rem; align-items: start; }
    .recovery { display: grid; justify-items: start; gap: 1rem; }
    summary { cursor: pointer; padding-block: 0.65rem; }
    @media (min-width: 64rem) { .catalog-columns { grid-template-columns: minmax(15rem, 1fr) minmax(0, 2fr); } }
</style>
