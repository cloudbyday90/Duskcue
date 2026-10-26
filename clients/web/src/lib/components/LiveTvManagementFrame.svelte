<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->
<script>
    import { onMount, tick } from 'svelte';
    import { beforeNavigate, goto } from '$app/navigation';
    import { navigationGuard } from '$lib/navigation/guard.js';
    import { getServerOrigin } from '$lib/api/core.js';
    import { listenSessionChanges } from '$lib/household/session-sync.js';
    import { problemText } from '$lib/live-tv-management/presentation.js';
    import { formatInstant, deviceTimeZone } from '$lib/time/presentation.js';
    import { getLocale } from '$lib/paraglide/runtime.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    import { m } from '$lib/paraglide/messages.js';

    let { title, intro, controller, commands, standard, dirty = false, ondiscard = () => {}, children } = $props();
    let root; let heading; let dialog; let cancelButton; let resolver = null; let invoker = null;
    let question = $state(''); let action = $state(''); let clock = $state(Date.now()); let navigating = false;
    const busy = $derived($controller.busy || $commands);
    const wait = $derived(Math.max(0, Math.ceil(($controller.retryAt - clock) / 1000)));
    export async function review(text, label = m.iptv_manage_confirm()) {
        if (resolver || busy) return false;
        invoker = document.activeElement; question = text; action = label;
        const answer = new Promise((resolve) => { resolver = resolve; });
        await tick(); dialog.showModal(); cancelButton.focus(); return answer;
    }
    function settle(value) {
        const answer = resolver; resolver = null; dialog?.close(); question = '';
        const target = invoker; invoker = null;
        tick().then(() => target?.isConnected ? target.focus() : heading?.focus()); answer?.(value);
    }
    async function discard() { if (!dirty) return true; return review(m.iptv_manage_discard_question(), m.iptv_manage_discard()); }
    async function reload() { if (busy || wait > 0 || !await discard()) return; ondiscard(); await controller.reload(); }
    beforeNavigate((navigation) => {
        if (navigating || navigation.willUnload || (!dirty && !busy)) return;
        navigation.cancel();
        if (busy || !navigation.to) return;
        discard().then(async (approved) => { if (approved) { ondiscard(); navigating = true; try { await goto(navigation.to.url.href); } finally { navigating = false; } } });
    });
    onMount(() => {
        const unregister = navigationGuard.register({ isDirty: () => dirty, isBusy: () => busy, confirmDiscard: discard, discard: ondiscard });
        const stop = listenSessionChanges(() => getServerOrigin() || location.origin, () => { ondiscard(); controller.invalidate(); });
        const visibility = () => { if (document.visibilityState === 'hidden') controller.stopWatching?.(); };
        const hide = () => { ondiscard(); controller.invalidate(); };
        const restore = (event) => { if (event.persisted && standard) controller.reload(); };
        const unload = (event) => { if (dirty || busy) { event.preventDefault(); event.returnValue = ''; } };
        const subscription = controller.subscribe(() => {
            const focused = document.activeElement; const inside = root?.contains(focused);
            tick().then(() => { if (inside && focused && (!focused.isConnected || focused.matches(':disabled'))) heading?.focus(); });
        });
        document.addEventListener('visibilitychange', visibility); window.addEventListener('pagehide', hide); window.addEventListener('pageshow', restore); window.addEventListener('beforeunload', unload);
        const timer = setInterval(() => { if (controller.getState().retryAt) clock = Date.now(); }, 1000);
        if (standard) controller.reload();
        return () => { settle(false); unregister(); stop(); subscription(); clearInterval(timer); document.removeEventListener('visibilitychange', visibility); window.removeEventListener('pagehide', hide); window.removeEventListener('pageshow', restore); window.removeEventListener('beforeunload', unload); controller.dispose(); };
    });
</script>

