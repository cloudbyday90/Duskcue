<script>
    import { onMount } from 'svelte';
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    import { page } from '$app/stores';
    import { localizeUrl } from '$lib/paraglide/runtime.js';

    let {
        open = $bindable(false), profiles = [], activeProfile = null,
        switchingProfileId = null, deviceCanRememberProfile = false,
        rememberedProfileId = null, rememberProfileOnDevice = $bindable(false),
        canAccessAdmin = false, disabled = false,
        onselect, onremember, onforget, onlogout,
    } = $props();
    let wrapper;
    let trigger;
    let isKids = $derived(activeProfile?.profile_type === 'kids');
    let viewingPreferencesDestination = $derived.by(() => {
        const localized = localizeUrl(new URL('/settings/preferences', $page.url));
        return `${localized.pathname}${localized.search}${localized.hash}`;
    });

    export function focusTrigger() {
        trigger?.focus();
    }

    function close(restoreFocus = false) {
        open = false;
        if (restoreFocus) trigger?.focus();
    }

    onMount(() => {
        const outside = (event) => {
            if (open && !wrapper.contains(event.target)) close();
        };
        const escape = (event) => {
            if (open && event.key === 'Escape') {
                event.preventDefault();
                event.stopPropagation();
                close(true);
            }
        };
        document.addEventListener('pointerdown', outside);
        document.addEventListener('focusin', outside);
        document.addEventListener('keydown', escape);
        return () => {
            document.removeEventListener('pointerdown', outside);
            document.removeEventListener('focusin', outside);
            document.removeEventListener('keydown', escape);
        };
    });
</script>

<div class="profile-area" bind:this={wrapper}>
    <button bind:this={trigger} class="profile-trigger" onclick={() => open = !open}
        aria-label={m.routes_layout_user_menu()} aria-expanded={open} aria-controls="profile-disclosure" {disabled}>
        <span class="avatar" aria-hidden="true">{activeProfile?.name?.[0]?.toUpperCase() || 'P'}</span>
        <span lang={activeProfile?.name ? undefined : messageLocale('tonight_choose_profile')} class="profile-name">{activeProfile?.name || m.tonight_choose_profile()}</span>
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="m6 9 6 6 6-6" /></svg>
    </button>
    {#if open}
        <div id="profile-disclosure" class="profile-disclosure">
            <p lang={messageLocale('tonight_whos_watching')} class="disclosure-label">{m.tonight_whos_watching()}</p>
            <div class="profile-options">
                {#each profiles as profile (profile.id)}
                    <button lang={profile.profile_type === 'kids' ? messageLocale('tonight_kids') : undefined} class="profile-option" aria-pressed={profile.id === activeProfile?.id}
                        onclick={() => onselect(profile)} disabled={!!switchingProfileId}>
                        <span class="avatar small" aria-hidden="true">{profile.name?.[0]?.toUpperCase() || 'P'}</span>
                        <span class="option-name">{profile.name}</span>
                        {#if profile.profile_type === 'kids'}<small lang={messageLocale('tonight_kids')}>{m.tonight_kids()}</small>{/if}
                        {#if profile.id === activeProfile?.id}<span aria-hidden="true">✓</span>{/if}
                    </button>
                {/each}
            </div>
            {#if deviceCanRememberProfile && activeProfile}
                {#if rememberedProfileId === activeProfile.id}
                    <button lang={messageLocale('tonight_forget_device')} class="device-action" onclick={onforget} disabled={!!switchingProfileId}>{m.tonight_forget_device()}</button>
                {:else}
                    <label class="remember">
                        <input lang={messageLocale('tonight_remember_profile')} type="checkbox" bind:checked={rememberProfileOnDevice} onchange={onremember} disabled={!!switchingProfileId} />
                        <span lang={messageLocale('tonight_remember_profile')}>{m.tonight_remember_profile()}</span>
                    </label>
                {/if}
            {/if}
            <div class="links">
                <a lang={messageLocale('tonight_viewing_preferences')} href={viewingPreferencesDestination} onclick={() => close()}>{m.tonight_viewing_preferences()}</a>
                <a href="/libraries" onclick={() => close()}>{m.routes_layout_libraries()}</a>
                {#if !isKids}
                    <a lang={messageLocale('tonight_manage_profiles')} href="/settings/profiles" onclick={() => close()}>{m.tonight_manage_profiles()}</a>
                    <a lang={messageLocale('tonight_settings')} href="/settings" onclick={() => close()}>{m.tonight_settings()}</a>
                    {#if canAccessAdmin}<a href="/admin" onclick={() => close()}>{m.routes_admin_page_admin()}</a>{/if}
                {/if}
                <button lang={messageLocale('tonight_sign_out')} onclick={onlogout}>{m.tonight_sign_out()}</button>
            </div>
        </div>
    {/if}
</div>

<style>
    .profile-area { position: relative; }
    .profile-trigger, .profile-option { display: flex; align-items: center; gap: 0.65rem; min-height: 44px; padding: 0.4rem 0.6rem; border-radius: var(--radius-md); }
    .profile-trigger:hover, .profile-option:hover, .links a:hover, .links button:hover { background: var(--color-bg-hover); }
    .avatar { display: grid; place-items: center; flex-shrink: 0; width: 32px; height: 32px; border-radius: 50%; background: var(--color-accent); color: var(--color-on-accent); font-weight: 700; }
    .small { width: 28px; height: 28px; }
    .profile-name { max-width: 8rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .profile-disclosure { position: absolute; top: calc(100% + 0.5rem); inset-inline-end: 0; width: min(300px, calc(100vw - 2rem)); max-height: calc(100dvh - 6rem); overflow-y: auto; padding: 0.75rem; background: var(--color-bg-elevated); border: 1px solid var(--color-border); border-radius: var(--radius-lg); box-shadow: var(--shadow-elevated); z-index: 110; }
    .disclosure-label { color: var(--color-text-secondary); font-size: 0.8rem; padding: 0.4rem; }
    .profile-options { display: grid; gap: 0.2rem; }
    .profile-option { text-align: start; width: 100%; }
    .profile-option[aria-pressed='true'] { background: var(--color-accent-muted); }
    .option-name { flex: 1; overflow-wrap: anywhere; }
    small { color: var(--color-text-secondary); }
    .remember { display: flex; align-items: flex-start; gap: 0.6rem; padding: 0.75rem 0.4rem; font-size: 0.8rem; }
    input { margin-top: 0.2rem; accent-color: var(--color-accent); }
    .device-action { min-height: 44px; padding: 0.5rem; color: var(--color-text-secondary); text-align: start; }
    .links { display: grid; margin-top: 0.5rem; border-top: 1px solid var(--color-border); padding-top: 0.5rem; }
    .links a, .links button { min-height: 44px; display: flex; align-items: center; padding: 0.5rem 0.65rem; border-radius: var(--radius-sm); text-align: start; }
    @media (max-width: 600px) { .profile-name { display: none; } }
</style>
