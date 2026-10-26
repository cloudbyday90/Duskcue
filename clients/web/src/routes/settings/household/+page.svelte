<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->
<script>
    import { getContext, onMount, onDestroy, tick } from 'svelte';
    import { auth, currentUser } from '$lib/stores/auth.js';
    import { getDesktopSessionScope } from '$lib/desktop/session.js';
    import { getServerOrigin } from '$lib/api/core.js';
    import { listenSessionChanges } from '$lib/household/session-sync.js';
    import { createHouseholdController } from '$lib/household/controller.js';
    import { formatInstant, deviceTimeZone } from '$lib/time/presentation.js';
    import { getLocale } from '$lib/paraglide/runtime.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    import { m } from '$lib/paraglide/messages.js';

    const profile = getContext('active-profile')?.();
    const standard = profile?.profile_type === 'standard';
    const desktop = getDesktopSessionScope();
    const native = desktop.isDesktop;
    const controller = createHouseholdController({ auth, profileId: profile?.id, native });
    let root;
    let heading;
    let password = $state('');
    let handoffPassword = $state('');
    let handoff = $state('');
    let operation = $state('password');
    let passkeyId = $state('');
    let name = $state('');
    let localError = $state(false);
    let clock = $state(Date.now());
    let wait = $derived(Math.max(0, Math.ceil(($controller.retryAt - clock) / 1000)));
    let usable = $derived(Boolean($controller.context && $controller.status && !$controller.busy && wait === 0));
    function erase() { password = ''; handoffPassword = ''; handoff = ''; name = ''; passkeyId = ''; }
    function errorText(kind) {
        if (kind === 'invalid') return m.household_invalid();
        if (kind === 'busy') return m.household_busy();
        if (kind === 'changed') return m.household_changed();
        if (kind === 'session') return m.household_session();
        if (kind === 'pending') return m.household_pending();
        return m.household_uncertain();
    }
    async function submitPassword(event) {
        event.preventDefault(); localError = false;
        const value = password; password = '';
        try { await controller.password(value); } catch { localError = true; }
    }
    async function enroll(event) {
        event.preventDefault(); localError = false;
        const token = handoff.trim(); const value = handoffPassword; const label = name.trim(); const selected = passkeyId;
        erase();
        try {
            if (!token || new TextEncoder().encode(token).length > 128 || (operation === 'register' && (!label || [...label].length > 200))) throw new Error();
            if (operation === 'password') await controller.password(value, token);
            else await controller.passkey(operation, token, selected, label);
        } catch { localError = true; }
    }
    function methodChanged() { erase(); localError = false; if (operation === 'enroll') controller.candidates(); }
    onMount(() => {
        let previousDeadline = 0;
        const unsubscribe = controller.subscribe((state) => {
            if (state.error || (previousDeadline > 0 && state.deadline === 0)) erase();
            previousDeadline = state.deadline;
            const focused = document.activeElement; const inside = root?.contains(focused);
            tick().then(() => { if (inside && focused && !focused.isConnected) heading?.focus(); });
        });
        const stop = listenSessionChanges(() => getServerOrigin() || location.origin, () => { if (!controller.getState().busy) { erase(); controller.clear(); } });
        const hide = () => { erase(); controller.clear(); };
        const restore = (event) => { if (event.persisted && standard) controller.load(); };
        window.addEventListener('pagehide', hide); window.addEventListener('pageshow', restore);
        const timer = setInterval(() => { if (controller.getState().retryAt) clock = Date.now(); }, 1000);
        if (standard) controller.load();
        return () => { unsubscribe(); stop(); clearInterval(timer); window.removeEventListener('pagehide', hide); window.removeEventListener('pageshow', restore); };
    });
    onDestroy(() => { erase(); controller.dispose(); });
</script>

<svelte:head><title>{m.household_title()} — Duskcue</title></svelte:head>

