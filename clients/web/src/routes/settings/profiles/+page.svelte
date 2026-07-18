<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { onMount } from 'svelte';
    import { listLibraries } from '$lib/api/libraries.js';
    import { createProfile, deleteProfile, listProfiles, updateProfile } from '$lib/api/profiles.js';
    import { currentUser } from '$lib/stores/auth.js';
    import { notifications } from '$lib/stores/notifications.js';

    const RATINGS = ['TV-Y', 'TV-Y7', 'G', 'TV-G', 'PG', 'TV-PG', 'PG-13', 'TV-14', 'R', 'TV-MA', 'NC-17'];

    let profiles = $state([]);
    let libraries = $state([]);
    let loading = $state(true);
    let saving = $state(false);
    let error = $state('');
    let createOpen = $state(false);
    let form = $state(newProfileForm());

    function newProfileForm() {
        return {
            name: '',
            profile_type: 'standard',
            max_content_rating: 'TV-Y7',
            library_ids: [],
            allow_search: true,
            allow_downloads: false,
            allow_external_links: false,
            allow_ambient_channels: true,
            parent_pin: '',
        };
    }

    onMount(load);

    async function load() {
        loading = true;
        error = '';
        try {
            const [profileResponse, libraryResponse] = await Promise.all([
                listProfiles(),
                listLibraries({ page_size: 100 }),
            ]);
            profiles = profileResponse?.items || [];
            libraries = libraryResponse?.items || [];
        } catch (err) {
            error = err.detail || 'Unable to load profiles.';
        } finally {
            loading = false;
        }
    }

    function toggleLibrary(libraryId) {
        form.library_ids = form.library_ids.includes(libraryId)
            ? form.library_ids.filter((id) => id !== libraryId)
            : [...form.library_ids, libraryId];
    }

    async function create() {
        if (!form.name.trim()) {
            error = 'Enter a profile name.';
            return;
        }
        if (form.profile_type === 'kids' && !/^\d{4,12}$/.test(form.parent_pin)) {
            error = 'Enter a 4 to 12 digit parent PIN for this Kids profile.';
            return;
        }
        saving = true;
        error = '';
        try {
            const profile = await createProfile({
                ...form,
                name: form.name.trim(),
                parent_pin: form.profile_type === 'kids' ? form.parent_pin : undefined,
            });
            profiles = [...profiles, profile];
            createOpen = false;
            form = newProfileForm();
            notifications.success('Profile created.');
        } catch (err) {
            error = err.detail || 'Unable to create profile.';
        } finally {
            saving = false;
        }
    }

    async function saveControls(profile) {
        saving = true;
        error = '';
        try {
            const updated = await updateProfile(profile.id, {
                max_content_rating: profile.max_content_rating,
                library_ids: profile.library_ids,
                allow_search: profile.allow_search,
                allow_downloads: profile.allow_downloads,
                allow_external_links: profile.allow_external_links,
                allow_ambient_channels: profile.allow_ambient_channels,
                parent_pin: profile.parent_pin || undefined,
            });
            profiles = profiles.map((item) => item.id === updated.id ? updated : item);
            notifications.success('Parental controls saved.');
        } catch (err) {
            error = err.detail || 'Unable to save parental controls.';
        } finally {
            saving = false;
        }
    }

    async function remove(profile) {
        if (!confirm(`Delete ${profile.name}? Their viewing history will be removed.`)) return;
        saving = true;
        error = '';
        try {
            await deleteProfile(profile.id);
            profiles = profiles.filter((item) => item.id !== profile.id);
            notifications.success('Profile deleted.');
        } catch (err) {
            error = err.detail || 'Unable to delete this profile.';
        } finally {
            saving = false;
        }
    }
</script>

