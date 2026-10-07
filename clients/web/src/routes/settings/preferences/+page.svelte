<script>
    import { onMount, tick } from 'svelte';
    import { beforeNavigate, goto } from '$app/navigation';
    import { page } from '$app/stores';
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    import { getLocale } from '$lib/paraglide/runtime.js';
    import { getServerOrigin } from '$lib/api/core.js';
    import { currentUser } from '$lib/stores/auth.js';
    import { viewingPreferences } from '$lib/stores/viewing-preferences.js';
    import { navigationGuard } from '$lib/navigation/guard.js';
    import { PREFERENCE_LANGUAGE_CODES, preferencesEqual } from '$lib/preferences/model.js';
    import { devicePreferencesEqual } from '$lib/preferences/device.js';

    let documentTitle = $derived(messageLocale('tonight_preferences_title') === getLocale() ? m.tonight_preferences_title() : m.routes_settings_page_settings());

    let mounted = $state(false);
    let draftProfile = $state(null);
    let baselineProfile = $state(null);
    let draftDevice = $state(null);
    let baselineDevice = $state(null);
    let saving = $state(false);
    let errorText = $state('');
    let statusText = $state('');
    let statusLocale = $state('en');
    let errorLocale = $state('en');
    let discardDialog = $state();
    let requestedScopeKey = null;
    let appliedScopeKey = null;
    let appliedProfile = null;
    let appliedDevice = null;
    let resolveDiscard = null;
    let discardInvoker = null;

    const activeProfileId = $derived($currentUser?.active_profile_id);
    const profileDirty = $derived(!preferencesEqual(draftProfile, baselineProfile));
    const deviceDirty = $derived(!!draftDevice && !devicePreferencesEqual(draftDevice, baselineDevice));
    const dirty = $derived(!!draftProfile && (profileDirty || deviceDirty));
    const scopeResolved = $derived(
        $viewingPreferences.profileId === activeProfileId
        && !!$viewingPreferences.preferences && !!draftProfile && !!draftDevice,
    );
    const busy = $derived(saving || $viewingPreferences.status === 'saving');
    const languageOptions = $derived.by(() => {
        const codes = new Set(PREFERENCE_LANGUAGE_CODES);
        if (draftProfile?.audio_language) codes.add(draftProfile.audio_language);
        if (draftProfile?.subtitle_language) codes.add(draftProfile.subtitle_language);
        let names;
        try { names = new Intl.DisplayNames([getLocale()], { type: 'language' }); } catch { names = null; }
        return [...codes].map((code) => ({ code, label: names?.of(code) || code.toUpperCase() }))
            .sort((first, second) => first.label.localeCompare(second.label, getLocale()));
    });

    onMount(() => {
        mounted = true;
        const unregister = navigationGuard.register({
            isDirty: () => dirty,
            isBusy: () => busy,
            confirmDiscard,
            discard: discardChanges,
        });
        return () => {
            mounted = false;
            unregister();
            resolveDiscard?.(false);
            resolveDiscard = null;
        };
    });

    $effect(() => {
        if (!mounted) return;
        const user = $currentUser;
        if (!user?.id || !user.active_profile_id || user.profile_selection_required) return;
        const serverOrigin = getServerOrigin() || $page.url.origin;
        const key = JSON.stringify([serverOrigin, user.id, user.active_profile_id]);
        if (requestedScopeKey === key) return;
        requestedScopeKey = key;
        errorText = '';
        statusText = '';
        viewingPreferences.load(user.active_profile_id, { serverOrigin, userId: user.id })
            .catch((error) => {
                if (mounted && requestedScopeKey === key && error.name !== 'AbortError') {
                    errorText = m.tonight_preferences_load_failed();
                    errorLocale = messageLocale('tonight_preferences_load_failed');
                }
            });
    });

    $effect(() => {
        const state = $viewingPreferences;
        if (state.profileId !== activeProfileId || !state.preferences || !state.devicePreferences) {
            appliedScopeKey = null;
            appliedProfile = null;
            appliedDevice = null;
            draftProfile = null;
            baselineProfile = null;
            draftDevice = null;
            baselineDevice = null;
            return;
        }
        const scopeChanged = appliedScopeKey !== state.scope.key;
        if (scopeChanged || appliedProfile !== state.preferences) {
            const keepDraft = !scopeChanged && profileDirty && !saving;
            baselineProfile = { ...state.preferences };
            if (!keepDraft) draftProfile = { ...state.preferences };
            appliedProfile = state.preferences;
        }
        if (scopeChanged || appliedDevice !== state.devicePreferences) {
            const keepDraft = !scopeChanged && deviceDirty && !saving;
            baselineDevice = { ...state.devicePreferences };
            if (!keepDraft) draftDevice = { ...state.devicePreferences };
            appliedDevice = state.devicePreferences;
        }
        appliedScopeKey = state.scope.key;
    });

    beforeNavigate((navigation) => {
        if (!dirty && !busy) return;
        if (navigation.willUnload) {
            navigation.cancel();
            return;
        }
        const destination = navigation.to?.url;
        if (!destination) return;
        if (destination.pathname === $page.url.pathname && destination.search === $page.url.search) return;
        navigation.cancel();
        const delta = navigation.type === 'popstate' ? navigation.delta : null;
        navigationGuard.requestNavigationTransition().then(async (allowed) => {
            if (!allowed || !mounted) return;
            if (delta !== null) {
                window.history.go(delta);
            } else {
                try { await goto(destination); } catch { errorText = m.tonight_preferences_load_failed();
            errorLocale = messageLocale('tonight_preferences_load_failed'); }
            }
        });
    });

    async function retryLoad() {
        errorText = '';
        const user = $currentUser;
        if (!user?.active_profile_id || !user.id) return;
        try {
            await viewingPreferences.load(user.active_profile_id, {
                serverOrigin: getServerOrigin() || $page.url.origin,
                userId: user.id,
            }, { force: true });
        } catch (error) {
            if (error.name !== 'AbortError') errorText = m.tonight_preferences_load_failed();
            errorLocale = messageLocale('tonight_preferences_load_failed');
        }
    }

    async function saveChanges(event) {
        event.preventDefault();
        if (!scopeResolved || busy) return;
        const profileId = activeProfileId;
        const scopeKey = $viewingPreferences.scope.key;
        const profileValue = { ...draftProfile };
        const deviceValue = { ...draftDevice };
        const needsProfileSave = profileDirty || !$viewingPreferences.hasSavedPreferences;
        const needsDeviceSave = deviceDirty;
        let profileSaved = false;
        saving = true;
        errorText = '';
        statusText = m.tonight_preferences_saving();
        statusLocale = messageLocale('tonight_preferences_saving');
        try {
            if (needsProfileSave) {
                const result = await viewingPreferences.save(profileId, profileValue);
                if ($viewingPreferences.scope?.key !== scopeKey || activeProfileId !== profileId) return;
                baselineProfile = { ...result.preferences };
                draftProfile = { ...result.preferences };
                profileSaved = true;
            }
            if ($viewingPreferences.scope?.key !== scopeKey || activeProfileId !== profileId) return;
            if (needsDeviceSave) {
                const result = viewingPreferences.saveDevice(profileId, deviceValue);
                baselineDevice = { ...result };
                draftDevice = { ...result };
            }
            statusText = m.tonight_preferences_saved();
            statusLocale = messageLocale('tonight_preferences_saved');
        } catch (error) {
            if (activeProfileId !== profileId || error.name === 'AbortError') return;
            statusText = '';
            errorText = profileSaved ? m.tonight_preferences_device_save_failed()
                : error.detail || m.tonight_preferences_save_failed();
            errorLocale = profileSaved ? messageLocale('tonight_preferences_device_save_failed') : error.detail ? 'en' : messageLocale('tonight_preferences_save_failed');
            await tick();
            document.getElementById('preferences-error')?.focus();
        } finally {
            saving = false;
        }
    }

    function discardChanges() {
        if (!baselineProfile || !baselineDevice) return;
        draftProfile = { ...baselineProfile };
        draftDevice = { ...baselineDevice };
        errorText = '';
        statusText = m.tonight_preferences_discarded();
        statusLocale = messageLocale('tonight_preferences_discarded');
    }

    function confirmDiscard() {
        return new Promise((resolve) => {
            resolveDiscard = resolve;
            discardInvoker = document.activeElement;
            discardDialog.returnValue = 'keep';
            discardDialog.showModal();
            discardDialog.querySelector('button[value="keep"]')?.focus();
        });
    }

    function closeDiscardDialog() {
        const allowed = discardDialog.returnValue === 'discard';
        resolveDiscard?.(allowed);
        resolveDiscard = null;
        if (!allowed && !discardInvoker?.isConnected) {
            document.getElementById('preferences-title')?.focus();
        }
        discardInvoker = null;
    }

    function changeQuality(event) {
        draftDevice.quality_mode = event.currentTarget.value;
        draftDevice.max_streaming_bitrate = draftDevice.quality_mode === 'manual'
            ? draftDevice.max_streaming_bitrate || 6000000 : null;
    }
