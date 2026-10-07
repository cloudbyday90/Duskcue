<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->

<script>
    import { onMount, tick } from 'svelte';
    import { beforeNavigate, goto } from '$app/navigation';
    import { page } from '$app/stores';
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    import { navigationGuard } from '$lib/navigation/guard.js';
    import { selectDesktopServer } from '$lib/desktop/session.js';
    import { createServerSelection } from '$lib/desktop/selection.js';
    import DesktopServerDiscardDialog from './DesktopServerDiscardDialog.svelte';

    let { server = null, onbeforeselect = () => navigationGuard.requestNavigationTransition(), onselected = (_result) => {}, onsettled = (_result) => {} } = $props();
    const identity = $props.id();
    let input = $state('');
    let networkMode = $state('local');
    let baselineInput = $state('');
    let baselineMode = $state('local');
    let appliedServer = $state(null);
    let mounted = false;
    let discardDialog;
    let errorElement = $state();
    let urlInput;

    const selection = createServerSelection({
        selectServer: selectDesktopServer,
        beforeSelect: () => onbeforeselect(),
        onSelected: (result) => {
            baselineInput = input = result.server.origin;
            baselineMode = networkMode = result.server.network_mode;
            return onselected(result);
        },
        onSettled: (result) => onsettled(result),
    });
    let checking = $derived($selection.phase === 'checking');
    let selecting = $derived($selection.phase === 'selecting');
    let busy = $derived(checking || selecting);
    let dirty = $derived(input !== baselineInput || networkMode !== baselineMode);
    let errorLocale = $derived(typeof $selection.error === 'string' || $selection.error?.detail || $selection.error?.message ? 'en' : messageLocale('tonight_desktop_server_failed'));
    let errorText = $derived(typeof $selection.error === 'string' ? $selection.error : $selection.error?.detail || $selection.error?.message || ($selection.error ? m.tonight_desktop_server_failed() : ''));

    $effect(() => {
        const key = `${server?.origin || ''}:${server?.network_mode || 'local'}`;
        if (key === appliedServer || selecting) return;
        appliedServer = key;
        baselineInput = input = server?.origin || '';
        baselineMode = networkMode = server?.network_mode || 'local';
    });

    onMount(() => {
        mounted = true;
        const unregister = navigationGuard.register({
            isDirty: () => dirty && !checking,
            isBusy: () => selecting,
            confirmDiscard: () => discardDialog.confirm(),
            discard: discardChanges,
        });
        return () => { mounted = false; unregister(); selection.dispose(); };
    });

    beforeNavigate((navigation) => {
        if (checking) { navigation.cancel(); return; }
        if (!dirty && !selecting) return;
        if (navigation.willUnload) { navigation.cancel(); return; }
        const destination = navigation.to?.url;
        if (!destination || (destination.pathname === $page.url.pathname && destination.search === $page.url.search)) return;
        navigation.cancel();
        const delta = navigation.type === 'popstate' ? navigation.delta : null;
        navigationGuard.requestNavigationTransition().then(async (allowed) => {
            if (!allowed || !mounted) return;
            if (delta !== null) window.history.go(delta);
            else await goto(destination);
        });
    });

    function discardChanges() {
        input = baselineInput;
        networkMode = baselineMode;
        urlInput?.focus();
    }

    async function submit(event) {
        event.preventDefault();
        await selection.connect({ input, networkMode });
        if (mounted && $selection.error) { await tick(); errorElement?.focus(); }
    }
</script>