<div class="profiles-page">
    <header class="page-header">
        <div>
            <h1>Profiles</h1>
            <p>Each profile has its own viewing history and recommendations.</p>
        </div>
        <button class="btn-primary" onclick={() => createOpen = !createOpen}>Add profile</button>
    </header>

    {#if error}<p class="error-copy">{error}</p>{/if}

    {#if createOpen}
        <section class="editor-card" aria-labelledby="new-profile-heading">
            <h2 id="new-profile-heading">New profile</h2>
            <div class="form-grid">
                <label>
                    <span>Name</span>
                    <input bind:value={form.name} maxlength="80" placeholder="Profile name" />
                </label>
                <label>
                    <span>Profile type</span>
                    <select bind:value={form.profile_type}>
                        <option value="standard">Standard</option>
                        <option value="kids">Kids</option>
                    </select>
                </label>
            </div>
            {#if form.profile_type === 'kids'}
                <label>
                    <span>Maximum content rating</span>
                    <select bind:value={form.max_content_rating}>
                        {#each RATINGS as rating}<option value={rating}>{rating}</option>{/each}
                    </select>
                </label>
                <fieldset>
                    <legend>Libraries this Kids profile can use</legend>
                    <div class="check-list">
                        {#each libraries as library}
                            <label><input type="checkbox" checked={form.library_ids.includes(library.id)} onchange={() => toggleLibrary(library.id)} /> {library.name}</label>
                        {/each}
                    </div>
                </fieldset>
                <div class="check-list controls">
                    <label><input type="checkbox" bind:checked={form.allow_search} /> Allow search</label>
                    <label><input type="checkbox" bind:checked={form.allow_downloads} /> Allow downloads</label>
                    <label><input type="checkbox" bind:checked={form.allow_external_links} /> Allow external links</label>
                    <label><input type="checkbox" bind:checked={form.allow_ambient_channels} /> Allow Kids channels</label>
                </div>
                <label>
                    <span>Parent PIN</span>
                    <input type="password" inputmode="numeric" pattern="[0-9]*" minlength="4" maxlength="12" bind:value={form.parent_pin} autocomplete="new-password" placeholder="4 to 12 digits" />
                </label>
            {/if}
            <div class="actions"><button class="btn-secondary" onclick={() => createOpen = false}>Cancel</button><button class="btn-primary" onclick={create} disabled={saving}>Create profile</button></div>
        </section>
    {/if}

    {#if loading}
        <p class="state-copy">Loading profiles…</p>
    {:else}
        <div class="profile-grid">
            {#each profiles as profile}
                <section class="profile-card" class:active={profile.id === $currentUser?.active_profile_id}>
                    <div class="avatar">{profile.name?.[0]?.toUpperCase() || 'P'}</div>
                    <div class="profile-heading"><h2>{profile.name}</h2>{#if profile.profile_type === 'kids'}<span>Kids</span>{/if}{#if profile.is_default}<span>Default</span>{/if}</div>
                    {#if profile.profile_type === 'kids'}
                        <div class="kids-controls">
                            <label>Maximum rating <select bind:value={profile.max_content_rating}>{#each RATINGS as rating}<option value={rating}>{rating}</option>{/each}</select></label>
                            <fieldset><legend>Allowed libraries</legend><div class="check-list">{#each libraries as library}<label><input type="checkbox" checked={profile.library_ids.includes(library.id)} onchange={() => profile.library_ids = profile.library_ids.includes(library.id) ? profile.library_ids.filter((id) => id !== library.id) : [...profile.library_ids, library.id]} /> {library.name}</label>{/each}</div></fieldset>
                            <div class="check-list controls"><label><input type="checkbox" bind:checked={profile.allow_search} /> Search</label><label><input type="checkbox" bind:checked={profile.allow_downloads} /> Downloads</label><label><input type="checkbox" bind:checked={profile.allow_external_links} /> External links</label><label><input type="checkbox" bind:checked={profile.allow_ambient_channels} /> Kids channels</label></div>
                            <label>Parent PIN <input type="password" inputmode="numeric" pattern="[0-9]*" minlength="4" maxlength="12" bind:value={profile.parent_pin} autocomplete="new-password" placeholder={profile.parent_pin_configured ? 'Replace PIN (optional)' : 'Set a 4 to 12 digit PIN'} /></label>
                            <button class="btn-secondary" onclick={() => saveControls(profile)} disabled={saving}>Save controls</button>
                        </div>
                    {:else}
                        <p class="profile-copy">Standard profile with separate watch history, favorites, and ratings.</p>
                    {/if}
                    {#if !profile.is_default && profile.id !== $currentUser?.active_profile_id}<button class="delete-button" onclick={() => remove(profile)} disabled={saving}>Delete profile</button>{/if}
                </section>
            {/each}
        </div>
    {/if}
</div>

<style>
    .profiles-page { max-width: 1040px; display: grid; gap: 1.5rem; }
    .page-header { display: flex; align-items: center; justify-content: space-between; gap: 1rem; }
    h1, h2, p { margin: 0; }
    h1 { font-size: 1.6rem; } .page-header p, .profile-copy, .state-copy { color: var(--color-text-secondary); }
    .profile-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(270px, 1fr)); gap: 1rem; }
    .profile-card, .editor-card { border: 1px solid var(--color-border-subtle); border-radius: var(--radius-md); background: var(--color-bg-surface); padding: 1rem; }
    .profile-card.active { border-color: var(--color-accent); box-shadow: 0 0 0 1px var(--color-accent-muted); }
    .avatar { display: grid; width: 58px; height: 58px; place-items: center; border-radius: 50%; background: var(--color-accent-muted); color: var(--color-accent); font-size: 1.35rem; font-weight: 700; }
    .profile-heading { display: flex; align-items: center; gap: 0.5rem; margin: 0.8rem 0; } .profile-heading span { border-radius: 999px; padding: 0.15rem 0.45rem; background: var(--color-bg-elevated); color: var(--color-text-muted); font-size: 0.7rem; }
    .editor-card, .kids-controls { display: grid; gap: 0.9rem; } .form-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 0.75rem; }
    label { display: grid; gap: 0.35rem; color: var(--color-text-secondary); font-size: 0.85rem; } input, select { border: 1px solid var(--color-border); border-radius: var(--radius-sm); padding: 0.55rem 0.65rem; color: var(--color-text-primary); background: var(--color-bg-elevated); font: inherit; }
    fieldset { margin: 0; border: 1px solid var(--color-border-subtle); border-radius: var(--radius-sm); } legend { color: var(--color-text-secondary); font-size: 0.8rem; } .check-list { display: grid; gap: 0.45rem; } .check-list label { display: flex; align-items: center; gap: 0.45rem; } .controls { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .actions { display: flex; justify-content: flex-end; gap: 0.75rem; } .delete-button { margin-top: 1rem; color: var(--color-error); } .error-copy { color: var(--color-error); }
    @media (max-width: 600px) { .page-header, .form-grid { grid-template-columns: 1fr; display: grid; } .controls { grid-template-columns: 1fr; } }
</style>
