<script>
    import { getLocale } from '$lib/paraglide/runtime.js';
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    let {
        profiles = [], loading = false, switchingProfileId = null,
        deviceCanRememberProfile = false, rememberProfileOnDevice = $bindable(false),
        onselect, onreload, onlogout,
    } = $props();
</script>

<section lang={profiles.length ? messageLocale('tonight_whos_watching') : messageLocale('tonight_profiles_unavailable')} class="profile-gate" aria-labelledby="profile-gate-title" aria-busy={loading}>
    {#if loading}
        <p lang={messageLocale('tonight_loading_profiles')} role="status">{m.tonight_loading_profiles()}</p>
    {:else if profiles.length}
        <h1 lang={messageLocale('tonight_whos_watching')} id="profile-gate-title" class="tonight-heading">{m.tonight_whos_watching()}</h1>
        <p lang={messageLocale('tonight_profile_gate_description')}>{m.tonight_profile_gate_description()}</p>
        <div lang={getLocale()} class="profiles">
            {#each profiles as profile (profile.id)}
                <button lang={profile.profile_type === 'kids' ? messageLocale('tonight_kids') : undefined} class="profile-gate-option" onclick={() => onselect(profile)} disabled={!!switchingProfileId}>
                    <span class="avatar" aria-hidden="true">{profile.name?.[0]?.toUpperCase() || 'P'}</span>
                    <span>{profile.name}</span>
                    {#if profile.profile_type === 'kids'}<small lang={messageLocale('tonight_kids')}>{m.tonight_kids()}</small>{/if}
                </button>
            {/each}
        </div>
        {#if deviceCanRememberProfile}
            <label class="remember">
                <input lang={messageLocale('tonight_remember_profile')} type="checkbox" bind:checked={rememberProfileOnDevice} disabled={!!switchingProfileId} />
                <span lang={messageLocale('tonight_remember_profile')}>{m.tonight_remember_profile()}</span>
            </label>
        {/if}
    {:else}
        <h1 lang={messageLocale('tonight_profiles_unavailable')} id="profile-gate-title" class="tonight-heading">{m.tonight_profiles_unavailable()}</h1>
        <p lang={messageLocale('tonight_profile_retry_description')}>{m.tonight_profile_retry_description()}</p>
        <button lang={messageLocale('tonight_try_again')} class="secondary-action" onclick={onreload}>{m.tonight_try_again()}</button>
    {/if}
    <button lang={messageLocale('tonight_sign_out')} class="signout" onclick={onlogout}>{m.tonight_sign_out()}</button>
</section>

<style>
    .profile-gate { display: flex; flex-direction: column; align-items: center; gap: 1.5rem; margin: clamp(2rem, 10vh, 6rem) auto; max-width: 56rem; text-align: center; }
    h1 { font-size: clamp(2.2rem, 5vw, 3.8rem); }
    p { color: var(--color-text-secondary); }
    .profiles { display: grid; grid-template-columns: repeat(auto-fit, minmax(8rem, 1fr)); width: 100%; gap: 1rem; }
    .profile-gate-option { display: flex; flex-direction: column; align-items: center; gap: 0.75rem; padding: 1.25rem 0.75rem; border: 1px solid var(--color-border); border-radius: var(--radius-lg); background: var(--color-bg-surface); overflow-wrap: anywhere; }
    .profile-gate-option:hover { background: var(--color-bg-hover); border-color: var(--color-accent); }
    .avatar { display: grid; place-items: center; width: 4rem; height: 4rem; border-radius: 50%; background: var(--color-accent-muted); color: var(--color-accent); font-size: 1.6rem; }
    small { color: var(--color-text-secondary); }
    .remember { display: flex; align-items: center; gap: 0.65rem; text-align: start; }
    input { accent-color: var(--color-accent); }
    .signout { min-height: 44px; padding: 0.5rem 1rem; color: var(--color-text-secondary); }
</style>