<svelte:head><title>{title} — Duskcue</title></svelte:head>
<div class="management" lang={messageLocale('iptv_manage_admin_title')} bind:this={root}>
    <header><h1 class="tonight-heading" tabindex="-1" bind:this={heading}>{title}</h1><p>{intro}</p></header>
    <a href="/settings">{m.iptv_manage_back()}</a>
    {#if !standard}<p>{m.iptv_manage_standard_only()}</p>{:else}
        <p role="status" aria-live="polite" aria-atomic="true">{#if busy}{m.iptv_manage_working()}{:else if $controller.notice === 'accepted'}{m.iptv_manage_accepted()}{:else if $controller.notice === 'saved'}{m.iptv_manage_saved()}{/if}</p>
        {#if $controller.error}<p role="alert">{problemText($controller.error)}</p>{/if}
        {#if wait > 0}<p>{m.iptv_manage_wait({ seconds: wait })}</p>{/if}
        {#if $controller.session?.grant.enabled}<p>{m.iptv_manage_verified({ time: formatInstant($controller.session.grant.expires_at, getLocale()) })}</p>{:else}<p>{m.iptv_manage_locked()}</p>{/if}
        <div class="actions"><button class="secondary-action" type="button" aria-disabled={busy || wait > 0} onclick={reload}>{m.iptv_manage_reload()}</button><a href="/settings/household">{m.iptv_manage_unlock()}</a></div>
        <p>{m.household_zone()}: <bdi lang="">{deviceTimeZone()}</bdi></p>
        {@render children?.()}
    {/if}
    <dialog bind:this={dialog} aria-labelledby="live-tv-confirm-title" aria-describedby="live-tv-confirm-copy" oncancel={(event) => { event.preventDefault(); settle(false); }} onclose={() => { if (resolver) settle(false); }}>
        <h2 id="live-tv-confirm-title">{m.iptv_manage_confirmation()}</h2><p id="live-tv-confirm-copy">{question}</p>
        <div class="actions"><button class="secondary-action" type="button" bind:this={cancelButton} onclick={() => settle(false)}>{m.iptv_manage_keep()}</button><button class="primary-action" type="button" onclick={() => settle(true)}>{action}</button></div>
    </dialog>
</div>

<style>
    .management { display: grid; gap: 1.1rem; max-width: 66rem; min-width: 0; overflow-wrap: anywhere; }
    h1 { font-size: clamp(2rem, 4vw, 3.5rem); }
    .management :global(section), .management :global(form), .management :global(fieldset) { display: grid; gap: 0.8rem; min-width: 0; }
    .management :global(section) { border-block-start: 1px solid var(--color-border); padding-block-start: 1rem; }
    .management :global(fieldset) { border: 0; margin: 0; padding: 0; }
    .management :global(legend) { font-weight: 700; margin-block-end: 0.75rem; }
    .management :global(input:not([type='checkbox'])), .management :global(select), .management :global(textarea) { min-width: 0; width: 100%; max-width: 100%; min-height: 44px; padding: 0.6rem; background: var(--color-bg-elevated); border: 1px solid var(--color-border); border-radius: var(--radius-sm); }
    .management :global(textarea) { min-height: 6rem; resize: vertical; }
    .management :global(.check) { display: flex; align-items: center; gap: 0.7rem; min-height: 44px; }
    .management :global(.actions) { display: flex; flex-wrap: wrap; align-items: center; gap: 0.75rem; }
    .management :global(.metadata) { display: grid; grid-template-columns: minmax(0, 1fr); gap: 0.5rem; }
    .management :global([role='alert']) { color: var(--color-error); }
    .management :global(p) { color: var(--color-text-secondary); }
    .management :global(button[aria-disabled='true']) { color: var(--color-text-muted); cursor: default; }
    .management :global(fieldset:disabled) { color: var(--color-text-secondary); }
    dialog { max-width: min(34rem, calc(100vw - 2rem)); padding: 1.5rem; background: var(--color-bg-elevated); color: var(--color-text-primary); border: 1px solid var(--color-border); border-radius: var(--radius-sm); }
    dialog::backdrop { background: rgb(0 0 0 / 65%); }
    dialog p { margin-block: 1rem; }
</style>