<section class="server-selector" aria-labelledby={`${identity}-heading`}>
    <header>
        <p class="eyebrow">Duskcue</p>
        <h1 lang={server ? messageLocale('tonight_desktop_server_switch_title') : messageLocale('tonight_desktop_server_title')} id={`${identity}-heading`}>{server ? m.tonight_desktop_server_switch_title() : m.tonight_desktop_server_title()}</h1>
        <p lang={messageLocale('tonight_desktop_server_description')} class="description">{m.tonight_desktop_server_description()}</p>
    </header>
    <form onsubmit={submit} aria-busy={busy}>
        <div class="field">
            <label lang={messageLocale('tonight_desktop_server_url')} for={`${identity}-url`}>{m.tonight_desktop_server_url()}</label>
            <input lang={messageLocale('tonight_desktop_server_url')} id={`${identity}-url`} bind:this={urlInput} type="text" inputmode="url" autocomplete="url" autocapitalize="none" spellcheck="false" required bind:value={input} disabled={busy} aria-describedby={`${identity}-url-help`} />
            <p lang={messageLocale('tonight_desktop_server_url_help')} id={`${identity}-url-help`}>{m.tonight_desktop_server_url_help()}</p>
        </div>
        <div class="field">
            <label lang={messageLocale('tonight_desktop_server_mode')} for={`${identity}-mode`}>{m.tonight_desktop_server_mode()}</label>
            <select lang={messageLocale('tonight_desktop_server_mode')} id={`${identity}-mode`} bind:value={networkMode} disabled={busy} aria-describedby={`${identity}-mode-help`}>
                <option lang={messageLocale('tonight_desktop_server_local')} value="local">{m.tonight_desktop_server_local()}</option>
                <option lang={messageLocale('tonight_desktop_server_vpn')} value="remote_vpn">{m.tonight_desktop_server_vpn()}</option>
                <option lang={messageLocale('tonight_desktop_server_exposed')} value="exposed">{m.tonight_desktop_server_exposed()}</option>
            </select>
            <p lang={messageLocale('tonight_desktop_server_mode_help')} id={`${identity}-mode-help`}>{m.tonight_desktop_server_mode_help()}</p>
        </div>
        <p lang={selecting ? messageLocale('tonight_desktop_server_connecting') : checking ? messageLocale('tonight_desktop_server_checking_changes') : undefined} class="status" role="status">{selecting ? m.tonight_desktop_server_connecting() : checking ? m.tonight_desktop_server_checking_changes() : ''}</p>
        {#if errorText}<p lang={errorLocale} class="error" role="alert" tabindex="-1" bind:this={errorElement}>{errorText}</p>{/if}
        <div class="actions">
            <button lang={messageLocale('tonight_desktop_server_connect')} type="submit" class="tonight-button" disabled={busy}>{m.tonight_desktop_server_connect()}</button>
            {#if dirty}<button lang={messageLocale('tonight_desktop_server_discard')} type="button" class="tonight-button secondary" disabled={busy} onclick={discardChanges}>{m.tonight_desktop_server_discard()}</button>{/if}
        </div>
    </form>
</section>
<DesktopServerDiscardDialog bind:this={discardDialog} />

<style>
    .server-selector { inline-size: min(100%, 36rem); margin-inline: auto; display: grid; gap: 1.75rem; }
    .eyebrow { color: var(--color-accent); text-transform: uppercase; letter-spacing: 0.14em; font-size: 0.75rem; margin-block-end: 0.75rem; }
    h1 { font-size: clamp(1.75rem, 5vw, 2.25rem); line-height: 1.15; margin-block-end: 1rem; }
    .description, .field p { color: var(--color-text-secondary); line-height: 1.6; }
    form { display: grid; gap: 1.25rem; }
    .field { display: grid; gap: 0.5rem; }
    label { font-size: 0.875rem; font-weight: 600; }
    input, select { min-inline-size: 0; inline-size: 100%; min-block-size: 44px; border: 1px solid var(--color-border); border-radius: var(--radius-md); padding: 0.65rem 0.75rem; background: var(--color-bg-surface); color: var(--color-text-primary); font: inherit; }
    .field p, .status, .error { font-size: 0.875rem; }
    .status:empty { display: none; }
    .error { color: var(--color-error); overflow-wrap: anywhere; }
    .actions { display: flex; flex-wrap: wrap; gap: 0.75rem; }
    input:focus-visible, select:focus-visible, button:focus-visible, .error:focus-visible { outline: 2px solid var(--color-accent); outline-offset: 3px; }
</style>
