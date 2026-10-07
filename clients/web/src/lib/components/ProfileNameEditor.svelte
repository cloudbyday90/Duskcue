<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->

<script>
    import { onDestroy, tick } from 'svelte';
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    import { updateProfile } from '../api/profiles.js';

    let { profile, disabled = false, onupdated = (_profile) => {} } = $props();
    const identity = $props.id();
    let name = $state('');
    let initializedProfile;
    let saving = $state(false);
    let error = $state('');
    let errorLocale = $state('en');
    let status = $state('');
    let statusLocale = $state('en');
    let mounted = true;
    let nameInput = $state();
    const dirty = $derived(name.trim() !== profile.name);
    $effect(() => {
        if (initializedProfile !== profile.id) {
            initializedProfile = profile.id;
            name = profile.name;
        }
    });

    async function save(event) {
        event.preventDefault();
        if (saving || disabled || !dirty) return;
        const profileId = profile.id;
        const heldFocus = event.currentTarget.contains(document.activeElement);
        saving = true;
        error = '';
        status = '';
        try {
            const updated = await updateProfile(profile.id, { name: name.trim() });
            if (!mounted || updated.id !== profile.id) return;
            name = updated.name;
            onupdated(updated);
            status = m.tonight_profile_name_saved();
            statusLocale = messageLocale('tonight_profile_name_saved');
        } catch (failure) {
            if (mounted) {
                error = failure.detail || m.tonight_profile_name_failed();
                errorLocale = failure.detail ? 'en' : messageLocale('tonight_profile_name_failed');
            }
        } finally {
            if (mounted) saving = false;
            await tick();
            if (mounted && profileId === profile.id && heldFocus && document.activeElement === document.body) nameInput?.focus();
        }
    }

    async function discard(event) {
        const heldFocus = document.activeElement === event.currentTarget;
        name = profile.name;
        error = '';
        status = '';
        await tick();
        if (mounted && heldFocus && document.activeElement === document.body) nameInput?.focus();
    }

    onDestroy(() => { mounted = false; });
</script>

<form lang={messageLocale('tonight_profile_name_form')} onsubmit={save} aria-busy={saving} aria-label={m.tonight_profile_name_form({ name: profile.name })}>
    <label lang={messageLocale('tonight_profile_name_label')} for={`${identity}-name`}>{m.tonight_profile_name_label()}</label>
    <div class="name-controls">
        <input lang={messageLocale('tonight_profile_name_label')} bind:this={nameInput} id={`${identity}-name`} name="name" bind:value={name} required minlength="1" maxlength="80" disabled={disabled || saving} aria-describedby={error ? `${identity}-error` : undefined} />
        <button lang={saving ? messageLocale('tonight_preferences_saving') : messageLocale('tonight_profile_name_save')} type="submit" disabled={disabled || saving || !dirty || !name.trim()}>{saving ? m.tonight_preferences_saving() : m.tonight_profile_name_save()}</button>
        {#if dirty}<button lang={messageLocale('tonight_preferences_discard')} type="button" disabled={saving} onclick={discard}>{m.tonight_preferences_discard()}</button>{/if}
    </div>
    <p lang={statusLocale} role="status" aria-live="polite">{status}</p>
    {#if error}<p lang={errorLocale} id={`${identity}-error`} role="alert">{error}</p>{/if}
</form>

<style>
    form { display: grid; gap: 0.35rem; margin-block: 0.8rem; }
    label { color: var(--color-text-secondary); font-size: 0.85rem; }
    .name-controls { display: flex; flex-wrap: wrap; gap: 0.5rem; }
    input { min-inline-size: 0; inline-size: 100%; min-block-size: 44px; border: 1px solid var(--color-border); border-radius: var(--radius-sm); padding: 0.65rem; background: var(--color-bg-deep); }
    button { min-block-size: 44px; padding: 0.5rem 0.75rem; border: 1px solid var(--color-border); border-radius: var(--radius-sm); }
    button:focus-visible, input:focus-visible { outline: 2px solid var(--color-accent); outline-offset: 2px; }
    p { color: var(--color-text-secondary); font-size: 0.85rem; }
    p[role='alert'] { color: var(--color-error); }
</style>