<div class="household" lang={messageLocale('household_title')} bind:this={root}>
    <header><h1 class="tonight-heading" tabindex="-1" bind:this={heading}>{m.household_title()}</h1><p>{m.household_intro()}</p></header>
    {#if !standard}<p>{m.household_standard_only()}</p>
    {:else}
        <p role="status" aria-live="polite" aria-atomic="true">{#if $controller.busy}{m.household_working()}{:else if $controller.status?.expires_at}{m.household_verified({ time: formatInstant($controller.status.expires_at, getLocale()) })}{:else if $controller.status}{m.household_locked()}{/if}</p>
        {#if $controller.error}<p role="alert">{errorText($controller.error)}</p>{/if}
        {#if localError}<p role="alert">{m.household_validation()}</p>{/if}
        {#if wait > 0}<p>{m.household_wait({ seconds: wait })}</p>{/if}
        <div class="actions"><button class="secondary-action" type="button" aria-disabled={$controller.busy || wait > 0} onclick={() => !$controller.busy && wait === 0 && controller.load()}>{m.household_check()}</button>
            <button class="secondary-action" type="button" aria-disabled={!usable} onclick={() => usable && controller.lock()}>{m.household_lock()}</button></div>
        {#if $controller.status}
            <p>{m.household_protection()}: {$controller.status.protection_enabled ? m.household_enabled() : m.household_disabled()}</p>
            <p>{m.household_factor_count({ count: $controller.status.trusted_passkeys })}</p>
            <p>{m.household_duration()}</p>
            <p>{m.household_zone()}: <bdi lang="">{deviceTimeZone() || m.household_unknown_zone()}</bdi></p>
        {/if}
        {#if usable}
            {#if $controller.status.password_available}
                <section aria-labelledby="household-password"><h2 id="household-password">{m.household_password_title()}</h2>
                    <form onsubmit={submitPassword}>
                        <label class="visually-hidden" for="household-username">{m.household_username()}</label><input class="visually-hidden" id="household-username" name="username" autocomplete="username" value={$currentUser?.username || ''} readonly tabindex="-1" />
                        <label for="owner-password">{m.household_password()}</label><input id="owner-password" name="password" type="password" autocomplete="current-password" required bind:value={password} />
                        <button class="primary-action" type="submit">{m.household_verify_password()}</button>
                    </form>
                </section>
            {/if}
            {#if !native && $controller.status.trusted_passkeys > 0}
                <section aria-labelledby="household-passkey"><h2 id="household-passkey">{m.household_passkey_title()}</h2><button class="primary-action" type="button" onclick={() => controller.passkey()}>{m.household_verify_passkey()}</button></section>
            {/if}
            {#if native}<p>{m.household_browser_help()} <bdi lang="">{desktop.server?.origin}</bdi></p>{/if}
            <details><summary>{m.household_handoff_title()}</summary>
                <p>{m.household_handoff_help()}</p><p>{m.household_activation_help()}</p>
                <form onsubmit={enroll}>
                    <label class="visually-hidden" for="owner-enrollment-username">{m.household_username()}</label><input class="visually-hidden" id="owner-enrollment-username" name="username" autocomplete="username" value={$currentUser?.username || ''} readonly tabindex="-1" />
                    <label for="owner-method">{m.household_method()}</label>
                    <select id="owner-method" bind:value={operation} onchange={methodChanged}>
                        <option value="password">{m.household_existing_password()}</option>
                        {#if !native}<option value="enroll">{m.household_existing_key()}</option><option value="register">{m.household_new_key()}</option>{/if}
                    </select>
                    <label for="owner-handoff">{m.household_handoff()}</label><input id="owner-handoff" type="password" name="handoff" autocomplete="one-time-code" maxlength="128" required bind:value={handoff} />
                    {#if operation === 'password'}
                        <p>{m.household_password_help()}</p><label for="owner-enrollment-password">{m.household_password()}</label><input id="owner-enrollment-password" type="password" name="password" autocomplete="current-password" required bind:value={handoffPassword} />
                    {:else if operation === 'enroll'}
                        <label for="owner-existing-key">{m.household_key()}</label><select id="owner-existing-key" required bind:value={passkeyId}><option value="">{m.household_choose_key()}</option>{#each $controller.passkeys as key}<option value={key.id} lang="">{key.name}</option>{/each}</select>
                    {:else}
                        <label for="owner-key-name">{m.household_key_name()}</label><input id="owner-key-name" name="passkey-name" autocomplete="off" required bind:value={name} />
                    {/if}
                    <button class="primary-action" type="submit">{$controller.status.protection_enabled ? m.household_recover() : m.household_activate()}</button>
                </form>
            </details>
        {/if}
    {/if}
</div>

<style>
    .household { display: grid; gap: 1.2rem; max-width: 55rem; min-width: 0; overflow-wrap: anywhere; }
    header, section, details[open] { display: grid; gap: 0.85rem; }
    h1 { font-size: clamp(2.1rem, 5vw, 3.5rem); }
    h2 { font-size: 1.35rem; }
    p { color: var(--color-text-secondary); }
    [role='alert'] { color: var(--color-error); }
    .actions { display: flex; flex-wrap: wrap; gap: 0.7rem; }
    form { display: grid; gap: 0.7rem; max-width: 32rem; min-width: 0; }
    input, select { min-width: 0; max-width: 100%; min-height: 44px; padding: 0.6rem; background: var(--color-bg-elevated); border: 1px solid var(--color-border); border-radius: var(--radius-sm); }
    input.visually-hidden { min-height: 0; padding: 0; }
    summary { min-height: 44px; padding-block: 0.65rem; cursor: pointer; }
    details { border-block-start: 1px solid var(--color-border); padding-block-start: 0.6rem; }
    button[aria-disabled='true'] { color: var(--color-text-muted); cursor: default; }
</style>