</script>

<svelte:head><title>{documentTitle} · Duskcue</title></svelte:head>

<div class="viewing-preferences">
    <header>
        <h1 lang={messageLocale('tonight_preferences_title')} id="preferences-title" tabindex="-1">{m.tonight_preferences_title()}</h1>
        <p lang={messageLocale('tonight_preferences_description')}>{m.tonight_preferences_description()}</p>
    </header>
    {#if errorText || $viewingPreferences.status === 'error'}
        <p lang={errorText ? errorLocale : messageLocale('tonight_preferences_load_failed')} id="preferences-error" class="error" role="alert" tabindex="-1">{errorText || m.tonight_preferences_load_failed()}</p>
    {/if}
    <p lang={statusLocale} class="status" role="status" aria-live="polite">{statusText}</p>

    {#if !scopeResolved}
        {#if $viewingPreferences.status === 'error'}
            <button lang={messageLocale('tonight_preferences_retry')} type="button" class="secondary" onclick={retryLoad}>{m.tonight_preferences_retry()}</button>
        {:else}
            <p lang={messageLocale('tonight_preferences_loading')} class="loading">{m.tonight_preferences_loading()}</p>
        {/if}
    {:else}
        <form onsubmit={saveChanges} oninput={() => statusText = ''} aria-busy={busy}>
            <fieldset disabled={busy}>
                <legend lang={messageLocale('tonight_preferences_profile')}>{m.tonight_preferences_profile()}</legend>
                {#if $viewingPreferences.legacyAutoplayOff}
                    <p lang={messageLocale('tonight_preferences_legacy_off')} class="notice">{m.tonight_preferences_legacy_off()}</p>
                {/if}
                <label class="checkbox-label" for="pref-autoplay">
                    <input lang={messageLocale('tonight_preferences_autoplay')} id="pref-autoplay" name="autoplay_next_episode" type="checkbox" bind:checked={draftProfile.autoplay_next_episode} aria-describedby="pref-autoplay-help" />
                    <span lang={messageLocale('tonight_preferences_autoplay')}>{m.tonight_preferences_autoplay()}</span>
                </label>
                <p lang={messageLocale('tonight_preferences_autoplay_help')} id="pref-autoplay-help" class="hint">{m.tonight_preferences_autoplay_help()}</p>
                <label lang={messageLocale('tonight_preferences_audio')} for="pref-audio">{m.tonight_preferences_audio()}</label>
                <select lang={messageLocale('tonight_preferences_audio')} id="pref-audio" name="audio_language" value={draftProfile.audio_language || ''} onchange={(event) => draftProfile.audio_language = event.currentTarget.value || null} aria-describedby="pref-audio-help">
                    <option lang={messageLocale('tonight_preferences_source_default')} value="">{m.tonight_preferences_source_default()}</option>
                    {#each languageOptions as language (language.code)}<option lang={getLocale()} value={language.code}>{language.label}</option>{/each}
                </select>
                <p lang={messageLocale('tonight_preferences_audio_help')} id="pref-audio-help" class="hint">{m.tonight_preferences_audio_help()}</p>
                <label class="checkbox-label" for="pref-description">
                    <input lang={messageLocale('tonight_preferences_audio_description')} id="pref-description" name="prefer_audio_description" type="checkbox" bind:checked={draftProfile.prefer_audio_description} aria-describedby="pref-description-help" />
                    <span lang={messageLocale('tonight_preferences_audio_description')}>{m.tonight_preferences_audio_description()}</span>
                </label>
                <p lang={messageLocale('tonight_preferences_audio_description_help')} id="pref-description-help" class="hint">{m.tonight_preferences_audio_description_help()}</p>
                <label lang={messageLocale('tonight_preferences_subtitles')} for="pref-subtitles">{m.tonight_preferences_subtitles()}</label>
                <select lang={messageLocale('tonight_preferences_subtitles')} id="pref-subtitles" name="subtitle_mode" bind:value={draftProfile.subtitle_mode}>
                    <option lang={messageLocale('tonight_preferences_subtitles_off')} value="none">{m.tonight_preferences_subtitles_off()}</option>
                    <option lang={messageLocale('tonight_preferences_subtitles_preferred')} value="always">{m.tonight_preferences_subtitles_preferred()}</option>
                </select>
                <label lang={messageLocale('tonight_preferences_subtitle_language')} for="pref-subtitle-language">{m.tonight_preferences_subtitle_language()}</label>
                <select lang={messageLocale('tonight_preferences_subtitle_language')} id="pref-subtitle-language" name="subtitle_language" value={draftProfile.subtitle_language || ''} onchange={(event) => draftProfile.subtitle_language = event.currentTarget.value || null} disabled={draftProfile.subtitle_mode === 'none'} required={draftProfile.subtitle_mode === 'always'}>
                    <option lang={messageLocale('tonight_preferences_choose_language')} value="">{m.tonight_preferences_choose_language()}</option>
                    {#each languageOptions as language (language.code)}<option lang={getLocale()} value={language.code}>{language.label}</option>{/each}
                </select>
                <label class="checkbox-label" for="pref-sdh">
                    <input lang={messageLocale('tonight_preferences_sdh')} id="pref-sdh" name="prefer_sdh" type="checkbox" bind:checked={draftProfile.prefer_sdh} disabled={draftProfile.subtitle_mode === 'none'} aria-describedby="pref-sdh-help" />
                    <span lang={messageLocale('tonight_preferences_sdh')}>{m.tonight_preferences_sdh()}</span>
                </label>
                <p lang={messageLocale('tonight_preferences_sdh_help')} id="pref-sdh-help" class="hint">{m.tonight_preferences_sdh_help()}</p>
                <p lang={messageLocale('tonight_preferences_fallback')} class="hint">{m.tonight_preferences_fallback()}</p>
            </fieldset>

            <fieldset disabled={busy}>
                <legend lang={messageLocale('tonight_preferences_device')}>{m.tonight_preferences_device()}</legend>
                <label lang={messageLocale('tonight_preferences_quality')} for="pref-quality">{m.tonight_preferences_quality()}</label>
                <select lang={messageLocale('tonight_preferences_quality')} id="pref-quality" name="quality_mode" value={draftDevice.quality_mode} onchange={changeQuality} aria-describedby="pref-quality-help">
                    <option lang={messageLocale('tonight_preferences_quality_auto')} value="auto">{m.tonight_preferences_quality_auto()}</option>
                    <option lang={messageLocale('tonight_preferences_quality_maximum')} value="maximum">{m.tonight_preferences_quality_maximum()}</option>
                    <option lang={messageLocale('tonight_preferences_quality_manual')} value="manual">{m.tonight_preferences_quality_manual()}</option>
                </select>
                {#if draftDevice.quality_mode === 'manual'}
                    <label lang={messageLocale('tonight_preferences_bitrate')} for="pref-bitrate">{m.tonight_preferences_bitrate()}</label>
                    <select lang={messageLocale('tonight_preferences_bitrate')} id="pref-bitrate" name="max_streaming_bitrate" bind:value={draftDevice.max_streaming_bitrate}>
                        {#each [1500000, 3000000, 6000000, 12000000, 20000000, 40000000] as bitrate}
                            <option value={bitrate}>{bitrate / 1000000}</option>
                        {/each}
                    </select>
                {/if}
                <p lang={messageLocale('tonight_preferences_quality_help')} id="pref-quality-help" class="hint">{m.tonight_preferences_quality_help()}</p>
            </fieldset>
            <div class="actions">
                <button lang={busy ? messageLocale('tonight_preferences_saving') : messageLocale('tonight_preferences_save')} type="submit" class="primary" disabled={busy || (!dirty && $viewingPreferences.hasSavedPreferences)}>{busy ? m.tonight_preferences_saving() : m.tonight_preferences_save()}</button>
                <button lang={messageLocale('tonight_preferences_discard')} type="button" class="secondary" disabled={busy || !dirty} onclick={discardChanges}>{m.tonight_preferences_discard()}</button>
            </div>
        </form>
    {/if}

    <aside class="locale-note">
        <a lang={messageLocale('tonight_preferences_locale')} href="/settings">{m.tonight_preferences_locale()}</a>
        <p lang={messageLocale('tonight_preferences_locale_help')}>{m.tonight_preferences_locale_help()}</p>
    </aside>
</div>

<dialog lang={messageLocale('tonight_preferences_unsaved_title')} bind:this={discardDialog} aria-labelledby="discard-title" aria-describedby="discard-copy" onclose={closeDiscardDialog}>
    <h2 lang={messageLocale('tonight_preferences_unsaved_title')} id="discard-title">{m.tonight_preferences_unsaved_title()}</h2>
    <p lang={messageLocale('tonight_preferences_unsaved_body')} id="discard-copy">{m.tonight_preferences_unsaved_body()}</p>
    <form method="dialog" class="actions">
        <button lang={messageLocale('tonight_preferences_keep_editing')} type="submit" class="primary" value="keep">{m.tonight_preferences_keep_editing()}</button>
        <button lang={messageLocale('tonight_preferences_discard_continue')} type="submit" class="secondary" value="discard">{m.tonight_preferences_discard_continue()}</button>
    </form>
</dialog>

<style>
    .viewing-preferences { max-inline-size: 48rem; margin-inline: auto; padding-block: 1rem 4rem; }
    header { margin-block-end: 1.5rem; }
    h1 { font-family: var(--font-display); font-size: clamp(2rem, 5vw, 3rem); font-weight: 400; }
    header p, .locale-note p { color: var(--color-text-secondary); margin-block-start: 0.6rem; line-height: 1.6; }
    fieldset { display: grid; gap: 0.8rem; min-inline-size: 0; margin-block: 1.5rem; padding: 1.5rem; border: 1px solid var(--color-border-subtle); border-radius: var(--radius-md); background: var(--color-bg-surface); }
    legend { padding-inline: 0.5rem; font-size: 1.1rem; color: var(--color-text-primary); }
    label { color: var(--color-text-primary); line-height: 1.5; }
    select { inline-size: 100%; min-block-size: 2.75rem; padding: 0.65rem 0.8rem; border: 1px solid var(--color-border); border-radius: var(--radius-sm); background: var(--color-bg-deep); color: var(--color-text-primary); font: inherit; }
    .checkbox-label { display: flex; align-items: start; gap: 0.8rem; min-block-size: 2.75rem; padding-block: 0.5rem; cursor: pointer; }
    input[type='checkbox'] { flex: 0 0 auto; inline-size: 1.15rem; block-size: 1.15rem; margin-block-start: 0.15rem; accent-color: var(--color-accent); }
    .hint { color: var(--color-text-secondary); font-size: 0.9rem; line-height: 1.6; margin-block-end: 0.6rem; }
    .notice { padding: 0.8rem; background: var(--color-bg-elevated); border-radius: var(--radius-sm); color: var(--color-text-secondary); }
    .actions { display: flex; flex-wrap: wrap; gap: 0.75rem; }
    button { min-block-size: 2.75rem; padding: 0.65rem 1.1rem; border-radius: var(--radius-sm); cursor: pointer; font-weight: 600; }
    .primary { color: var(--color-on-accent); background: var(--color-accent); border: 1px solid var(--color-accent); }
    .secondary { color: var(--color-text-primary); background: var(--color-bg-surface); border: 1px solid var(--color-border); }
    button:disabled, select:disabled { opacity: 0.65; cursor: default; }
    .error { color: var(--color-error); border-inline-start: 3px solid currentColor; padding: 0.75rem 1rem; line-height: 1.6; }
    .status { min-block-size: 1.5rem; color: var(--color-text-secondary); }
    .loading { padding-block: 1.5rem; color: var(--color-text-secondary); }
    .locale-note { margin-block-start: 2rem; padding-block-start: 1rem; border-block-start: 1px solid var(--color-border-subtle); }
    .locale-note a { color: var(--color-accent); text-underline-offset: 0.2em; }
    dialog { max-inline-size: min(30rem, calc(100vw - 2rem)); max-block-size: 90dvh; overflow: auto; padding: 1.5rem; background: var(--color-bg-elevated); color: var(--color-text-primary); border: 1px solid var(--color-border); border-radius: var(--radius-md); margin: auto; }
    dialog::backdrop { background: rgb(0 0 0 / 65%); }
    dialog h2 { font-family: var(--font-display); font-weight: 400; font-size: 1.8rem; }
    dialog p { margin-block: 1rem 1.5rem; line-height: 1.6; color: var(--color-text-secondary); }
    @media (max-width: 480px) { fieldset { padding: 1rem; } .actions button { flex: 1 1 auto; } }
</style>
